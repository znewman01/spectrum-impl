use crate::{
    experiment::Experiment, net::Config as NetConfig, protocols::wrapper::ProtocolWrapper,
};

use clap::Clap;
use simplelog::{LevelFilter, SimpleLogger, TermLogger, TerminalMode};
use tonic::transport::{Certificate, Identity};

#[derive(Clap)]
pub struct Args {
    #[clap(flatten)]
    logs: LogArgs,
    #[clap(flatten)]
    experiment: ExperimentArgs,
}

impl Args {
    pub fn init_logs(&self) {
        self.logs.init();
    }
}

impl From<Args> for Experiment {
    fn from(args: Args) -> Experiment {
        args.experiment.into()
    }
}

// TODO: parse valid values, put in help string
#[derive(Clap)]
pub struct LogArgs {
    /// Log level.
    #[clap(short = 'v', long, default_value = "debug", env = "SPECTRUM_LOG_LEVEL")]
    log_level: LevelFilter,
}

impl LogArgs {
    pub fn init(&self) {
        use simplelog::{CombinedLogger, ConfigBuilder, SharedLogger};
        const FILTER: &str = "spectrum_impl";

        let config = ConfigBuilder::new().add_filter_ignore_str(FILTER).build();
        let other_logger: Box<dyn SharedLogger> =
            match TermLogger::new(LevelFilter::Info, config.clone(), TerminalMode::Stderr) {
                Some(logger) => logger,
                None => SimpleLogger::new(LevelFilter::Info, config),
            };

        let config = ConfigBuilder::new().add_filter_allow_str(FILTER).build();
        let spectrum_logger: Box<dyn SharedLogger> =
            match TermLogger::new(self.log_level, config.clone(), TerminalMode::Stderr) {
                Some(logger) => logger,
                None => SimpleLogger::new(self.log_level, config),
            };

        CombinedLogger::init(vec![spectrum_logger, other_logger])
            .expect("Failed initializing logger.");
    }
}

#[derive(Clap)]
pub struct NetArgs {
    /// Port on which the service should bind (localhost interface).
    ///
    /// If not given, a random unused port will be picked.
    #[clap(long)]
    local_port: Option<u16>,

    /// Host (and optional port) to publish as the address of this service.
    ///
    /// If not given, use `localhost` and the port from `--local-port`.
    #[clap(long = "public-address")]
    public_addr: Option<String>,

    #[clap(flatten)]
    tls: TlsServerArgs,
}

#[derive(Clap)]
pub struct TlsServerArgs {
    /// Path to .crt
    #[clap(long = "tls-cert", env = "SPECTRUM_TLS_CERT")]
    cert_file: Option<String>,

    /// Path to .key
    #[clap(long = "tls-key", env = "SPECTRUM_TLS_KEY")]
    key_file: Option<String>,

    #[clap(flatten)]
    tls_ca: TlsCaArgs,
}

impl From<TlsServerArgs> for Option<(Identity, Certificate)> {
    fn from(args: TlsServerArgs) -> Option<(Identity, Certificate)> {
        match (args.cert_file, args.key_file) {
            (None, None) => None,
            (Some(cert_file), Some(key_file)) => {
                let cert = std::fs::read_to_string(cert_file).unwrap();
                let key = std::fs::read_to_string(key_file).unwrap();
                let identity = Identity::from_pem(cert, key);
                let cert: Option<Certificate> = args.tls_ca.into();
                Some(identity).zip(cert)
            }
            _ => {
                panic!("TLS cert and key must be provided together.");
            }
        }
    }
}

#[derive(Clap)]
pub struct TlsCaArgs {
    /// Path to ca.pem
    #[clap(long = "tls-ca", env = "SPECTRUM_TLS_CA")]
    ca_file: Option<String>,
}

impl From<TlsCaArgs> for Option<Certificate> {
    fn from(args: TlsCaArgs) -> Option<Certificate> {
        args.ca_file.map(|ca_file| {
            let pem = std::fs::read_to_string(ca_file).unwrap();
            Certificate::from_pem(pem)
        })
    }
}

impl From<NetArgs> for NetConfig {
    fn from(args: NetArgs) -> NetConfig {
        let tls: Option<(Identity, Certificate)> = args.tls.into();
        match (args.local_port, args.public_addr) {
            (None, None) => NetConfig::with_free_port_localhost(tls),
            (None, Some(public_addr)) => NetConfig::with_free_port(public_addr, tls),
            (Some(local_port), None) => NetConfig::new_localhost(local_port, tls),
            (Some(local_port), Some(public_addr)) => NetConfig::new(local_port, public_addr, tls),
        }
    }
}

#[derive(Clap)]
pub struct ExperimentArgs {
    /// Number of clients to simulate.
    ///
    /// This includes both viewers and broadcasters (one per channel), so it
    /// must be at least as great as the number of channels.
    #[clap(long, default_value = "10")]
    clients: u128,

    /// Number of workers per group.
    #[clap(long, default_value = "2")]
    group_size: u16,

    /// Number of channels to simulate.
    #[clap(long, default_value = "3")]
    channels: usize,

    /// Number of groups to simulate.
    ///
    /// Must be exactly 2 for the default protocol.
    #[clap(long, default_value = "2")]
    groups: usize,

    /// Size (in bytes) of each message.
    #[clap(long = "message-size", default_value = "1024")]
    msg_size: usize,

    // Security args might get a little cleaner with:
    // https://github.com/TeXitoi/structopt/issues/104
    /// Size (in bytes) to use for the secure protocol.
    ///
    /// At most one of {--security, --no-security, --security-multi-key} may be set.
    /// [default: 16]
    #[clap(long = "security", group = "security")]
    security_bytes: Option<u32>,

    /// Size (in bytes) to use for the secure protocol.
    #[clap(long = "security-multi-key", group = "security")]
    security_multi_key_bytes: Option<u32>,

    /// Run the insecure protocol.
    ///
    /// At most one of {--security, --no-security} may be set.
    #[clap(long = "no-security", group = "security")]
    no_security: bool,

    /// If true, don't set up a publisher or leaders; just measure raw QPS.
    #[clap(long)]
    hammer: bool,
}

impl ExperimentArgs {
    fn security_bytes(&self) -> Option<u32> {
        if self.no_security {
            None
        } else if let Some(bytes) = self.security_multi_key_bytes {
            Some(bytes)
        } else {
            self.security_bytes.or(Some(16))
        }
    }
}

impl From<ExperimentArgs> for ProtocolWrapper {
    fn from(args: ExperimentArgs) -> Self {
        ProtocolWrapper::new(
            args.security_bytes().is_some(),
            args.security_multi_key_bytes.is_some(),
            args.groups,
            args.channels,
            args.msg_size,
        )
    }
}

impl From<ExperimentArgs> for Experiment {
    fn from(args: ExperimentArgs) -> Self {
        let group_size = args.group_size;
        let clients = args.clients;
        let hammer = args.hammer;
        Experiment::new_sample_keys(args.into(), group_size, clients, hammer)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn test_security_default() {
        let args = ExperimentArgs::try_parse_from(&["binary"]).unwrap();
        assert_eq!(args.security_bytes(), Some(16));
    }

    #[test]
    fn test_security_no_security() {
        let args = ExperimentArgs::try_parse_from(&["binary", "--no-security"]).unwrap();
        assert_eq!(args.security_bytes(), None);
    }

    #[test]
    fn test_security_custom_security() {
        let args = ExperimentArgs::try_parse_from(&["binary", "--security", "30"]).unwrap();
        assert_eq!(args.security_bytes(), Some(30));
    }

    #[test]
    fn test_security_conflicts() {
        assert!(
            ExperimentArgs::try_parse_from(&["binary", "--security", "30", "--no-security"])
                .is_err(),
            "Passing both `--no-security` and `--security` should error."
        );
    }
}
