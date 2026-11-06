//! Per-IP rate limiting of posts, and client IP extraction behind trusted
//! reverse proxies.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::http::HeaderMap;
use ipnet::IpNet;

/// Remembers the last post time per client; a client may post once per `interval`.
pub struct RateLimiter {
    interval: Duration,
    last_post: Mutex<HashMap<IpAddr, Instant>>,
}

impl RateLimiter {
    /// `interval` of zero disables the limiter.
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            last_post: Mutex::new(HashMap::new()),
        }
    }

    /// Records a post attempt. Returns `false` if the client must wait.
    pub fn check(&self, ip: IpAddr) -> bool {
        self.check_at(ip, Instant::now())
    }

    pub fn check_at(&self, ip: IpAddr, now: Instant) -> bool {
        if self.interval.is_zero() {
            return true;
        }
        let key = bucket(ip);
        let mut map = self.last_post.lock().unwrap_or_else(|e| e.into_inner());
        match map.get(&key) {
            Some(&last) if now.saturating_duration_since(last) < self.interval => false,
            _ => {
                map.insert(key, now);
                true
            }
        }
    }

    /// Forgets clients whose interval has elapsed.
    pub fn prune(&self) {
        self.prune_at(Instant::now());
    }

    pub fn prune_at(&self, now: Instant) {
        let interval = self.interval;
        let mut map = self.last_post.lock().unwrap_or_else(|e| e.into_inner());
        map.retain(|_, &mut last| now.saturating_duration_since(last) < interval);
    }

    pub fn len(&self) -> usize {
        self.last_post
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// IPv6 clients usually control a whole /64, so they are limited per /64.
fn bucket(ip: IpAddr) -> IpAddr {
    match ip.to_canonical() {
        IpAddr::V6(v6) => {
            let s = v6.segments();
            IpAddr::V6(Ipv6Addr::new(s[0], s[1], s[2], s[3], 0, 0, 0, 0))
        }
        v4 => v4,
    }
}

/// Determines the client address. `X-Forwarded-For` is only honored when the
/// TCP peer is a trusted proxy; the right-most address that is not itself a
/// trusted proxy is the client (left-most entries are client-controlled).
pub fn client_ip(peer: IpAddr, headers: &HeaderMap, trusted: &[IpNet]) -> IpAddr {
    let peer = peer.to_canonical();
    let is_trusted = |ip: &IpAddr| trusted.iter().any(|net| net.contains(ip));
    if !is_trusted(&peer) {
        return peer;
    }
    let forwarded: Vec<&str> = headers
        .get_all("x-forwarded-for")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .map(str::trim)
        .collect();
    let mut client = peer;
    for entry in forwarded.iter().rev() {
        let Some(ip) = parse_forwarded(entry) else {
            // Garbage injected by someone: stop at the last address we trust.
            break;
        };
        client = ip.to_canonical();
        if !is_trusted(&client) {
            break;
        }
    }
    client
}

/// An `X-Forwarded-For` entry: a bare address, or one with a port as some
/// proxies send (`1.2.3.4:5678`, `[2001:db8::1]:443`, `[2001:db8::1]`).
fn parse_forwarded(entry: &str) -> Option<IpAddr> {
    entry
        .parse::<IpAddr>()
        .or_else(|_| entry.parse::<SocketAddr>().map(|s| s.ip()))
        .ok()
        .or_else(|| {
            entry
                .strip_prefix('[')?
                .strip_suffix(']')?
                .parse::<IpAddr>()
                .ok()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn nets() -> Vec<IpNet> {
        vec![
            "127.0.0.1/32".parse().unwrap(),
            "172.16.0.0/12".parse().unwrap(),
        ]
    }

    fn headers(values: &[&str]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for v in values {
            h.append("x-forwarded-for", HeaderValue::from_str(v).unwrap());
        }
        h
    }

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn forwarded_entries_with_ports() {
        assert_eq!(parse_forwarded("1.2.3.4:5678"), Some(ip("1.2.3.4")));
        assert_eq!(
            parse_forwarded("[2001:db8::1]:443"),
            Some(ip("2001:db8::1"))
        );
        assert_eq!(parse_forwarded("[2001:db8::1]"), Some(ip("2001:db8::1")));
        assert_eq!(parse_forwarded("2001:db8::1"), Some(ip("2001:db8::1")));
        assert_eq!(parse_forwarded("unknown"), None);
        let h = headers(&["9.9.9.9:1234"]);
        assert_eq!(client_ip(ip("127.0.0.1"), &h, &nets()), ip("9.9.9.9"));
    }

    #[test]
    fn untrusted_peer_ignores_header() {
        let h = headers(&["1.2.3.4"]);
        assert_eq!(client_ip(ip("8.8.8.8"), &h, &nets()), ip("8.8.8.8"));
    }

    #[test]
    fn trusted_peer_uses_rightmost_untrusted() {
        let h = headers(&["6.6.6.6, 1.2.3.4, 172.17.0.5"]);
        assert_eq!(client_ip(ip("127.0.0.1"), &h, &nets()), ip("1.2.3.4"));
        // Multiple header lines are concatenated in order.
        let h = headers(&["6.6.6.6", "1.2.3.4"]);
        assert_eq!(client_ip(ip("172.18.0.1"), &h, &nets()), ip("1.2.3.4"));
    }

    #[test]
    fn trusted_peer_without_header_is_client() {
        assert_eq!(
            client_ip(ip("127.0.0.1"), &HeaderMap::new(), &nets()),
            ip("127.0.0.1")
        );
    }

    #[test]
    fn all_trusted_chain_yields_leftmost() {
        let h = headers(&["172.16.0.9, 127.0.0.1"]);
        assert_eq!(client_ip(ip("127.0.0.1"), &h, &nets()), ip("172.16.0.9"));
    }

    #[test]
    fn garbage_stops_the_walk() {
        let h = headers(&["1.2.3.4, garbage"]);
        assert_eq!(client_ip(ip("127.0.0.1"), &h, &nets()), ip("127.0.0.1"));
    }

    #[test]
    fn ipv4_mapped_peer_is_canonicalized() {
        let h = headers(&["9.9.9.9"]);
        assert_eq!(
            client_ip(ip("::ffff:127.0.0.1"), &h, &nets()),
            ip("9.9.9.9")
        );
    }

    #[test]
    fn empty_trust_list_never_trusts() {
        let h = headers(&["9.9.9.9"]);
        assert_eq!(client_ip(ip("127.0.0.1"), &h, &[]), ip("127.0.0.1"));
    }

    #[test]
    fn limiter_enforces_interval() {
        let limiter = RateLimiter::new(Duration::from_secs(10));
        let t0 = Instant::now();
        assert!(limiter.check_at(ip("1.1.1.1"), t0));
        assert!(!limiter.check_at(ip("1.1.1.1"), t0 + Duration::from_secs(9)));
        assert!(limiter.check_at(ip("2.2.2.2"), t0 + Duration::from_secs(9)));
        assert!(limiter.check_at(ip("1.1.1.1"), t0 + Duration::from_secs(10)));
    }

    #[test]
    fn limiter_groups_ipv6_by_64() {
        let limiter = RateLimiter::new(Duration::from_secs(10));
        let t0 = Instant::now();
        assert!(limiter.check_at(ip("2001:db8:1:2::1"), t0));
        assert!(!limiter.check_at(ip("2001:db8:1:2:ffff::9"), t0));
        assert!(limiter.check_at(ip("2001:db8:1:3::1"), t0));
    }

    #[test]
    fn limiter_disabled_and_prune() {
        let disabled = RateLimiter::new(Duration::ZERO);
        assert!(disabled.check(ip("1.1.1.1")));
        assert!(disabled.check(ip("1.1.1.1")));

        let limiter = RateLimiter::new(Duration::from_secs(10));
        let t0 = Instant::now();
        limiter.check_at(ip("1.1.1.1"), t0);
        limiter.check_at(ip("2.2.2.2"), t0 + Duration::from_secs(5));
        limiter.prune_at(t0 + Duration::from_secs(12));
        assert_eq!(limiter.len(), 1);
        limiter.prune_at(t0 + Duration::from_secs(20));
        assert!(limiter.is_empty());
    }
}
