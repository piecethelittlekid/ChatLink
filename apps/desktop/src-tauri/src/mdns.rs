use socket2::{Domain, Protocol, Socket, Type};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

const MDNS_PORT: u16 = 5353;
const MDNS_MULTICAST_IPV4: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);

// QNAME: \x08chatlink\x05local\x00
const CHATLINK_LOCAL_QNAME: &[u8] = b"\x08chatlink\x05local\x00";

pub struct MdnsHandle {
    cancel: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}

impl MdnsHandle {
    pub async fn stop(self) {
        self.cancel.cancel();
        let _ = tokio::time::timeout(std::time::Duration::from_millis(500), self.task).await;
    }
}

pub fn build_a_response(ip: Ipv4Addr) -> Vec<u8> {
    let mut packet = Vec::with_capacity(64);
    // DNS Header
    packet.extend_from_slice(&[
        0x00, 0x00, // ID: 0
        0x84, 0x00, // Flags: Standard response, Authoritative Answer
        0x00, 0x00, // QDCOUNT: 0
        0x00, 0x01, // ANCOUNT: 1
        0x00, 0x00, // NSCOUNT: 0
        0x00, 0x00, // ARCOUNT: 0
    ]);

    // Answer RR: chatlink.local
    packet.extend_from_slice(CHATLINK_LOCAL_QNAME);
    packet.extend_from_slice(&[
        0x00, 0x01, // TYPE: A (0x0001)
        0x80, 0x01, // CLASS: IN (0x0001) with cache-flush bit (0x8000)
        0x00, 0x00, 0x00, 0x78, // TTL: 120 seconds
        0x00, 0x04, // RDLENGTH: 4 bytes
    ]);
    packet.extend_from_slice(&ip.octets());
    packet
}

fn contains_query(buf: &[u8]) -> bool {
    // Quick lowercase comparison for \x08chatlink\x05local\x00
    if buf.len() < CHATLINK_LOCAL_QNAME.len() {
        return false;
    }
    let target = CHATLINK_LOCAL_QNAME;
    buf.windows(target.len()).any(|window| {
        window
            .iter()
            .zip(target.iter())
            .all(|(&a, &b)| a.to_ascii_lowercase() == b.to_ascii_lowercase())
    })
}

pub fn start_mdns(ip: Ipv4Addr) -> Option<MdnsHandle> {
    let socket = match create_socket() {
        Ok(s) => s,
        Err(err) => {
            tracing::warn!(%err, "Could not bind mDNS port 5353; relying on OS hostname resolution");
            return None;
        }
    };

    let cancel = CancellationToken::new();
    let cancel_child = cancel.clone();
    let ip_state = Arc::new(RwLock::new(ip));

    let task = tokio::spawn(async move {
        let multi_dest = SocketAddr::V4(SocketAddrV4::new(MDNS_MULTICAST_IPV4, MDNS_PORT));
        
        // Announce on startup
        let startup_ip = *ip_state.read().await;
        let response = build_a_response(startup_ip);
        let _ = socket.send_to(&response, multi_dest).await;

        let mut buf = [0u8; 1500];
        loop {
            tokio::select! {
                _ = cancel_child.cancelled() => {
                    break;
                }
                res = socket.recv_from(&mut buf) => {
                    match res {
                        Ok((len, src)) => {
                            if contains_query(&buf[..len]) {
                                let cur_ip = *ip_state.read().await;
                                let resp = build_a_response(cur_ip);
                                // Send both to requester and to multicast group
                                let _ = socket.send_to(&resp, src).await;
                                let _ = socket.send_to(&resp, multi_dest).await;
                            }
                        }
                        Err(err) => {
                            tracing::debug!(%err, "mDNS recv error");
                            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                        }
                    }
                }
            }
        }
    });

    Some(MdnsHandle { cancel, task })
}

fn create_socket() -> std::io::Result<tokio::net::UdpSocket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    socket.set_nonblocking(true)?;

    let bind_addr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, MDNS_PORT);
    socket.bind(&bind_addr.into())?;
    socket.join_multicast_v4(&MDNS_MULTICAST_IPV4, &Ipv4Addr::UNSPECIFIED)?;

    let std_socket: std::net::UdpSocket = socket.into();
    tokio::net::UdpSocket::from_std(std_socket)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_a_response() {
        let ip = Ipv4Addr::new(192, 168, 1, 100);
        let packet = build_a_response(ip);
        assert_eq!(packet.len(), 42);
        assert_eq!(&packet[38..42], &[192, 168, 1, 100]);
    }

    #[test]
    fn test_contains_query() {
        let mut query = vec![0u8; 12];
        query.extend_from_slice(b"\x08ChatLink\x05LOCAL\x00");
        query.extend_from_slice(&[0x00, 0x01, 0x00, 0x01]);
        assert!(contains_query(&query));
        assert!(!contains_query(b"\x06google\x03com\x00"));
    }
}
