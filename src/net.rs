#[cfg(test)]
pub mod tests {
    use proptest::prelude::*;
    use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};

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
}
