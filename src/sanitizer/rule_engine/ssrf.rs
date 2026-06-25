use std::net::IpAddr;

pub fn is_private_or_reserved_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(addr) => {
            addr.is_private()
                || addr.is_loopback()
                || addr.is_link_local()
                || addr.is_broadcast()
                || addr.is_unspecified()
                || addr.octets()[0] == 100 && (addr.octets()[1] & 0b1100_0000 == 0b0100_0000)
                || addr.octets()[0] == 10
                || addr.octets()[0] == 172 && (addr.octets()[1] & 0b1111_0000 == 0b0001_0000)
                || addr.octets()[0] == 192 && addr.octets()[1] == 168
        },
        IpAddr::V6(addr) => {
            addr.is_loopback()
                || addr.is_unspecified()
                || addr.segments()[0] == 0xfc00
                || addr.segments()[0] == 0xfe80
        },
    }
}

pub async fn resolve_and_check_ssrf(host: &str) -> bool {
    use std::net::ToSocketAddrs;
    let addr = format!("{host}:443");
    if let Ok(mut iter) = addr.to_socket_addrs()
        && let Some(socket_addr) = iter.next()
    {
        return !is_private_or_reserved_ip(&socket_addr.ip());
    }
    false
}
