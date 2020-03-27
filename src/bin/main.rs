use clap::{arg_enum, crate_authors, crate_version, value_t, App, Arg};
use simplelog::{CombinedLogger, LevelFilter, TermLogger, TerminalMode, WriteLogger};
use spectrum_impl::{config, experiment::Experiment, protocols::wrapper::ProtocolWrapper, run};
use std::fs::File;

arg_enum! {
    // Corresponds 1:1 to LevelFilter enum
    pub enum LogLevel {
        Off,
        Error,
        Warn,
        Info,
        Debug,
        Trace,
    }
}

impl Into<LevelFilter> for LogLevel {
    fn into(self) -> LevelFilter {
        match self {
            Self::Off => LevelFilter::Off,
            Self::Error => LevelFilter::Error,
            Self::Warn => LevelFilter::Warn,
            Self::Info => LevelFilter::Info,
            Self::Debug => LevelFilter::Debug,
            Self::Trace => LevelFilter::Trace,
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let matches = App::new("Spectrum -- local testing client")
        .version(crate_version!())
        .about("Run the Spectrum protocol locally.")
        .author(crate_authors!())
        .arg(
            Arg::with_name("log-level")
                .long("log-level")
                .short("v")
                .takes_value(true)
                .possible_values(&LogLevel::variants())
                .default_value("debug")
                .help("Log level")
                .case_insensitive(true),
        )
        .arg(
            Arg::with_name("clients")
                .long("clients")
                .takes_value(true)
                .default_value("10")
                .help("Number of clients to simulate.")
                .long_help(
                    "Number of clients to simulate. \
                     This includes both viewers and broadcasters (one per channel), \
                     so it  must be at least as great as the number of channels.",
                ),
        )
        .arg(
            Arg::with_name("group-size")
                .long("group-size")
                .takes_value(true)
                .default_value("2")
                .help("Number of workers per group."),
        )
        .arg(
            Arg::with_name("channels")
                .long("channels")
                .takes_value(true)
                .number_of_values(1)
                .default_value("3")
                .help("Number of channels to simulate"),
        )
        .arg(
            Arg::with_name("message-size")
                .long("message-size")
                .takes_value(true)
                .number_of_values(1)
                .default_value("1024")
                .help("Size (in bytes) of each message."),
        )
        .arg(
            Arg::with_name("security")
                .long("security")
                .takes_value(true)
                .number_of_values(1)
                .default_value("40")
                .help("Size (in bits) to use for the secure protocol.")
                .long_help(
                    "Size (in bits) to use for the secure protocol. \
                            At most one of {--security, --no-security} may be set.",
                ),
        )
        .arg(
            Arg::with_name("no-security")
                .long("no-security")
                .conflicts_with("security")
                .help("Use the insecure protocol.")
                .long_help(
                    "Use the insecure protocol. \
                            At most one of {--security, --no-security} may be set.",
                ),
        )
        .get_matches();

    let log_level: LevelFilter = value_t!(matches, "log-level", LogLevel)
        .unwrap_or_else(|e| e.exit())
        .into();
    CombinedLogger::init(vec![
        TermLogger::new(
            log_level,
            simplelog::ConfigBuilder::new()
                .add_filter_allow_str("spectrum_impl")
                .build(),
            TerminalMode::Stderr,
        )
        .unwrap(),
        WriteLogger::new(
            LevelFilter::Trace,
            simplelog::ConfigBuilder::new()
                .add_filter_allow_str("spectrum_impl")
                .build(),
            File::create("spectrum.log").unwrap(),
        ),
    ])
    .unwrap();

    let groups = 2; // hard-coded for now
    let msg_size = value_t!(matches, "message-size", usize).unwrap_or_else(|e| e.exit());
    let security: Option<u32> = if matches.is_present("no-security") {
        None
    } else {
        Some(value_t!(matches, "security", u32).unwrap_or_else(|e| e.exit()))
    };
    let clients = value_t!(matches, "clients", u16).unwrap_or_else(|e| e.exit());
    let group_size = value_t!(matches, "group-size", u16).unwrap_or_else(|e| e.exit());
    let channels = value_t!(matches, "channels", usize).unwrap_or_else(|e| e.exit());

    let protocol = ProtocolWrapper::new(security, groups, channels, msg_size);
    let experiment = Experiment::new(protocol, group_size, clients);

    eprintln!("Running: {:?}...", experiment);
    let config = config::from_env()?;
    let elapsed = run(experiment, config).await?;
    eprintln!("Elapsed time: {:?}", elapsed);
    Ok(())
}
