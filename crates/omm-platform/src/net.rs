//! Sleep and uplink changes, polled: a cheap look every few seconds needs
//! neither D-Bus nor a netlink subscription.

/// Milliseconds the machine spent suspended since boot. Grows by the length
/// of every sleep, so a jump between two looks means it just woke up.
pub fn slept_ms() -> i64 {
    // The first clock keeps running while suspended, the second stops.
    #[cfg(target_os = "macos")]
    let (with, without) = (libc::CLOCK_MONOTONIC, libc::CLOCK_UPTIME_RAW);
    #[cfg(not(target_os = "macos"))]
    let (with, without) = (libc::CLOCK_BOOTTIME, libc::CLOCK_MONOTONIC);
    clock_ms(with) - clock_ms(without)
}

fn clock_ms(clock: libc::clockid_t) -> i64 {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    unsafe { libc::clock_gettime(clock, &mut ts) };
    ts.tv_sec as i64 * 1000 + ts.tv_nsec as i64 / 1_000_000
}

/// The machine's ways out, sorted: `iface via gateway` for every default
/// route and `iface address` for their IPv4 addresses. Another network
/// gives another list; an empty one means offline. Tunnels (mihomo's own
/// TUN among them) are left out. `None` where it cannot be told.
pub fn uplinks() -> Option<Vec<String>> {
    #[cfg(target_os = "macos")]
    {
        None
    }
    #[cfg(not(target_os = "macos"))]
    {
        let v4 = std::fs::read_to_string("/proc/net/route").ok()?;
        let v6 = std::fs::read_to_string("/proc/net/ipv6_route").unwrap_or_default();
        let tunnel = |iface: &str| std::path::Path::new("/sys/class/net").join(iface).join("tun_flags").exists();
        let routes: Vec<(String, String)> = default_routes(&v4, &v6).into_iter().filter(|(iface, _)| !tunnel(iface)).collect();
        let mut out: Vec<String> = routes.iter().map(|(iface, gw)| format!("{iface} via {gw}")).collect();
        let ifaces: Vec<&str> = routes.iter().map(|(iface, _)| iface.as_str()).collect();
        out.extend(ipv4_addresses().into_iter().filter(|(iface, _)| ifaces.contains(&iface.as_str())).map(|(iface, ip)| format!("{iface} {ip}")));
        out.sort();
        out.dedup();
        Some(out)
    }
}

/// `(iface, gateway)` of the default routes in `/proc/net/route` and
/// `/proc/net/ipv6_route`. IPv6 addresses are left to the caller's
/// discretion: privacy addresses rotate on their own, gateways do not.
pub fn default_routes(v4: &str, v6: &str) -> Vec<(String, String)> {
    const RTF_UP: u32 = 0x1;
    let mut out = Vec::new();
    for line in v4.lines().skip(1) {
        let f: Vec<&str> = line.split_whitespace().collect();
        let [iface, dest, gw, flags, _, _, _, mask, ..] = f[..] else { continue };
        let up = u32::from_str_radix(flags, 16).is_ok_and(|f| f & RTF_UP != 0);
        if dest != "00000000" || mask != "00000000" || !up {
            continue;
        }
        // Little-endian hex: 0100A8C0 is 192.168.0.1.
        let gw = u32::from_str_radix(gw, 16).map(|g| std::net::Ipv4Addr::from(g.swap_bytes()).to_string()).unwrap_or_default();
        out.push((iface.to_owned(), gw));
    }
    for line in v6.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        let [dest, plen, _, _, gw, _, _, _, flags, iface] = f[..] else { continue };
        let up = u32::from_str_radix(flags, 16).is_ok_and(|f| f & RTF_UP != 0);
        // Unreachable defaults in other tables hang off `lo`.
        if dest.bytes().any(|b| b != b'0') || plen != "00" || iface == "lo" || !up {
            continue;
        }
        let gw = u128::from_str_radix(gw, 16).map(|g| std::net::Ipv6Addr::from(g).to_string()).unwrap_or_default();
        out.push((iface.to_owned(), gw));
    }
    out
}

#[cfg(not(target_os = "macos"))]
fn ipv4_addresses() -> Vec<(String, std::net::Ipv4Addr)> {
    let mut out = Vec::new();
    let mut head: *mut libc::ifaddrs = std::ptr::null_mut();
    if unsafe { libc::getifaddrs(&mut head) } != 0 {
        return out;
    }
    let mut cur = head;
    while !cur.is_null() {
        let ifa = unsafe { &*cur };
        cur = ifa.ifa_next;
        if ifa.ifa_addr.is_null() || i32::from(unsafe { (*ifa.ifa_addr).sa_family }) != libc::AF_INET {
            continue;
        }
        let sin = unsafe { &*(ifa.ifa_addr as *const libc::sockaddr_in) };
        let name = unsafe { std::ffi::CStr::from_ptr(ifa.ifa_name) }.to_string_lossy().into_owned();
        out.push((name, std::net::Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr))));
    }
    unsafe { libc::freeifaddrs(head) };
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_routes_from_proc() {
        let v4 = "Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\t\tMTU\tWindow\tIRTT\n\
                  wlp5s0\t00000000\t0100A8C0\t0003\t0\t0\t600\t00000000\t0\t0\t0\n\
                  wlp5s0\t0000A8C0\t00000000\t0001\t0\t0\t600\t00FFFFFF\t0\t0\t0\n\
                  Meta\t000012C6\t00000000\t0001\t0\t0\t0\tFCFFFFFF\t0\t0\t0\n";
        let v6 = "00000000000000000000000000000000 00 00000000000000000000000000000000 00 fe800000000000000000000000000001 00000400 00000001 00000000 00000003 wlp5s0\n\
                  00000000000000000000000000000000 00 00000000000000000000000000000000 00 00000000000000000000000000000000 ffffffff 00000001 00000000 00200200 lo\n\
                  fe800000000000000000000000000000 40 00000000000000000000000000000000 00 00000000000000000000000000000000 00000100 00000001 00000000 00000001 Meta\n";
        assert_eq!(
            default_routes(v4, v6),
            vec![("wlp5s0".to_owned(), "192.168.0.1".to_owned()), ("wlp5s0".to_owned(), "fe80::1".to_owned())]
        );
        assert!(default_routes("Iface\tDestination\n", "").is_empty());
    }

    #[test]
    fn sleep_clock_is_monotonic() {
        let a = slept_ms();
        assert!(a >= 0);
        assert!(slept_ms() - a < 1000);
    }
}
