use anyhow::Result;
use log::info;
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct MdnsService {
    hostname_map: Arc<Mutex<HashMap<String, Ipv4Addr>>>,
    is_initialized: bool,
}

impl MdnsService {
    pub fn new() -> Self {
        Self {
            hostname_map: Arc::new(Mutex::new(HashMap::new())),
            is_initialized: false,
        }
    }

    /// Initialize mDNS service (simplified version)
    pub fn init(&mut self) -> Result<()> {
        if self.is_initialized {
            info!("mDNS service already initialized");
            return Ok(());
        }

        // Since mDNS functions are not available in current ESP-IDF bindings,
        // we'll maintain our own hostname registry for now
        self.is_initialized = true;
        info!("mDNS service initialized (local registry mode)");
        Ok(())
    }

    /// Register a hostname in our local registry
    pub fn register_hostname(&self, hostname: String, ip: Ipv4Addr) -> Result<()> {
        if !self.is_initialized {
            return Err(anyhow::anyhow!("mDNS service not initialized"));
        }

        let sanitized_hostname = Self::sanitize_hostname(&hostname);

        // Store in our local map
        {
            let mut map = self.hostname_map.lock().unwrap();
            map.insert(sanitized_hostname.clone(), ip);
        }

        info!("mDNS: Registered {}.local -> {}", sanitized_hostname, ip);
        Ok(())
    }

    /// Unregister a hostname from our local registry
    pub fn unregister_hostname(&self, hostname: &str) -> Result<()> {
        if !self.is_initialized {
            return Err(anyhow::anyhow!("mDNS service not initialized"));
        }

        let sanitized_hostname = Self::sanitize_hostname(hostname);

        // Remove from our local map
        {
            let mut map = self.hostname_map.lock().unwrap();
            if map.remove(&sanitized_hostname).is_some() {
                info!("mDNS: Unregistered {}.local", sanitized_hostname);
            }
        }

        Ok(())
    }

    /// Query for a hostname (for testing purposes)
    pub fn query_hostname(&self, hostname: &str) -> Option<Ipv4Addr> {
        let map = self.hostname_map.lock().unwrap();
        let sanitized_hostname = Self::sanitize_hostname(hostname);
        map.get(&sanitized_hostname).copied()
    }

    /// List all registered hostnames
    pub fn list_hostnames(&self) -> Vec<(String, Ipv4Addr)> {
        let map = self.hostname_map.lock().unwrap();
        map.iter()
            .map(|(hostname, ip)| (format!("{}.local", hostname), *ip))
            .collect()
    }

    /// Sanitize hostname to be DNS-compatible
    fn sanitize_hostname(hostname: &str) -> String {
        hostname
            .to_lowercase()
            .trim_end_matches(".local")
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

    /// Register a device with validation
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

        self.register_hostname(final_hostname.clone(), ip)?;

        info!(
            "Device registered: MAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} -> {}.local ({})",
            mac[0], mac[1], mac[2], mac[3], mac[4], mac[5], final_hostname, ip
        );

        Ok(final_hostname)
    }

    /// Stop mDNS service
    pub fn stop(&mut self) -> Result<()> {
        if !self.is_initialized {
            return Ok(());
        }

        // Clear the hostname map
        {
            let mut map = self.hostname_map.lock().unwrap();
            map.clear();
        }

        self.is_initialized = false;
        info!("mDNS service stopped");
        Ok(())
    }

    /// Get number of registered hostnames
    pub fn get_hostname_count(&self) -> usize {
        let map = self.hostname_map.lock().unwrap();
        map.len()
    }

    /// Print all registered hostnames to console
    pub fn print_hostnames(&self) {
        let hostnames = self.list_hostnames();
        if hostnames.is_empty() {
            info!("No hostnames registered");
        } else {
            info!("Registered hostnames ({}):", hostnames.len());
            for (hostname, ip) in hostnames {
                info!("  {} -> {}", hostname, ip);
            }
        }
    }
}

impl Drop for MdnsService {
    fn drop(&mut self) {
        if self.is_initialized {
            let _ = self.stop();
        }
    }
}
