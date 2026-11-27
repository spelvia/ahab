//! Addresses: ranges of them (`trusted_proxies`), and which networks an
//! address belongs to.

use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::str::FromStr;

/// A range of addresses, written as an address (`127.0.0.1`) or in CIDR
/// notation (`10.0.0.0/8`, `fd7a:115c:a1e0::/48`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpRange {
    network: IpAddr,
    prefix: u8,
}

impl IpRange {
    pub const fn v4(a: u8, b: u8, c: u8, d: u8, prefix: u8) -> Self {
        Self {
            network: IpAddr::V4(Ipv4Addr::new(a, b, c, d)),
            prefix,
        }
    }

    pub const fn v6(segments: [u16; 8], prefix: u8) -> Self {
        let s = segments;
        Self {
            network: IpAddr::V6(Ipv6Addr::new(
                s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7],
            )),
            prefix,
        }
    }

    /// Whether `address` is in this range. An IPv4 address mapped into IPv6
    /// (`::ffff:10.0.0.1`, as a server listening on `[::]` sees IPv4 peers)
    /// counts as the IPv4 address.
    pub fn contains(&self, address: IpAddr) -> bool {
        match (self.network, address.to_canonical()) {
            (IpAddr::V4(network), IpAddr::V4(address)) => {
                let mask = u32::MAX
                    .checked_shl(32 - u32::from(self.prefix))
                    .unwrap_or(0);
                u32::from(network) & mask == u32::from(address) & mask
            }
            (IpAddr::V6(network), IpAddr::V6(address)) => {
                let mask = u128::MAX
                    .checked_shl(128 - u32::from(self.prefix))
                    .unwrap_or(0);
                u128::from(network) & mask == u128::from(address) & mask
            }
            _ => false,
        }
    }
}

impl FromStr for IpRange {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let text = text.trim();
        let invalid = || format!("{text:?} is not an address or a range such as 10.0.0.0/8");
        let (address, prefix) = match text.split_once('/') {
            Some((address, prefix)) => (address, Some(prefix)),
            None => (text, None),
        };
        let network: IpAddr = address
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse()
            .map_err(|_| invalid())?;
        let bits = if network.is_ipv4() { 32 } else { 128 };
        let prefix = match prefix {
            Some(prefix) => prefix.parse::<u8>().map_err(|_| invalid())?,
            None => bits,
        };
        if prefix > bits {
            return Err(invalid());
        }
        Ok(Self { network, prefix })
    }
}

impl fmt::Display for IpRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.network, self.prefix)
    }
}

/// The loopback ranges: the default `trusted_proxies`.
pub const LOOPBACK: [IpRange; 2] = [
    IpRange::v4(127, 0, 0, 0, 8),
    IpRange::v6([0, 0, 0, 0, 0, 0, 0, 1], 128),
];

/// Home and office networks, and link-local addresses.
const PRIVATE: [IpRange; 6] = [
    IpRange::v4(10, 0, 0, 0, 8),
    IpRange::v4(172, 16, 0, 0, 12),
    IpRange::v4(192, 168, 0, 0, 16),
    IpRange::v4(169, 254, 0, 0, 16),
    IpRange::v6([0xfc00, 0, 0, 0, 0, 0, 0, 0], 7),
    IpRange::v6([0xfe80, 0, 0, 0, 0, 0, 0, 0], 10),
];

/// Tailscale's and Headscale's addresses, which `WireGuard` encrypts. The
/// IPv6 range sits inside `fc00::/7`, so it is checked first.
const TAILNET: [IpRange; 2] = [
    IpRange::v4(100, 64, 0, 0, 10),
    IpRange::v6([0xfd7a, 0x115c, 0xa1e0, 0, 0, 0, 0, 0], 48),
];

/// The kind of network an address is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Network {
    /// This computer.
    Loopback,
    /// A tailnet: private, and encrypted by `WireGuard`.
    Tailnet,
    /// A home or office network.
    Private,
    /// Anything else: the internet.
    Public,
}

impl Network {
    pub fn of(address: IpAddr) -> Self {
        let address = address.to_canonical();
        if LOOPBACK.iter().any(|range| range.contains(address)) {
            Self::Loopback
        } else if TAILNET.iter().any(|range| range.contains(address)) {
            Self::Tailnet
        } else if PRIVATE.iter().any(|range| range.contains(address)) {
            Self::Private
        } else {
            Self::Public
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(text: &str) -> IpAddr {
        text.parse().unwrap()
    }

    #[test]
    fn parses_addresses_and_ranges() {
        let range: IpRange = "10.0.0.0/8".parse().unwrap();
        assert!(range.contains(ip("10.1.2.3")));
        assert!(!range.contains(ip("11.0.0.1")));
        let one: IpRange = "192.168.1.5".parse().unwrap();
        assert!(one.contains(ip("192.168.1.5")));
        assert!(!one.contains(ip("192.168.1.6")));
        let all: IpRange = "0.0.0.0/0".parse().unwrap();
        assert!(all.contains(ip("8.8.8.8")));
        let v6: IpRange = "[fd7a:115c:a1e0::]/48".parse().unwrap();
        assert!(v6.contains(ip("fd7a:115c:a1e0:ab12::1")));
        assert!("10.0.0.0/33".parse::<IpRange>().is_err());
        assert!("nonsense".parse::<IpRange>().is_err());
        assert_eq!(range.to_string(), "10.0.0.0/8");
    }

    #[test]
    fn mapped_ipv4_addresses_count_as_ipv4() {
        assert!(LOOPBACK[0].contains(ip("::ffff:127.0.0.1")));
        assert_eq!(Network::of(ip("::ffff:192.168.1.2")), Network::Private);
    }

    #[test]
    fn tells_networks_apart() {
        assert_eq!(Network::of(ip("127.0.0.1")), Network::Loopback);
        assert_eq!(Network::of(ip("::1")), Network::Loopback);
        assert_eq!(Network::of(ip("100.101.102.103")), Network::Tailnet);
        assert_eq!(Network::of(ip("fd7a:115c:a1e0::1")), Network::Tailnet);
        assert_eq!(Network::of(ip("192.168.178.40")), Network::Private);
        assert_eq!(Network::of(ip("172.20.0.4")), Network::Private);
        assert_eq!(Network::of(ip("fd00::4")), Network::Private);
        assert_eq!(Network::of(ip("172.32.0.1")), Network::Public);
        assert_eq!(Network::of(ip("8.8.8.8")), Network::Public);
        assert_eq!(Network::of(ip("2001:db8::1")), Network::Public);
    }
}
