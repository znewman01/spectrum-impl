use failure::Error;
use itertools::Itertools;
use rusoto_core::Region;
use sessh::Session;
use slog::{debug, o, trace, Logger};
use tokio::process::Command;
use tsunami::providers::aws;
use tsunami::providers::Launcher;
use tsunami::TsunamiBuilder;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, Error>;

/// Install Rust and dependencies over the given SSH session.
///
/// Installs (using apt) all dependences, and then (using rustup) nightly rust
/// with rustfmt.
fn install_rust(log: &Logger, ssh: &mut Session) -> Result<()> {
    trace!(log, "Installing Rust dependencies...");
    // Rust dependencies
    ssh.cmd(
        "sudo apt-get install -y \
         build-essential \
         libssl-dev \
         pkg-config \
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
    dest: &Path,
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
    trace!(
        log,
        "Running build (what takes the time)";
        "profile" => profile.name()
    );
    ssh.cmd(&format!(
        "cd $HOME/spectrum && $HOME/.cargo/bin/cargo build {} --bins",
        profile.flag()
    ))?;
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

    trace!(log, "Downloading compiled binaries.");
    ssh.download(Path::new(BIN_ARCHIVE_NAME), dest)?;
    Ok(())
}

async fn spawn_and_compile(
    log: &slog::Logger,
    bin_dir: PathBuf,
    hash: String,
    src_archive: PathBuf,
    machine_types: Vec<String>,
    profile: Profile,
    ami: String,
) -> Result<HashMap<String, PathBuf>> {
    debug!(log, "Compiling binaries for"; "machine_types" => format!("{:?}", &machine_types));
    let mut bin_archives: HashMap<String, PathBuf> = HashMap::new();

    let mut compile_tsunami = TsunamiBuilder::default();
    compile_tsunami.set_logger(log.new(o!("tsunami" => "compile")));

    for machine_type in machine_types.into_iter() {
        let src_archive = src_archive.clone();
        let archive_name = format_binary(&hash, profile, &machine_type);
        let bin_archive = bin_dir.join(archive_name);
        bin_archives.insert(machine_type.to_string(), bin_archive.clone());

        let machine = {
            let machine_type = machine_type.clone();
            aws::Setup::default()
                .region(Region::UsEast2)
                .ami(ami.clone())
                .username("ubuntu")
                .instance_type(&machine_type)
                .setup(move |ssh, log| {
                    let log = log.new(o!("machine_type" => machine_type.clone()));
                    ssh.cmd("sudo apt update")?;
                    install_rust(&log, ssh)?;
                    build_spectrum(&log, ssh, &src_archive, &bin_archive, profile)?;
                    Ok(())
                })
        };
        compile_tsunami.add(&machine_type, machine)?;
    }

    let mut aws_launcher = aws::Launcher::default();
    aws_launcher.set_max_instance_duration(1);
    compile_tsunami.spawn(&mut aws_launcher)?;

    // Nothing happens until this point.
    // Will block for a long time.
    aws_launcher.connect_all()?;
    trace!(log, "Compilation complete.");

    Ok(bin_archives)
}

/// Format the name of a compiled binary.
fn format_binary(hash: &str, profile: Profile, machine: &str) -> String {
    format!("spectrum-{}-{}-{}.tar.gz", hash, machine, profile.name())
}

/// Compile Spectrum binaries for the given machine types (in AWS).
///
/// `src_dir` is the path to the root of the Spectrum git repo.
/// This function compiles the HEAD commit on the appropriate machine type in
/// AWS.
///
/// Checks whether the binaries have been compiled for the given machine type
/// with the given source (cached in `bin_dir`) before compiling.
pub async fn compile(
    log: &slog::Logger,
    bin_dir: PathBuf,
    src_dir: PathBuf,
    machine_types: Vec<String>,
    profile: Profile,
    ami: String,
) -> Result<HashMap<String, PathBuf>> {
    trace!(
        log,
        "Creating a tarball with current checked-in Git src (HEAD)"
    );
    let src_archive: PathBuf = bin_dir.join("spectrum-src.tar.gz");
    Command::new("git")
        .arg("archive")
        .args(&["--format", "tar.gz"])
        .args(&["--output", &src_archive.to_string_lossy()])
        .args(&["--prefix", "spectrum/"])
        .arg("HEAD")
        .current_dir(&src_dir)
        .spawn()?
        .await?;

    // Only compile machine types that aren't in the `bin_dir` cache already.
    // We format file names based on machine type and the git hash.
    let hash = Command::new("git")
        .args(&["rev-parse", "--short", "HEAD"])
        .current_dir(&src_dir)
        .output()
        .await?
        .stdout;
    let hash = String::from_utf8(hash)
        .expect("git output should be ASCII")
        .trim_end_matches('\n')
        .to_string();
    trace!(log, "Source tarball created"; "hash" => &hash);
    let machine_types: Vec<String> = machine_types
        .into_iter()
        .filter(|t| !bin_dir.join(format_binary(&hash, profile, t)).exists())
        .collect();

    spawn_and_compile(log, bin_dir, hash, src_archive, machine_types, profile, ami).await
}
