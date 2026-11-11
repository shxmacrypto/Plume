//! This PC's address on the local network: the one of the connection Windows would use to go out (Wi-Fi or
//! Ethernet), read from the network adapters. No packet is sent.

use std::net::Ipv4Addr;
use windows::Win32::{
    NetworkManagement::{
        IpHelper::{
            GAA_FLAG_INCLUDE_GATEWAYS, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST, GetAdaptersAddresses, GetBestInterface,
            IF_TYPE_SOFTWARE_LOOPBACK, IF_TYPE_TUNNEL, IP_ADAPTER_ADDRESSES_LH,
        },
        Ndis::IfOperStatusUp,
    },
    Networking::WinSock::{AF_INET, SOCKADDR_IN},
};

struct Adapter {
    index: u32,
    ips: Vec<Ipv4Addr>,
    gateway: bool,
}

fn adapters() -> Vec<Adapter> {
    let mut out = Vec::new();
    unsafe {
        let flags = GAA_FLAG_INCLUDE_GATEWAYS | GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
        let mut size = 16 * 1024u32;
        let mut buf: Vec<u64> = Vec::new();
        for _ in 0..3 {
            buf = vec![0u64; size as usize / 8 + 1];
            let r = GetAdaptersAddresses(AF_INET.0 as u32, flags, None, Some(buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH), &mut size);
            match r {
                0 => break,
                111 => continue, // ERROR_BUFFER_OVERFLOW: `size` now holds what it needs
                _ => return out,
            }
        }
        let mut p = buf.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
        while let Some(a) = p.as_ref() {
            if a.OperStatus == IfOperStatusUp && a.IfType != IF_TYPE_SOFTWARE_LOOPBACK && a.IfType != IF_TYPE_TUNNEL {
                let mut ips = Vec::new();
                let mut u = a.FirstUnicastAddress;
                while let Some(ua) = u.as_ref() {
                    let sa = ua.Address.lpSockaddr;
                    if !sa.is_null() && (*sa).sa_family == AF_INET {
                        let sin = &*(sa as *const SOCKADDR_IN);
                        let ip = Ipv4Addr::from(u32::from_be(sin.sin_addr.S_un.S_addr));
                        if !ip.is_loopback() && !ip.is_unspecified() {
                            ips.push(ip);
                        }
                    }
                    u = ua.Next;
                }
                if !ips.is_empty() {
                    out.push(Adapter { index: a.Anonymous1.Anonymous.IfIndex, ips, gateway: !a.FirstGatewayAddress.is_null() });
                }
            }
            p = a.Next;
        }
    }
    out
}

/// Ranked choice: the adapter of the way out, then one with a gateway, then any; private addresses first.
fn choose(list: &[Adapter], best: Option<u32>) -> Option<Ipv4Addr> {
    let score = |a: &Adapter, ip: &Ipv4Addr| -> i32 {
        let mut s = 0;
        if Some(a.index) == best {
            s += 100;
        }
        if a.gateway {
            s += 20;
        }
        if ip.is_private() {
            s += 10;
        }
        if ip.is_link_local() {
            s -= 50;
        }
        s
    };
    list.iter().flat_map(|a| a.ips.iter().map(move |ip| (score(a, ip), *ip))).max_by_key(|(s, _)| *s).map(|(_, ip)| ip)
}

/// This PC's address for the phones, if it is on a network.
pub fn lan_ip() -> Option<Ipv4Addr> {
    let mut best = 0u32;
    // the interface Windows would use to reach the internet (8.8.8.8, in network order)
    let dest = u32::from_ne_bytes([8, 8, 8, 8]);
    let best = (unsafe { GetBestInterface(dest, &mut best) } == 0).then_some(best);
    choose(&adapters(), best)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranking() {
        let list = vec![
            Adapter { index: 3, ips: vec![Ipv4Addr::new(169, 254, 1, 2)], gateway: false },
            Adapter { index: 7, ips: vec![Ipv4Addr::new(172, 20, 0, 1)], gateway: false },
            Adapter { index: 9, ips: vec![Ipv4Addr::new(192, 168, 1, 20)], gateway: true },
        ];
        assert_eq!(choose(&list, Some(9)), Some(Ipv4Addr::new(192, 168, 1, 20)));
        assert_eq!(choose(&list, None), Some(Ipv4Addr::new(192, 168, 1, 20)));
        assert_eq!(choose(&list, Some(7)), Some(Ipv4Addr::new(172, 20, 0, 1)));
        assert_eq!(choose(&[], None), None);
    }

    #[test]
    fn finds_something_or_nothing() {
        // on a PC with no network this is None; it must not crash either way
        if let Some(ip) = lan_ip() {
            assert!(!ip.is_loopback());
        }
    }
}
