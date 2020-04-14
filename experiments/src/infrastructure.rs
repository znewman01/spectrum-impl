///! Infrastructure for running Spectrum experiments.
///!
///! At a high level, we have the following machine types:
///! - one publisher, running the `publisher` binary and `etcd` (on localhost:2379)
///! - many workers, running `worker` binaries (and possibly `leader` binaries)
///! - many client machines, each running many clients
///!
///! We manage services with systemd on Ubuntu and proxy traffic through Nginx
///! (which will handle TLS (TODO)).
///!
///! Services should bind to a local network interface (ports 6000-6999) and
///! advertise themselves with their AWS public DNS and an Nginx-managed port
///! (the equivalent port in the 5000-5999 range), which does a simple
///! proxy_pass through.
///!
///! We use the following default port values:
///! - etcd: 2379
///! - leader: 6001
///! - publisher: 6002
///! - worker: 6100--...
// TODO: etcd security (fine for now because of intense AWS firewalling)
// TODO: TLS (on Nginx)
use crate::compile::{download_s3, install_aws_cli, install_rust};
use crate::config::Environment;

use failure::{format_err, Error};
use rusoto_core::Region;
use sessh::Session;
use slog::{o, trace, Logger};
use tsunami::providers::aws;
use tsunami::TsunamiBuilder;

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

type Result<T> = std::result::Result<T, Error>;

pub const ETCD_PUBLIC_PORT: u16 = 2379;

/// Installs a config file with given contents to a remote host (as root).
pub fn install_config_file(
    _log: &Logger,
    ssh: &mut Session,
    config: String,
    dest: &Path,
) -> Result<()> {
    let mut file = tempfile::NamedTempFile::new()?;
    file.write_all(config.as_bytes())?;

    let file_name = dest
        .file_name()
        .ok_or_else(|| format_err!("Couldn't get config file base name"))?;
    ssh.upload(file.path(), Path::new(file_name))?;
    ssh.cmd(&format!(
        "sudo mv {} {}",
        file_name.to_string_lossy(),
        dest.to_string_lossy()
    ))?;

    Ok(())
}

// drop in `/etc/systemd/system/spectrum-{}.service`
fn install_systemd_service_unit(
    log: &Logger,
    ssh: &mut Session,
    service: String,
    binary: &Path,
    port: u16,
    public_address: String,
) -> Result<()> {
    let vars = vec![
        ("service".to_string(), service.clone()),
        ("binary".to_string(), binary.to_string_lossy().to_string()),
        ("port".to_string(), port.to_string()),
        ("public_address".to_string(), public_address),
    ]
    .into_iter()
    .collect();
    let config = envsubst::substitute(include_str!("data/spectrum.service.template"), &vars)?;
    trace!(log, "Formatted systemd service");
    let path = Path::new("/etc/systemd/system/").join(format!("spectrum-{}.service", service));
    install_config_file(log, ssh, config, &path)?;

    Ok(())
}

fn install_systemd_worker_unit(log: &Logger, ssh: &mut Session, hostname: String) -> Result<()> {
    let vars = vec![("hostname".to_string(), hostname)]
        .into_iter()
        .collect();
    let config = envsubst::substitute(include_str!("data/worker@.service.template"), &vars)?;
    trace!(log, "Formatted systemd worker unit");
    let path = Path::new("/etc/systemd/system/spectrum-worker@.service");
    install_config_file(log, ssh, config, &path)?;

    Ok(())
}

/// Note that we can run many clients using the same unit file; see:
/// https://serverfault.com/questions/730239/start-n-processes-with-one-systemd-service-file

/// Install an Nginx config to proxy_pass traffic from external_port to localhost_port.
fn install_nginx_conf(
    log: &Logger,
    ssh: &mut Session,
    external_port: u16,
    localhost_port: u16,
) -> Result<()> {
    // TODO: TLS
    let vars = vec![
        ("external".to_string(), external_port.to_string()),
        ("internal".to_string(), localhost_port.to_string()),
    ]
    .into_iter()
    .collect();
    let config = envsubst::substitute(include_str!("data/nginx.conf.template"), &vars)?;
    trace!(log, "Formatted nginx config");
    let path = Path::new("/etc/nginx/conf.d/")
        .join(format!("proxy-{}-{}.conf", external_port, localhost_port));

    install_config_file(log, ssh, config, &path)?;

    Ok(())
}

// TODO: break into separate methods for machine type
fn install_spectrum(
    log: &Logger,
    ssh: &mut Session,
    s3_object: &str,
    worker_processes_per_machine: u16,
) -> Result<()> {
    const ARCHIVE_NAME: &str = "spectrum.tar.gz";

    // not necessary to install all rust dependencies but probably sufficient
    install_rust(log, ssh)?;
    ssh.cmd("sudo apt install -y nginx")?;
    install_config_file(
        log,
        ssh,
        include_str!("data/nginx.conf").to_string(),
        &Path::new("/etc/nginx/nginx.conf"),
    )?;
    install_config_file(
        log,
        ssh,
        include_str!("data/nginx.conf").to_string(),
        &Path::new("/etc/sysctl.d/20-spectrum.conf"),
    )?;
    ssh.cmd("sudo sysctl --system")?;

    install_aws_cli(log, ssh)?;
    download_s3(log, ssh, Path::new(ARCHIVE_NAME), s3_object)?;
    ssh.cmd(&format!("tar -xzf {}", ARCHIVE_NAME))?;

    let (hostname, _) = ssh.cmd("ec2metadata --local-hostname")?;
    let hostname = hostname.trim();
    trace!(log, "Got private hostname: {}", hostname);
    for (service, port) in vec!["leader", "publisher"].into_iter().zip(6000..6100) {
        let external_port = port - 1000; // skip NGINX
        install_nginx_conf(log, ssh, external_port, port)?;
        let public_addr = format!("{}:{}", hostname, external_port);
        let binary = Path::new("/home/ubuntu/spectrum").join(service);
        install_systemd_service_unit(log, ssh, service.to_string(), &binary, port, public_addr)?;
    }

    for idx in 1..=worker_processes_per_machine {
        let external_port = 5100 + idx;
        let port = 6100 + idx;
        install_nginx_conf(log, ssh, external_port, port)?;
    }
    install_systemd_worker_unit(log, ssh, hostname.to_string())?;

    install_config_file(
        log,
        ssh,
        include_str!("data/viewer@.service.template").to_string(),
        &Path::new("/etc/systemd/system/viewer@.service"),
    )?;
    install_config_file(
        log,
        ssh,
        include_str!("data/broadcaster@.service.template").to_string(),
        &Path::new("/etc/systemd/system/broadcaster@.service"),
    )?;

    ssh.cmd("sudo nginx -s reload")?;
    ssh.cmd("sudo systemctl daemon-reload")?;

    Ok(())
}

pub fn setup<H: std::hash::BuildHasher>(
    log: &Logger,
    s3_binaries: &HashMap<String, String, H>,
    environment: Environment,
) -> Result<TsunamiBuilder<aws::Setup>> {
    let mut tsunami = TsunamiBuilder::default();
    tsunami.set_logger(log.new(o!("tsunami" => "experiment")));

    let machine_types = environment.machine_types.clone();
    let base_ami = environment.base_ami.clone();

    {
        let machine_type = machine_types.publisher.instance_type.clone();
        let s3_binary = s3_binaries[&machine_type].clone();
        let workers_per_machine = environment.workers_per_machine;
        let base_ami = base_ami.clone();
        tsunami.add(
            "publisher",
            aws::Setup::default()
                .region(Region::UsEast2)
                .ami(base_ami)
                .username("ubuntu")
                .instance_type(machine_type)
                .setup(move |ssh, log| {
                    install_spectrum(log, ssh, &s3_binary, workers_per_machine)?;
                    ssh.cmd("sudo apt install -y etcd-server etcd-client")?;
                    let (hostname, _) = ssh.cmd("ec2metadata --local-hostname")?;
                    let hostname = hostname.trim();
                    let etcd_config = format!(
                        "\
                        ETCD_LISTEN_CLIENT_URLS=http://0.0.0.0:2379\n\
                        ETCD_ADVERTISE_CLIENT_URLS=http://{}:2379\n\
                        ",
                        hostname
                    );
                    install_config_file(log, ssh, etcd_config, Path::new("/etc/default/etcd"))?;
                    ssh.cmd("sudo systemctl restart etcd")?;
                    Ok(())
                }),
        )?;
    }

    {
        let machine_type = machine_types.worker.instance_type.clone();
        let s3_binary = s3_binaries[&machine_type].clone();
        let workers_per_machine = environment.workers_per_machine;
        let base_ami = base_ami.clone();
        tsunami.add_multiple(
            environment.worker_machines.into(),
            "worker",
            aws::Setup::default()
                .region(Region::UsEast2)
                .ami(base_ami)
                .username("ubuntu")
                .instance_type(machine_type)
                .setup(move |ssh, log| {
                    install_spectrum(log, ssh, &s3_binary, workers_per_machine)?;
                    Ok(())
                }),
        )?;
    }

    {
        let machine_type = machine_types.client.instance_type;
        let s3_binary = s3_binaries[&machine_type].clone();
        let workers_per_machine = environment.workers_per_machine;
        tsunami.add_multiple(
            environment.client_machines.into(),
            "client",
            aws::Setup::default()
                .region(Region::UsEast2)
                .ami(base_ami)
                .username("ubuntu")
                .instance_type(machine_type)
                .setup(move |ssh, log| {
                    install_spectrum(log, ssh, &s3_binary, workers_per_machine)?;
                    Ok(())
                }),
        )?;
    }

    Ok(tsunami)
}
