//! Which peers may tell the limiter who the client is.
//!
//! `X-Forwarded-For` and `X-Real-IP` are plain request headers: any client can send them. They
//! say something only when a reverse proxy we run wrote them, so they are read only when the
//! connection's peer is one of the configured trusted proxies.

use std::net::IpAddr;

use axum::http::HeaderMap;

/// A malformed trusted-proxy entry.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid trusted proxy entry '{0}': expected an IP address or CIDR block")]
pub struct InvalidProxy(pub String);

/// One address block: an address and the number of leading bits that must match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Block {
    addr: IpAddr,
    prefix: u8,
}

impl Block {
    fn parse(raw: &str) -> Result<Self, InvalidProxy> {
        let invalid = || InvalidProxy(raw.to_string());
        let (addr, prefix) = match raw.split_once('/') {
            Some((addr, prefix)) => (addr, Some(prefix)),
            None => (raw, None),
        };
        let addr: IpAddr = addr.trim().parse().map_err(|_| invalid())?;
        let max = if addr.is_ipv4() { 32 } else { 128 };
        let prefix = match prefix {
            Some(p) => p.trim().parse::<u8>().map_err(|_| invalid())?,
            None => max,
        };
        if prefix > max {
            return Err(invalid());
        }
        Ok(Self { addr, prefix })
    }

    fn contains(&self, ip: IpAddr) -> bool {
        match (self.addr, canonical(ip)) {
            (IpAddr::V4(net), IpAddr::V4(ip)) => {
                masked(u32::from(net) as u128, self.prefix, 32)
                    == masked(u32::from(ip) as u128, self.prefix, 32)
            }
            (IpAddr::V6(net), IpAddr::V6(ip)) => {
                masked(u128::from(net), self.prefix, 128)
                    == masked(u128::from(ip), self.prefix, 128)
            }
            _ => false,
        }
    }
}

/// Keep the top `prefix` bits of a `width`-bit address.
fn masked(bits: u128, prefix: u8, width: u8) -> u128 {
    if prefix == 0 {
        return 0;
    }
    let shift = u32::from(width - prefix);
    (bits >> shift) << shift
}

/// An IPv4 address carried in IPv6 (`::ffff:a.b.c.d`, a dual-stack socket) compares as IPv4.
fn canonical(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map(IpAddr::V4).unwrap_or(ip),
        v4 => v4,
    }
}

/// The peers whose forwarding headers the limiter believes. Empty by default: the peer address
/// is the client.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrustedProxies(Vec<Block>);

impl TrustedProxies {
    /// Parse a comma-separated list of addresses and CIDR blocks, e.g.
    /// `"172.16.0.0/12, 10.0.0.0/8, ::1"`. An empty string trusts no one.
    ///
    /// # Errors
    /// [`InvalidProxy`] naming the first entry that is not an address or a block.
    pub fn parse(list: &str) -> Result<Self, InvalidProxy> {
        list.split(',')
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
            .map(Block::parse)
            .collect::<Result<Vec<_>, _>>()
            .map(Self)
    }

    /// Whether no proxy is trusted.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether `ip` is a trusted proxy.
    pub fn contains(&self, ip: IpAddr) -> bool {
        self.0.iter().any(|block| block.contains(ip))
    }

    /// The client address of a request that arrived from `peer`.
    ///
    /// From an untrusted peer, the peer itself. From a trusted one, the rightmost
    /// `X-Forwarded-For` entry that is not a trusted proxy — each proxy appends the address it
    /// received from, so entries to the left of that one were written by the client and prove
    /// nothing — else `X-Real-IP`, else the peer.
    pub fn client_ip(&self, peer: IpAddr, headers: &HeaderMap) -> IpAddr {
        let peer = canonical(peer);
        if !self.contains(peer) {
            return peer;
        }
        let forwarded = headers
            .get_all("x-forwarded-for")
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|v| v.split(','))
            .map(str::trim)
            .collect::<Vec<_>>();
        if !forwarded.is_empty() {
            for entry in forwarded.iter().rev() {
                match entry.parse::<IpAddr>() {
                    Ok(ip) if self.contains(ip) => continue,
                    Ok(ip) => return canonical(ip),
                    // An entry we cannot read: stop here rather than reach past it to a value
                    // the client wrote.
                    Err(_) => return peer,
                }
            }
            return peer;
        }
        headers
            .get("x-real-ip")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<IpAddr>().ok())
            .map(canonical)
            .unwrap_or(peer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_match_by_prefix() {
        let proxies = TrustedProxies::parse("172.16.0.0/12, fd00::/8, 127.0.0.1").unwrap();
        assert!(proxies.contains("172.18.0.5".parse().unwrap()));
        assert!(!proxies.contains("172.32.0.1".parse().unwrap()));
        assert!(proxies.contains("fd12::1".parse().unwrap()));
        assert!(proxies.contains("127.0.0.1".parse().unwrap()));
        assert!(proxies.contains("::ffff:172.18.0.5".parse().unwrap()));
        assert!(!proxies.contains("127.0.0.2".parse().unwrap()));
        assert!(TrustedProxies::parse("0.0.0.0/0")
            .unwrap()
            .contains("8.8.8.8".parse().unwrap()));
    }
}
