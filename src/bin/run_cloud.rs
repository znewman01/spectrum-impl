use spectrum_impl::experiment::{compile, config, infrastructure};

use clap::{crate_authors, crate_version, Clap};
use failure::Error;
use serde::ser::{SerializeSeq, Serializer};
use std::path::PathBuf;
use tsunami::providers::{aws, Launcher};

use std::collections::HashSet;
use std::convert::TryInto;
use std::fs::File;
use std::io;
use std::path::Path;
use std::time::Duration;

const BASE_AMI: &str = "ami-0fc20dd1da406780b";

type Result<T> = std::result::Result<T, Error>;

/// Spectrum -- driver for cloud-based experiments.
///
/// Runs the whole protocol on AWS defined-duration spot instances.
///
/// Uses typical AWS `$AWS_ACCESS_KEY_ID`, `$AWS_SECRET_ACCESS_KEY` environment
/// variables for authentication.
#[derive(Clap, Clone)]
#[clap(version = crate_version!(), author = crate_authors!())]
struct Args {
    /// Path to a JSON file containing the experiments to run.
    ///
    /// The file should contain a list of input records; an example is
    /// distributed with the source of this utility (`experiments.json`).
    #[clap()]
    experiments_file: String,

    /// Path to a directory where compiled binaries should be stored.
    #[clap(long)]
    binary_dir: PathBuf,

    /// Whether to compile the binaries in debug mode.
    ///
    /// This is much faster but the performance of the ultimate binaries workse.
    #[clap(long)]
    debug: bool,
}

impl Args {
    fn profile(&self) -> compile::Profile {
        if self.debug {
            compile::Profile::Debug
        } else {
            compile::Profile::Release
        }
    }
}

fn init_logger() -> slog::Logger {
    use slog::o;
    use slog::Drain;
    use std::sync::Mutex;

    let decorator = slog_term::TermDecorator::new().build();
    let drain = Mutex::new(slog_term::FullFormat::new(decorator).build()).fuse();
    slog::Logger::root(drain, o!())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let log = init_logger();

    let experiments: Vec<config::Experiment>;
    {
        let file = File::open(args.experiments_file.clone())?;
        experiments = serde_json::from_reader(file)?;
    }

    let machine_types: HashSet<String> = experiments
        .iter()
        .flat_map(|e| e.instance_types())
        .collect();
    let src_dir = PathBuf::from("/home/zjn/git/spectrum-impl/");
    // TODO: Store binaries on EBS or something! uploading takes a long time.
    let bin_archives = compile::compile(
        &log,
        args.binary_dir.clone(),
        src_dir,
        machine_types.into_iter().collect(),
        args.profile(),
        BASE_AMI.to_string(),
    )
    .await?;

    // Stream the results to STDOUT.
    let mut serializer = serde_json::Serializer::pretty(io::stdout());
    let mut seq = serializer.serialize_seq(None)?;

    let experiment_sets = config::Experiment::by_environment(experiments);
    for (environment, experiments) in experiment_sets {
        slog::debug!(&log, "Setting up a new environment!");
        slog::trace!(&log, "Env: {:?}", environment);

        // TODO: Performance optimizations
        // - make our own AMI
        // - many rounds

        let tsunami = infrastructure::setup(&log, &bin_archives, environment)?;
        let mut aws_launcher = aws::OnDemandLauncher::default();
        tsunami.spawn(&mut aws_launcher)?;
        // vms[_].ssh is guaranteed to be populated by this point, so we can
        // unwrap
        let mut vms = aws_launcher.connect_all()?;

        for experiment in experiments {
            slog::debug!(&log, "Running a new experiment");
            slog::trace!(&log, "Experiment: {:?}", experiment);

            // Set up etcd
            let etcd_hostname = {
                let etcd = &vms["publisher"];
                let (hostname, _) = etcd
                    .ssh
                    .as_ref()
                    .unwrap()
                    .cmd("ec2metadata --local-hostname")?;
                hostname.trim().to_string()
            };
            let etcd_env = format!(
                "SPECTRUM_CONFIG_SERVER=etcd://{}:{}",
                etcd_hostname,
                infrastructure::ETCD_PUBLIC_PORT
            );

            {
                slog::trace!(
                    &log, "Writing experiment config to etcd using `setup` binary.";
                    "etcd_env" => &etcd_env
                );
                let publisher = &vms["publisher"];
                let protocol_flag = match experiment.protocol {
                    config::Protocol::Symmetric { security } => format!("--security {}", security),
                    config::Protocol::Insecure { .. } => "--no-security".to_string(),
                    config::Protocol::SeedHomomorphic { .. } => unimplemented!(),
                };
                publisher.ssh.as_ref().unwrap().cmd(&format!(
                    "\
                    {etcd_env} \
                    $HOME/spectrum/setup \
                        {protocol} \
                        --channels {channels} \
                        --clients {clients} \
                        --group-size {group_size} \
                        --message-size {message_size}\
                    ",
                    etcd_env = &etcd_env,
                    protocol = protocol_flag,
                    channels = experiment.channels,
                    clients = experiment.clients,
                    group_size = experiment.group_size,
                    message_size = experiment.message_size,
                ))?;
                // TODO: download key files
            }

            let workers = vms.iter_mut().filter(|(l, _)| l.starts_with("worker-"));
            for (id, (_, worker)) in workers.enumerate() {
                let group = id / (experiment.group_size as usize);
                let idx = id % (experiment.group_size as usize);

                let spectrum_conf = vec![
                    etcd_env.clone(),
                    format!("SPECTRUM_WORKER_GROUP={}", group + 1),
                    format!("SPECTRUM_LEADER_GROUP={}", group + 1),
                    format!("SPECTRUM_WORKER_INDEX={}", idx + 1),
                ]
                .join("\n");
                slog::trace!(
                    &log, "Starting worker";
                    "group" => group, "index" => idx, "config" => &spectrum_conf
                );
                infrastructure::install_config_file(
                    &log,
                    worker.ssh.as_mut().unwrap(),
                    spectrum_conf,
                    Path::new("/etc/spectrum.conf"),
                )?;

                worker
                    .ssh
                    .as_ref()
                    .unwrap()
                    .cmd("sudo systemctl start spectrum-worker")?;
                if idx == 0 {
                    slog::trace!(&log, "Starting leader too!"; "group" => group);
                    worker
                        .ssh
                        .as_ref()
                        .unwrap()
                        .cmd("sudo systemctl start spectrum-leader")?;
                }
            }

            let clients = vms.iter_mut().filter(|(l, _)| l.starts_with("client-"));
            for (id, (_label, client)) in clients.enumerate() {
                let clients: u32 = experiment.clients;
                let clients_per_machine = experiment.clients_per_machine;
                // max number on every machine but the last
                let num_clients: u32 = clients - (id as u32) * (clients_per_machine as u32);
                let num_clients: u16 = std::cmp::min(num_clients, clients_per_machine.into())
                    .try_into()
                    .unwrap();

                let spectrum_conf = vec![etcd_env.clone()].join("\n");
                infrastructure::install_config_file(
                    &log,
                    client.ssh.as_mut().unwrap(),
                    spectrum_conf.clone(),
                    Path::new("/etc/spectrum.conf"),
                )?;

                // start at 1 because CLI doesn't like 0 input
                let start_idx = id * (clients_per_machine as usize) + 1;
                slog::trace!(
                    &log, "Starting client simulator.";
                    "start_idx" => start_idx, "num_clients" => num_clients,
                    "id" => id, "config" => &spectrum_conf
                );
                client.ssh.as_ref().unwrap().cmd(&format!(
                    "sudo systemctl start viewer@{{{}..{}}}",
                    start_idx,
                    start_idx + (num_clients as usize) - 1
                ))?;
            }

            let time_millis: u64 = {
                let publisher = vms.get_mut("publisher").unwrap();
                let spectrum_conf = vec![etcd_env.clone()].join("\n");
                infrastructure::install_config_file(
                    &log,
                    publisher.ssh.as_mut().unwrap(),
                    spectrum_conf,
                    Path::new("/etc/spectrum.conf"),
                )?;

                let ssh = publisher.ssh.as_ref().unwrap();
                ssh.cmd("sudo systemctl start spectrum-publisher --wait")?;
                slog::trace!(&log, "Publisher exited successfully.");

                // output of publisher: "Elapsed time: 100ms"
                let (time_millis, _) = ssh.cmd(
                    "\
                    journalctl --unit spectrum-publisher \
                    | grep -o 'Elapsed time: .*' \
                    | sed 's/Elapsed time: \\(.*\\)ms/\\1/'",
                )?;
                slog::trace!(&log, "Got time: [{}]", time_millis);
                // don't let this same output confuse us if we run on this
                // machine again
                ssh.cmd("sudo journalctl --rotate")?;
                ssh.cmd("sudo journalctl --vacuum-time=1s")?;
                time_millis.trim().parse().unwrap()
            };

            let time = Duration::from_millis(time_millis);

            let result = config::Result::new(experiment, time);
            seq.serialize_element(&result)?;

            slog::trace!(&log, "Shutting down everything");
            for (label, vm) in &vms {
                let ssh = vm.ssh.as_ref().unwrap();
                ssh.cmd("sudo systemctl stop spectrum-leader")?;
                ssh.cmd("sudo systemctl stop spectrum-worker")?;
                if label == "publisher" {
                    slog::trace!(&log, "Cleaning out etcd");
                    ssh.cmd("ETCDCTL_API=3 etcdctl --endpoints localhost:2379 del --prefix \"\"")?;
                }
            }
        }
    }
    seq.end()?;

    Ok(())
}
