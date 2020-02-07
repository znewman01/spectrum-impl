use portpicker::pick_unused_port;
use std::net::SocketAddr;

// Pick a loopback address with
pub fn get_addr() -> SocketAddr {
    // TODO(zjn): should not be loopback address
    // TODO(zjn): use IPv6 if available
    SocketAddr::new(
        "127.0.0.1".parse().unwrap(),
        pick_unused_port().expect("No ports free."),
    )
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};

    pub fn addrs() -> impl Strategy<Value = SocketAddr> {
        let ipv4 = (
            any::<u8>(),
            any::<u8>(),
            any::<u8>(),
            any::<u8>(),
            any::<u16>(),
        )
            .prop_map(|(a, b, c, d, port)| {
                SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(a, b, c, d), port))
            });
        let ipv6addrs = (
            any::<u16>(),
            any::<u16>(),
            any::<u16>(),
            any::<u16>(),
            any::<u16>(),
            any::<u16>(),
            any::<u16>(),
            any::<u16>(),
        )
            .prop_map(|(a, b, c, d, e, f, g, h)| Ipv6Addr::new(a, b, c, d, e, f, g, h));
        let ipv6 = (ipv6addrs, any::<u16>())
            .prop_map(|(addr, port)| SocketAddr::V6(SocketAddrV6::new(addr, port, 0, 0)));

        ipv4.boxed().prop_union(ipv6.boxed())
    }

    #[test]
    fn test_get_socket_addr() {
        if pick_unused_port().is_none() {
            return; // no unused ports; this happens on e.g. Travis sometimes
        }

        assert!(get_addr().ip().is_loopback());
    }
}
