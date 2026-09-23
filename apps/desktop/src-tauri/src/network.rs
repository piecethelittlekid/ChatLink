use std::net::{IpAddr, Ipv4Addr};

#[derive(Clone)]
pub struct LanInterface {
    pub ip: Ipv4Addr,
    pub name: String,
}

#[cfg(target_os = "windows")]
fn friendly_name(interface_name: &str) -> String {
    use winreg::{enums::HKEY_LOCAL_MACHINE, RegKey};
    let path = format!(
        "SYSTEM\\CurrentControlSet\\Control\\Network\\{{4D36E972-E325-11CE-BFC1-08002BE10318}}\\{}\\Connection",
        interface_name
    );
    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(path)
        .and_then(|key| key.get_value::<String, _>("Name"))
        .unwrap_or_else(|_| interface_name.to_owned())
}

#[cfg(not(target_os = "windows"))]
fn friendly_name(interface_name: &str) -> String {
    interface_name.to_owned()
}

fn virtual_interface(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [
        "vethernet",
        "wsl",
        "hyper-v",
        "virtual",
        "vmware",
        "vbox",
        "docker",
        "tailscale",
        "zerotier",
        "loopback",
        "tunnel",
        "tun ",
        "tap ",
    ]
    .iter()
    .any(|part| lower.contains(part))
}

fn priority(name: &str) -> u8 {
    let lower = name.to_ascii_lowercase();
    if lower.contains("wi-fi") || lower.contains("wireless") || lower.contains("wlan") {
        0
    } else if lower.contains("ethernet") {
        1
    } else {
        2
    }
}

pub fn lan_interfaces() -> Vec<LanInterface> {
    let mut interfaces: Vec<_> = get_if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|interface| {
            let name = friendly_name(&interface.name);
            match interface.ip() {
                IpAddr::V4(ip)
                    if ip.is_private()
                        && !ip.is_loopback()
                        && !ip.is_link_local()
                        && !ip.is_unspecified()
                        && !virtual_interface(&name) =>
                {
                    Some(LanInterface { ip, name })
                }
                _ => None,
            }
        })
        .collect();
    interfaces.sort_by_key(|interface| (priority(&interface.name), interface.ip));
    interfaces.dedup_by_key(|interface| interface.ip);
    interfaces
}

pub fn lan_ipv4_addresses() -> Vec<Ipv4Addr> {
    lan_interfaces()
        .into_iter()
        .map(|interface| interface.ip)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_interfaces_are_preferred() {
        assert!(virtual_interface("vEthernet (WSL (Hyper-V firewall))"));
        assert!(virtual_interface("VirtualBox Host-Only Network"));
        assert!(!virtual_interface("Wi-Fi"));
        assert!(!virtual_interface("Ethernet"));
        assert!(priority("Wi-Fi") < priority("Ethernet"));
    }
}
