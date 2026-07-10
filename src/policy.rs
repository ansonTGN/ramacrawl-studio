use crate::config::CrawlerConfig;
use anyhow::{Context, Result, bail};
use regex::Regex;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use tokio::net::lookup_host;
use url::Url;

#[derive(Debug, Clone)]
pub struct UrlPolicy {
    allowed_domains: Vec<String>,
    denied_path_prefixes: Vec<String>,
    allowed_ports: Vec<u16>,
    allow_subdomains: bool,
    allow_private_networks: bool,
    include: Option<Regex>,
    exclude: Option<Regex>,
}

impl UrlPolicy {
    pub fn from_config(cfg: &CrawlerConfig) -> Result<Self> {
        let allowed_domains = cfg
            .allowed_domains
            .iter()
            .map(|domain| domain.trim().trim_end_matches('.').to_ascii_lowercase())
            .collect();
        let include = cfg
            .include_url_regex
            .as_deref()
            .map(Regex::new)
            .transpose()
            .context("invalid include_url_regex")?;
        let exclude = cfg
            .exclude_url_regex
            .as_deref()
            .map(Regex::new)
            .transpose()
            .context("invalid exclude_url_regex")?;
        Ok(Self {
            allowed_domains,
            denied_path_prefixes: cfg.denied_path_prefixes.clone(),
            allowed_ports: cfg.allowed_ports.clone(),
            allow_subdomains: cfg.allow_subdomains,
            allow_private_networks: cfg.allow_private_networks,
            include,
            exclude,
        })
    }

    pub fn validate_url(&self, url: &Url) -> Result<()> {
        if !matches!(url.scheme(), "http" | "https") {
            bail!("scheme is not allowed: {}", url.scheme());
        }
        if !url.username().is_empty() || url.password().is_some() {
            bail!("credentials embedded in URLs are forbidden");
        }
        let host = url
            .host_str()
            .context("URL has no host")?
            .to_ascii_lowercase();
        let port = url
            .port_or_known_default()
            .context("URL has no usable port")?;
        if !self.allowed_ports.contains(&port) {
            bail!("destination port is outside the allowlist: {port}");
        }
        if !self.host_allowed(&host) {
            bail!("host is outside the allowlist: {host}");
        }
        if self
            .denied_path_prefixes
            .iter()
            .any(|prefix| url.path().starts_with(prefix))
        {
            bail!("URL path is denied by policy: {}", url.path());
        }
        let normalized = url.as_str();
        if let Some(include) = &self.include
            && !include.is_match(normalized)
        {
            bail!("URL does not match include_url_regex");
        }
        if let Some(exclude) = &self.exclude
            && exclude.is_match(normalized)
        {
            bail!("URL matches exclude_url_regex");
        }
        Ok(())
    }

    pub async fn validate_network_destination(&self, url: &Url) -> Result<()> {
        self.validate_url(url)?;
        if self.allow_private_networks {
            return Ok(());
        }
        let host = url.host_str().context("URL has no host")?;
        if let Ok(ip) = host.parse::<IpAddr>() {
            if is_forbidden_ip(ip) {
                bail!("private or special-purpose destination is forbidden: {ip}");
            }
            return Ok(());
        }
        let port = url
            .port_or_known_default()
            .context("URL has no usable port")?;
        let addresses: Vec<_> = lookup_host((host, port))
            .await
            .with_context(|| format!("DNS resolution failed for {host}"))?
            .collect();
        if addresses.is_empty() {
            bail!("DNS returned no address for {host}");
        }
        if let Some(address) = addresses
            .iter()
            .find(|address| is_forbidden_ip(address.ip()))
        {
            bail!("DNS resolved {host} to forbidden address {}", address.ip());
        }
        Ok(())
    }

    pub fn host_allowed(&self, host: &str) -> bool {
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        self.allowed_domains.iter().any(|allowed| {
            host == *allowed
                || (self.allow_subdomains
                    && host.len() > allowed.len()
                    && host.ends_with(allowed)
                    && host.as_bytes()[host.len() - allowed.len() - 1] == b'.')
        })
    }
}

pub fn is_forbidden_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_forbidden_v4(ip),
        IpAddr::V6(ip) => is_forbidden_v6(ip),
    }
}

fn is_forbidden_v4(ip: Ipv4Addr) -> bool {
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
        || ip.is_multicast()
        || ip.octets()[0] == 0
        || ip.octets()[0] >= 240
        || (ip.octets()[0] == 100 && (64..=127).contains(&ip.octets()[1]))
        || (ip.octets()[0] == 198 && (18..=19).contains(&ip.octets()[1]))
}

fn is_forbidden_v6(ip: Ipv6Addr) -> bool {
    if let Some(ipv4) = ip.to_ipv4_mapped() {
        return is_forbidden_v4(ipv4);
    }
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (ip.segments()[0] & 0xfe00) == 0xfc00
        || (ip.segments()[0] & 0xffc0) == 0xfe80
        || (ip.segments()[0] == 0x2001 && ip.segments()[1] == 0x0db8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subdomain_boundary_is_enforced() {
        let policy = UrlPolicy {
            allowed_domains: vec!["example.com".to_owned()],
            denied_path_prefixes: vec![],
            allowed_ports: vec![80, 443],
            allow_subdomains: true,
            allow_private_networks: false,
            include: None,
            exclude: None,
        };
        assert!(policy.host_allowed("docs.example.com"));
        assert!(!policy.host_allowed("notexample.com"));
    }

    #[test]
    fn private_ranges_are_blocked() {
        assert!(is_forbidden_ip(
            "127.0.0.1"
                .parse()
                .unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
        ));
        assert!(is_forbidden_ip(
            "10.0.0.1"
                .parse()
                .unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
        ));
        assert!(!is_forbidden_ip(
            "8.8.8.8"
                .parse()
                .unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
        ));
    }
}
