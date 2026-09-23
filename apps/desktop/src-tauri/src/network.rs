use std::net::{IpAddr, Ipv4Addr};

pub fn lan_ipv4_addresses() -> Vec<Ipv4Addr> {
    let mut addresses: Vec<_> = get_if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|interface| match interface.ip() {
            IpAddr::V4(ip)
                if ip.is_private()
                    && !ip.is_loopback()
                    && !ip.is_link_local()
                    && !ip.is_unspecified() =>
            {
                Some(ip)
            }
            _ => None,
        })
        .collect();
    addresses.sort();
    addresses.dedup();
    addresses
}
