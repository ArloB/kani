//! Network-related security utilities.

use hickory_resolver::name_server::GenericConnector;
use hickory_resolver::proto::runtime::TokioRuntimeProvider;
use hickory_resolver::{TokioResolver, config::*};
use rquest::dns::{Addrs, Name, Resolve, Resolving};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use crate::error::Result;

/// Every private, reserved, or otherwise untrusted IP range.
fn is_forbidden_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()           
            || v4.is_private()         
            || v4.is_link_local()      
            || v4.is_broadcast()       
            || v4.is_unspecified()     
            || v4.is_documentation()   
            || v4.is_multicast()       
            || is_cgnat(v4)            
            || is_reserved(v4)         
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
            || v6.is_unspecified()
            || v6.is_multicast()
            || is_ipv4_mapped_private(v6)
        }
    }
}

fn is_cgnat(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    octets[0] == 100 && (octets[1] & 0xC0) == 64
}

fn is_reserved(ip: Ipv4Addr) -> bool {
    ip.octets()[0] >= 240
}

/// IPv4-mapped IPv6 (::ffff:0:0/96) can encode private IPv4 addresses.
fn is_ipv4_mapped_private(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_forbidden_ip(IpAddr::V4(v4));
    }
    false
}

/// A DNS resolver that validates every address before returning it.
#[derive(Clone)]
pub struct ValidatingResolver {
    inner: Arc<TokioResolver>,
}

impl ValidatingResolver {
    pub fn new() -> Result<Self> {
        let resolver = TokioResolver::builder_with_config(
            ResolverConfig::cloudflare(),
            GenericConnector::new(TokioRuntimeProvider::new())
        )
        .with_options(ResolverOpts::default())
        .build();

        Ok(Self { inner: Arc::new(resolver) })
    }
}

impl Resolve for ValidatingResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let resolver = self.inner.clone();

        Box::pin(async move {
            let lookup = resolver
                .lookup_ip(name.as_str())
                .await
                .map_err(|e| format!("DNS resolution failed: {}", e))?;

            let addrs: Vec<SocketAddr> = lookup
                .iter()
                .map(|ip| SocketAddr::new(ip, 0))
                .collect();

            if addrs.is_empty() {
                return Err("DNS returned no addresses".into());
            }

            for addr in &addrs {
                if is_forbidden_ip(addr.ip()) {
                    return Err(format!(
                        "Resolved address {} is in a forbidden range", addr.ip()
                    ).into());
                }
            }

            let addrs: Addrs = Box::new(addrs.into_iter());
            Ok(addrs)
        })
    }
}