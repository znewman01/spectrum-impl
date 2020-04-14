use crate::infrastructure::install_config_file;

use failure::Error;
use itertools::Itertools;
use rusoto_core::Region;
use rusoto_s3::{S3Client, S3};
use sessh::Session;
use slog::{debug, o, trace, Logger};
use tokio::process::Command;
use tsunami::providers::aws;
use tsunami::providers::Launcher;
use tsunami::TsunamiBuilder;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, Error>;

const PROJECT_S3_BUCKET: &str = "hornet-spectrum";

/// Install Rust and dependencies over the given SSH session.
///
/// Installs (using apt) all dependences, and then (using rustup) nightly rust
/// with rustfmt.
pub fn install_rust(log: &Logger, ssh: &mut Session) -> Result<()> {
    // try five times
    let mut result = install_rust_inner(log, ssh);
    for _ in 0..10 {
        if let Ok(()) = result {
            return result;
        }
        trace!(
            log,
            "Retrying due to error installing rust dependencies: {:?}",
            result
        );
        result = install_rust_inner(log, ssh);
    }
    result
}

pub fn install_rust_inner(log: &Logger, ssh: &mut Session) -> Result<()> {
    trace!(log, "Installing Rust dependencies...");
    // Rust dependencies
    ssh.cmd("sudo apt update -y")?;
    ssh.cmd(
        "sudo apt install -y \
         build-essential \
         libssl-dev \
         pkg-config \
         unzip \
         m4",
    )?;

    // We need `rustfmt` for our build (because of prost).
    // 2020-03-22 is (as of 2020-04-03) the latest nightly that includes it.
    ssh.cmd(
        "curl https://sh.rustup.rs -sSf | sh -s -- \
         -y \
         --default-toolchain nightly-2020-03-22",
    )?;

    trace!(log, "Done installing Rust dependencies!");
    Ok(())
}

pub fn install_aws_cli(log: &Logger, ssh: &mut Session) -> Result<()> {
    // Download and install AWS CLI
    slog::trace!(log, "Downloading AWS CLI");
    ssh.cmd(
        "\
        curl \
             \"https://awscli.amazonaws.com/awscli-exe-linux-x86_64.zip\" \
             -o \"awscliv2.zip\"\
        ",
    )?;
    slog::trace!(log, "Unzipping and installing AWS CLI");
    ssh.cmd("unzip awscliv2.zip")?;
    ssh.cmd("sudo ./aws/install")?;
    slog::trace!(log, "AWS CLI installed.");

    slog::trace!(log, "Configuring AWS CLI.");
    ssh.cmd("mkdir -p $HOME/.aws")?;
    install_config_file(
        log,
        ssh,
        include_str!("data/aws-config.template").to_string(),
        Path::new(".aws/config"),
    )?;

    // We wouldn't have gotten this far if these weren't set.
    let vars = vec![
        (
            "AWS_ACCESS_KEY_ID".to_string(),
            std::env::var("AWS_ACCESS_KEY_ID")?,
        ),
        (
            "AWS_SECRET_ACCESS_KEY".to_string(),
            std::env::var("AWS_SECRET_ACCESS_KEY")?,
        ),
    ]
    .into_iter()
    .collect();
    let aws_creds = envsubst::substitute(include_str!("data/aws-credentials.template"), &vars)?;
    install_config_file(log, ssh, aws_creds, Path::new(".aws/config"))?;
    slog::trace!(log, "AWS CLI configured.");

    Ok(())
}

fn upload_s3(log: &Logger, ssh: &mut Session, src: &Path, object: &str) -> Result<()> {
    let dst = format!("s3://{}/{}", PROJECT_S3_BUCKET, object);
    trace!(log, "Uploading {:?} to {}", src, dst);
    ssh.cmd(&format!("aws s3 cp {} {}", src.to_string_lossy(), dst))?;
    Ok(())
}

pub fn download_s3(log: &Logger, ssh: &mut Session, dst: &Path, object: &str) -> Result<()> {
    let src = format!("s3://{}/{}", PROJECT_S3_BUCKET, object);
    trace!(log, "Downloading {} to {:?}", src, dst);
    ssh.cmd(&format!("aws s3 cp {} {}", src, dst.to_string_lossy(),))?;
    Ok(())
}

#[derive(Clone, Copy, Debug)]
pub enum Profile {
    Debug,
    Release,
}

impl Profile {
    fn flag(self) -> String {
        match self {
            Self::Debug => "".to_string(),
            Self::Release => "--release".to_string(),
        }
    }

    fn name(self) -> String {
        match self {
            Self::Debug => "debug".to_string(),
            Self::Release => "release".to_string(),
        }
    }
}

/// Compile Spectrum over the given SSH session.
///
/// `source` is a local path to the Spectrum source tarball.
/// `dest` is a local path to where the binary archive (`.tar.gz` containing
/// `spectrum/<bin name>`) will be written.
///
/// Slow: takes about 10 minutes on moderately powerful machines to pull all
/// dependences and compile in release mode.
fn build_spectrum(
    log: &Logger,
    ssh: &mut Session,
    source: &Path,
    dest: &str,
    profile: Profile,
) -> Result<()> {
    const SRC_ARCHIVE_NAME: &str = "spectrum-src.tar.gz";
    const BIN_ARCHIVE_NAME: &str = "spectrum-bin.tar.gz";
    const ALL_BINARIES: &[&str] = &[
        "publisher",
        "worker",
        "leader",
        "viewer",
        "broadcaster",
        "setup",
    ];

    trace!(log, "Uploading and extracting source archive.");
    ssh.upload(source, Path::new(SRC_ARCHIVE_NAME))?;
    ssh.cmd(&format!("tar -xzf {}", SRC_ARCHIVE_NAME))?;

    // Run the build
    let build_cmd = format!(
        "cd $HOME/spectrum && $HOME/.cargo/bin/cargo build {} --bins",
        profile.flag()
    );
    trace!(
        log,
        "Running build (what takes the time)";
        "cmd" => &build_cmd
    );
    ssh.cmd(&build_cmd)?;
    trace!(log, "Build done.");

    // Tar the binaries
    let binaries = ALL_BINARIES.iter().join(",");
    // tar all of the binaries into `$HOME/spectrum-bin.tar.gz`
    // `spectrum/<binary name>` in the archive.
    trace!(log, "Tarring binaries on remote server."; "binaries" => binaries.clone());
    ssh.cmd(&format!(
        "cd $HOME/spectrum/target && tar -czf $HOME/{} \
             --transform s/{}/spectrum/ \
             {}/{{{}}}",
        BIN_ARCHIVE_NAME,
        profile.name(),
        profile.name(),
        binaries
    ))?;

    trace!(log, "Uploading compiled binaries to s3.");
    upload_s3(log, ssh, Path::new(BIN_ARCHIVE_NAME), dest)?;
    Ok(())
}

async fn spawn_and_compile(
    log: &slog::Logger,
    hash: String,
    src_archive: PathBuf,
    machine_types: Vec<String>,
    profile: Profile,
    ami: String,
) -> Result<HashMap<String, String>> {
    debug!(log, "Compiling binaries for"; "machine_types" => format!("{:?}", &machine_types));
    let mut s3_binaries: HashMap<String, String> = HashMap::new();

    let mut compile_tsunami = TsunamiBuilder::default();
    compile_tsunami.set_logger(log.new(o!("tsunami" => "compile")));

    for machine_type in machine_types.into_iter() {
        let src_archive = src_archive.clone();
        let archive_name = format_binary(&hash, profile, &machine_type);
        s3_binaries.insert(machine_type.to_string(), archive_name.clone());

        let machine = {
            let machine_type = machine_type.clone();
            aws::Setup::default()
                .region(Region::UsEast2)
                .ami(ami.clone())
                .username("ubuntu")
                .instance_type(&machine_type)
                .setup(move |ssh, log| {
                    let log = log.new(o!("machine_type" => machine_type.clone()));
                    install_rust(&log, ssh)?;
                    install_aws_cli(&log, ssh)?;
                    build_spectrum(&log, ssh, &src_archive, &archive_name, profile)?;
                    Ok(())
                })
        };
        compile_tsunami.add(&machine_type, machine)?;
    }

    let mut aws_launcher = aws::OnDemandLauncher::default();
    compile_tsunami.spawn(&mut aws_launcher)?;

    // Nothing happens until this point.
    // Will block for a long time.
    aws_launcher.connect_all()?;
    trace!(log, "Compilation complete.");

    Ok(s3_binaries)
}

/// Format the name of a compiled binary.
fn format_binary(hash: &str, profile: Profile, machine: &str) -> String {
    format!("spectrum-{}-{}-{}.tar.gz", hash, machine, profile.name())
}

/// Compile Spectrum binaries for the given machine types (in AWS).
///
/// This function compiles the latest commit modifying the Spectrum
/// implementation on the appropriate machine type in AWS EX2.
///
/// Checks whether the binaries have been compiled for the given machine type
/// with the given source (cached in AWS S3) before compiling.
pub async fn compile(
    log: &slog::Logger,
    machine_types: Vec<String>,
    profile: Profile,
    ami: String,
) -> Result<HashMap<String, String>> {
    let git_root = String::from_utf8(
        Command::new("git")
            .args(&["rev-parse", "--show-toplevel"])
            .output()
            .await?
            .stdout,
    )?;
    let git_root = &Path::new(git_root.trim());

    // Get the last commit modfiying Spectrum server code.
    // (ignore changes to experiment harness etc.)
    let last_commit = String::from_utf8(
        Command::new("git")
            .args(&["rev-list", "-1", "HEAD", "--", "spectrum"])
            .current_dir(&git_root)
            .output()
            .await?
            .stdout,
    )?;
    let last_commit = last_commit.trim();

    trace!(log, "Creating a tarball with current checked-in Git src"; "commit" => &last_commit);
    let src_dir = tempfile::TempDir::new()?;
    let src_archive = src_dir.path().join("spectrum-src.tar.gz");
    Command::new("git")
        .arg("archive")
        .args(&["--format", "tar.gz"])
        .args(&["--output", &src_archive.to_string_lossy()])
        .args(&["--prefix", "spectrum/"])
        .arg(&last_commit)
        .current_dir(&git_root)
        .spawn()?
        .await?;

    // Only compile machine types that aren't in the s3 cache already.
    // We format file names based on machine type and the git hash.
    trace!(log, "Source tarball created"; "hash" => &last_commit);
    let mut binaries = HashMap::<String, String>::new();

    let s3 = S3Client::new(Region::UsEast2);
    let request = rusoto_s3::ListObjectsRequest {
        bucket: PROJECT_S3_BUCKET.to_string(),
        ..Default::default()
    };
    let objects: HashSet<String> = s3
        .list_objects(request)
        .await?
        .contents
        .unwrap()
        .into_iter()
        .filter_map(|o| o.key)
        .collect();

    let needs_build: Vec<String> = machine_types
        .into_iter()
        .filter(|machine_type| {
            let object = format_binary(&last_commit, profile, machine_type);
            if objects.contains(&object) {
                slog::trace!(log, "Found {} in s3!", &object);
                binaries.insert(machine_type.to_string(), object);
                false
            } else {
                slog::trace!(log, "Didn't find {} in s3!", &object);
                true
            }
        })
        .collect();

    let new_binaries = spawn_and_compile(
        log,
        last_commit.to_string(),
        src_archive,
        needs_build,
        profile,
        ami,
    )
    .await?;
    binaries.extend(new_binaries);

    Ok(binaries)
}
