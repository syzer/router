use anyhow::Result;
use log::{info, warn};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// MAC address to hostname mapping configuration
#[derive(Debug, Clone)]
pub struct MacHostnameConfig {
    /// Static MAC to hostname mappings
    mappings: Arc<Mutex<HashMap<[u8; 6], String>>>,
    /// Reserved hostnames (cannot be auto-assigned)
    reserved_hostnames: Arc<Mutex<HashMap<String, [u8; 6]>>>,
}

impl Default for MacHostnameConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl MacHostnameConfig {
    /// Create a new MAC hostname configuration
    pub fn new() -> Self {
        Self {
            mappings: Arc::new(Mutex::new(HashMap::new())),
            reserved_hostnames: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create with predefined mappings
    pub fn with_mappings(mappings: HashMap<[u8; 6], String>) -> Self {
        let reserved = mappings
            .iter()
            .map(|(mac, hostname)| (hostname.clone(), *mac))
            .collect();

        Self {
            mappings: Arc::new(Mutex::new(mappings)),
            reserved_hostnames: Arc::new(Mutex::new(reserved)),
        }
    }

    /// Add a static MAC to hostname mapping
    pub fn add_mapping(&self, mac: [u8; 6], hostname: String) -> Result<()> {
        let clean_hostname = Self::sanitize_hostname(&hostname);

        if !Self::is_valid_hostname(&clean_hostname) {
            return Err(anyhow::anyhow!("Invalid hostname: {}", hostname));
        }

        // Check if hostname is already reserved by another MAC
        {
            let reserved = self.reserved_hostnames.lock().unwrap();
            if let Some(&existing_mac) = reserved.get(&clean_hostname) {
                if existing_mac != mac {
                    return Err(anyhow::anyhow!(
                        "Hostname '{}' is already reserved for MAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                        clean_hostname,
                        existing_mac[0], existing_mac[1], existing_mac[2],
                        existing_mac[3], existing_mac[4], existing_mac[5]
                    ));
                }
            }
        }

        // Add the mapping
        {
            let mut mappings = self.mappings.lock().unwrap();
            mappings.insert(mac, clean_hostname.clone());
        }

        // Reserve the hostname
        {
            let mut reserved = self.reserved_hostnames.lock().unwrap();
            reserved.insert(clean_hostname.clone(), mac);
        }

        info!(
            "Added MAC mapping: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} -> {}.local",
            mac[0], mac[1], mac[2], mac[3], mac[4], mac[5], clean_hostname
        );

        Ok(())
    }

    /// Remove a MAC to hostname mapping
    pub fn remove_mapping(&self, mac: [u8; 6]) -> Option<String> {
        let hostname = {
            let mut mappings = self.mappings.lock().unwrap();
            mappings.remove(&mac)
        };

        if let Some(ref hostname) = hostname {
            let mut reserved = self.reserved_hostnames.lock().unwrap();
            reserved.remove(hostname);

            info!(
                "Removed MAC mapping: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} -> {}.local",
                mac[0], mac[1], mac[2], mac[3], mac[4], mac[5], hostname
            );
        }

        hostname
    }

    /// Get hostname for a MAC address
    pub fn get_hostname(&self, mac: [u8; 6]) -> Option<String> {
        let mappings = self.mappings.lock().unwrap();
        mappings.get(&mac).cloned()
    }

    /// Get MAC address for a hostname
    pub fn get_mac(&self, hostname: &str) -> Option<[u8; 6]> {
        let clean_hostname = Self::sanitize_hostname(hostname);
        let reserved = self.reserved_hostnames.lock().unwrap();
        reserved.get(&clean_hostname).copied()
    }

    /// Check if a hostname is reserved
    pub fn is_hostname_reserved(&self, hostname: &str) -> bool {
        let clean_hostname = Self::sanitize_hostname(hostname);
        let reserved = self.reserved_hostnames.lock().unwrap();
        reserved.contains_key(&clean_hostname)
    }

    /// Check if a MAC has a static mapping
    pub fn has_static_mapping(&self, mac: [u8; 6]) -> bool {
        let mappings = self.mappings.lock().unwrap();
        mappings.contains_key(&mac)
    }

    /// List all static mappings
    pub fn list_mappings(&self) -> Vec<([u8; 6], String)> {
        let mappings = self.mappings.lock().unwrap();
        mappings
            .iter()
            .map(|(&mac, hostname)| (mac, hostname.clone()))
            .collect()
    }

    /// Get total number of mappings
    pub fn mapping_count(&self) -> usize {
        let mappings = self.mappings.lock().unwrap();
        mappings.len()
    }

    /// Clear all mappings
    pub fn clear_mappings(&self) {
        {
            let mut mappings = self.mappings.lock().unwrap();
            mappings.clear();
        }
        {
            let mut reserved = self.reserved_hostnames.lock().unwrap();
            reserved.clear();
        }
        info!("Cleared all MAC hostname mappings");
    }

    /// Load mappings from a configuration string
    /// Format: "MAC1:hostname1,MAC2:hostname2,..."
    /// MAC format: "aa:bb:cc:dd:ee:ff"
    pub fn load_from_config(&self, config_str: &str) -> Result<usize> {
        let mut loaded = 0;

        for entry in config_str.split(',') {
            let entry = entry.trim();
            if entry.is_empty() {
                continue;
            }

            let parts: Vec<&str> = entry.split(':').collect();
            if parts.len() != 7 {
                // 6 MAC parts + 1 hostname
                warn!("Invalid MAC hostname config entry: {}", entry);
                continue;
            }

            // Parse MAC address (first 6 parts)
            let mac_result: Result<Vec<u8>, _> = parts[0..6]
                .iter()
                .map(|s| u8::from_str_radix(s, 16))
                .collect();

            match mac_result {
                Ok(mac_vec) if mac_vec.len() == 6 => {
                    let mac = [
                        mac_vec[0], mac_vec[1], mac_vec[2], mac_vec[3], mac_vec[4], mac_vec[5],
                    ];
                    let hostname = parts[6].to_string();

                    match self.add_mapping(mac, hostname) {
                        Ok(()) => loaded += 1,
                        Err(e) => warn!("Failed to add mapping for {}: {}", entry, e),
                    }
                }
                _ => warn!("Invalid MAC address in config entry: {}", entry),
            }
        }

        info!("Loaded {} MAC hostname mappings from config", loaded);
        Ok(loaded)
    }

    /// Export mappings to configuration string
    pub fn export_to_config(&self) -> String {
        let mappings = self.list_mappings();
        mappings
            .iter()
            .map(|(mac, hostname)| {
                format!(
                    "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{}",
                    mac[0], mac[1], mac[2], mac[3], mac[4], mac[5], hostname
                )
            })
            .collect::<Vec<_>>()
            .join(",")
    }

    /// Print all current mappings
    pub fn print_mappings(&self) {
        let mappings = self.list_mappings();
        if mappings.is_empty() {
            info!("No static MAC hostname mappings configured");
        } else {
            info!("Static MAC hostname mappings ({}):", mappings.len());
            for (mac, hostname) in mappings {
                info!(
                    "  {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} -> {}.local",
                    mac[0], mac[1], mac[2], mac[3], mac[4], mac[5], hostname
                );
            }
        }
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

    /// Format MAC address for display
    #[allow(dead_code)]
    fn format_mac(mac: [u8; 6]) -> String {
        format!(
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
        )
    }
}

/// Predefined static mappings builder
pub struct StaticMappingsBuilder {
    mappings: HashMap<[u8; 6], String>,
}

impl StaticMappingsBuilder {
    pub fn new() -> Self {
        Self {
            mappings: HashMap::new(),
        }
    }

    /// Add a mapping by MAC string and hostname
    pub fn add(mut self, mac_str: &str, hostname: &str) -> Result<Self> {
        let mac = Self::parse_mac(mac_str)?;
        let clean_hostname = MacHostnameConfig::sanitize_hostname(hostname);

        if !MacHostnameConfig::is_valid_hostname(&clean_hostname) {
            return Err(anyhow::anyhow!("Invalid hostname: {}", hostname));
        }

        self.mappings.insert(mac, clean_hostname);
        Ok(self)
    }

    /// Add a mapping by MAC bytes and hostname
    pub fn add_mac(mut self, mac: [u8; 6], hostname: &str) -> Result<Self> {
        let clean_hostname = MacHostnameConfig::sanitize_hostname(hostname);

        if !MacHostnameConfig::is_valid_hostname(&clean_hostname) {
            return Err(anyhow::anyhow!("Invalid hostname: {}", hostname));
        }

        self.mappings.insert(mac, clean_hostname);
        Ok(self)
    }

    /// Build the configuration
    pub fn build(self) -> MacHostnameConfig {
        MacHostnameConfig::with_mappings(self.mappings)
    }

    /// Parse MAC address from string
    fn parse_mac(mac_str: &str) -> Result<[u8; 6]> {
        let parts: Result<Vec<u8>, _> = mac_str
            .split(':')
            .map(|s| u8::from_str_radix(s, 16))
            .collect();

        match parts {
            Ok(bytes) if bytes.len() == 6 => {
                Ok([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5]])
            }
            _ => Err(anyhow::anyhow!("Invalid MAC address format: {}", mac_str)),
        }
    }
}

impl Default for StaticMappingsBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Create example static mappings
pub fn create_example_mappings() -> Result<MacHostnameConfig> {
    StaticMappingsBuilder::new()
        .add("aa:bb:cc:dd:ee:ff", "my-laptop")?
        .add("11:22:33:44:55:66", "raspberry-pi")?
        .add("77:88:99:aa:bb:cc", "arduino-iot")?
        .add("dd:ee:ff:11:22:33", "security-camera")?
        .add("44:55:66:77:88:99", "smart-tv")?
        .build();

    Ok(StaticMappingsBuilder::new()
        .add("aa:bb:cc:dd:ee:ff", "my-laptop")?
        .add("11:22:33:44:55:66", "raspberry-pi")?
        .add("77:88:99:aa:bb:cc", "arduino-iot")?
        .add("dd:ee:ff:11:22:33", "security-camera")?
        .add("44:55:66:77:88:99", "smart-tv")?
        .build())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_and_get_mapping() {
        let config = MacHostnameConfig::new();
        let mac = [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff];

        config.add_mapping(mac, "test-device".to_string()).unwrap();
        assert_eq!(config.get_hostname(mac), Some("test-device".to_string()));
        assert_eq!(config.get_mac("test-device"), Some(mac));
    }

    #[test]
    fn test_hostname_sanitization() {
        let config = MacHostnameConfig::new();
        let mac = [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff];

        config.add_mapping(mac, "Test Device!".to_string()).unwrap();
        assert_eq!(config.get_hostname(mac), Some("test-device".to_string()));
    }

    #[test]
    fn test_duplicate_hostname_rejection() {
        let config = MacHostnameConfig::new();
        let mac1 = [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff];
        let mac2 = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];

        config.add_mapping(mac1, "test-device".to_string()).unwrap();
        assert!(config.add_mapping(mac2, "test-device".to_string()).is_err());
    }

    #[test]
    fn test_config_string_loading() {
        let config = MacHostnameConfig::new();
        let config_str = "aa:bb:cc:dd:ee:ff:laptop,11:22:33:44:55:66:raspberry";

        let loaded = config.load_from_config(config_str).unwrap();
        assert_eq!(loaded, 2);

        let mac1 = [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff];
        let mac2 = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];

        assert_eq!(config.get_hostname(mac1), Some("laptop".to_string()));
        assert_eq!(config.get_hostname(mac2), Some("raspberry".to_string()));
    }

    #[test]
    fn test_builder_pattern() {
        let config = StaticMappingsBuilder::new()
            .add("aa:bb:cc:dd:ee:ff", "test-device")
            .unwrap()
            .build();

        let mac = [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff];
        assert_eq!(config.get_hostname(mac), Some("test-device".to_string()));
    }
}
