// TODO(zjn): use IPv6 if available
// TODO(zjn): use portpicker when https://github.com/Dentosal/portpicker-rs/pull/1 merged
use clap::Clap;
use port_check::free_local_port;
use std::net::SocketAddr;

/// Common configuration for a network service.
#[derive(Clap, Debug, Clone)]
pub struct Config {
    /// Port on which the service should bind (localhost interface).
    #[clap(long)]
    local_port: u16,

    /// Host (and optional port) to publish as the address of this service.
    #[clap(long)]
    public_addr: String,
}

impl Config {
    /// A network configuration useful for running locally.
    pub fn with_free_port() -> Self {
        let local_port = free_local_port().expect("No ports free");
        Self {
            local_port,
            public_addr: format!("127.0.0.1:{}", local_port),
        }
    }

    pub fn local_socket_addr(&self) -> SocketAddr {
        SocketAddr::new("127.0.0.1".parse().unwrap(), self.local_port)
    }

    pub fn public_addr(&self) -> String {
        self.public_addr.clone()
    }
}
