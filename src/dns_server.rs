use anyhow::Result;
use esp_idf_svc::handle::RawHandle;
use esp_idf_svc::netif::EspNetif;
use esp_idf_sys as sys;
use log::{info, warn};
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};

pub struct DnsServer {
    hostname_map: Arc<Mutex<HashMap<String, Ipv4Addr>>>,
}

impl DnsServer {
    pub fn new() -> Self {
        Self {
            hostname_map: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Register a hostname with its IP address
    pub fn register_hostname(&self, hostname: String, ip: Ipv4Addr) {
        let mut map = self.hostname_map.lock().unwrap();
        let clean_hostname = hostname
            .to_lowercase()
            .trim_end_matches(".local")
            .to_string();
        map.insert(clean_hostname.clone(), ip);
        info!("DNS: Registered {}.local -> {}", clean_hostname, ip);
    }

    /// Remove a hostname from DNS
    pub fn unregister_hostname(&self, hostname: &str) {
        let mut map = self.hostname_map.lock().unwrap();
        let clean_hostname = hostname
            .to_lowercase()
            .trim_end_matches(".local")
            .to_string();
        if map.remove(&clean_hostname).is_some() {
            info!("DNS: Unregistered {}.local", clean_hostname);
        }
    }

    /// Get IP for hostname
    pub fn resolve_hostname(&self, hostname: &str) -> Option<Ipv4Addr> {
        let map = self.hostname_map.lock().unwrap();
        let clean_hostname = hostname
            .to_lowercase()
            .trim_end_matches(".local")
            .to_string();
        map.get(&clean_hostname).copied()
    }

    /// List all registered hostnames
    pub fn list_hostnames(&self) -> Vec<(String, Ipv4Addr)> {
        let map = self.hostname_map.lock().unwrap();
        map.iter()
            .map(|(k, v)| (format!("{}.local", k), *v))
            .collect()
    }

    /// Start the DNS server functionality
    pub fn start(&self, _ap_netif: &EspNetif) -> Result<()> {
        // For ESP-IDF, we'll rely on mDNS for .local domain resolution
        // The built-in DHCP server will provide basic DNS forwarding
        info!("DNS server service started (using mDNS for .local domains)");
        Ok(())
    }

    /// Configure DHCP to advertise this router as DNS server
    pub fn configure_dhcp_dns(&self, ap_netif: &EspNetif) -> Result<()> {
        unsafe {
            // Get AP IP info
            let mut ip_info: sys::esp_netif_ip_info_t = std::mem::zeroed();
            let result = sys::esp_netif_get_ip_info(ap_netif.handle(), &mut ip_info);

            if result != sys::ESP_OK {
                return Err(anyhow::anyhow!(
                    "Failed to get AP IP for DHCP config: {}",
                    result
                ));
            }

            // Configure DHCP to provide DNS server option
            // Point clients to use this router as their DNS server
            let dns_addr = ip_info.ip;

            // Set DHCP option 6 (DNS server)
            let result = sys::esp_netif_dhcps_option(
                ap_netif.handle(),
                sys::esp_netif_dhcp_option_mode_t_ESP_NETIF_OP_SET,
                sys::esp_netif_dhcp_option_id_t_ESP_NETIF_DOMAIN_NAME_SERVER,
                &dns_addr as *const _ as *mut _,
                std::mem::size_of::<sys::esp_ip4_addr_t>() as u32,
            );

            if result != sys::ESP_OK {
                warn!(
                    "Failed to configure DHCP DNS option: {} (this may be normal)",
                    result
                );
                // Don't fail completely as this is not critical
            } else {
                info!(
                    "DHCP configured to advertise {}.{}.{}.{} as DNS server",
                    (dns_addr.addr & 0xFF),
                    ((dns_addr.addr >> 8) & 0xFF),
                    ((dns_addr.addr >> 16) & 0xFF),
                    ((dns_addr.addr >> 24) & 0xFF)
                );
            }
        }

        Ok(())
    }

    /// Register hostname with validation and sanitization
    pub fn register_hostname_safe(&self, hostname: &str, ip: Ipv4Addr) -> Result<String> {
        let sanitized = Self::sanitize_hostname(hostname);

        if !Self::is_valid_hostname(&sanitized) {
            return Err(anyhow::anyhow!(
                "Invalid hostname after sanitization: {}",
                sanitized
            ));
        }

        self.register_hostname(sanitized.clone(), ip);
        Ok(sanitized)
    }

    /// Sanitize hostname to be DNS-compatible
    fn sanitize_hostname(hostname: &str) -> String {
        hostname
            .to_lowercase()
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' {
                    c
                } else {
                    '-'
                }
            })
            .collect::<String>()
            .trim_matches('-')
            .chars()
            .take(63) // DNS label limit
            .collect()
    }

    /// Validate hostname format
    fn is_valid_hostname(hostname: &str) -> bool {
        if hostname.is_empty() || hostname.len() > 63 {
            return false;
        }

        hostname
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
            && !hostname.starts_with('-')
            && !hostname.ends_with('-')
    }

    /// Register a device with MAC and friendly name
    pub fn register_device(
        &self,
        mac: [u8; 6],
        friendly_name: &str,
        ip: Ipv4Addr,
    ) -> Result<String> {
        let base_hostname = if Self::is_valid_hostname(friendly_name) {
            Self::sanitize_hostname(friendly_name)
        } else {
            // Generate fallback hostname from MAC
            format!("device-{:02x}{:02x}{:02x}", mac[3], mac[4], mac[5])
        };

        // Ensure uniqueness by checking if hostname already exists
        let final_hostname = {
            let map = self.hostname_map.lock().unwrap();
            let mut candidate = base_hostname.clone();
            let mut counter = 1;

            while map.contains_key(&candidate) {
                candidate = format!("{}-{}", base_hostname, counter);
                counter += 1;
                if counter > 99 {
                    // Fallback to MAC-based name if too many conflicts
                    candidate = format!(
                        "device-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
                        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
                    );
                    break;
                }
            }
            candidate
        };

        self.register_hostname(final_hostname.clone(), ip);
        Ok(final_hostname)
    }
}
