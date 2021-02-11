// TODO(zjn): use IPv6 if available
// TODO(zjn): use portpicker when https://github.com/Dentosal/portpicker-rs/pull/1 merged
use port_check::free_local_port;
use std::net::SocketAddr;
use tonic::transport::{Certificate, Identity};

/// Common configuration for a network service.
#[derive(Debug, Clone)]
pub struct Config {
    /// Port on which the service should bind (localhost interface).
    local_port: u16,

    /// Host (and optional port) to publish as the address of this service.
    public_addr: String,

    pub tls: Option<(Identity, Certificate)>,
}

impl Config {
    pub fn new(local_port: u16, public_addr: String, tls: Option<(Identity, Certificate)>) -> Self {
        Self {
            local_port,
            public_addr,
            tls,
        }
    }

    pub fn new_localhost(local_port: u16, tls: Option<(Identity, Certificate)>) -> Self {
        Self {
            local_port,
            public_addr: format!("localhost:{}", local_port),
            tls,
        }
    }

    pub fn tls_ident(&self) -> Option<Identity> {
        self.tls.as_ref().map(|(i, _)| i.clone())
    }
    pub fn tls_cert(&self) -> Option<Certificate> {
        self.tls.as_ref().map(|(_, c)| c.clone())
    }

    /// A network configuration useful for running locally.
    pub fn with_free_port_localhost(tls: Option<(Identity, Certificate)>) -> Self {
        let local_port = free_local_port().expect("No ports free");
        Self::new_localhost(local_port, tls)
    }

    pub fn with_free_port(public_addr: String, tls: Option<(Identity, Certificate)>) -> Self {
        let mut config = Self::with_free_port_localhost(tls);
        config.public_addr = public_addr;
        config
    }

    pub fn set_public_addr(&mut self, public_addr: String) {
        self.public_addr = public_addr;
    }

    pub fn local_socket_addr(&self) -> SocketAddr {
        SocketAddr::new("0.0.0.0".parse().unwrap(), self.local_port)
    }

    pub fn public_addr(&self) -> String {
        self.public_addr.clone()
    }
}

#[cfg(test)]
pub mod tests {
    use proptest::prelude::*;

    pub fn addrs() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("127.0.0.1:8080".to_string()),
            Just("localhost:8080".to_string()),
        ]
    }
}
