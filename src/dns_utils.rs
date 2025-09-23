use anyhow::Result;
use log::info;
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};

/// DNS configuration and utilities for the ESP32 router
pub struct DnsConfig {
    /// Default domain suffix for local devices
    pub domain_suffix: String,
    /// DNS cache TTL in seconds
    pub cache_ttl: u32,
    /// Maximum number of hostnames to cache
    pub max_cache_entries: usize,
}

impl Default for DnsConfig {
    fn default() -> Self {
        Self {
            domain_suffix: ".local".to_string(),
            cache_ttl: 300, // 5 minutes
            max_cache_entries: 100,
        }
    }
}

/// DNS utilities for testing and configuration
pub struct DnsUtils;

impl DnsUtils {
    /// Validate if a hostname is valid according to DNS standards
    pub fn is_valid_hostname(hostname: &str) -> bool {
        if hostname.is_empty() || hostname.len() > 253 {
            return false;
        }

        // Split into labels and validate each
        for label in hostname.split('.') {
            if !Self::is_valid_label(label) {
                return false;
            }
        }

        true
    }

    /// Validate a DNS label (part of hostname between dots)
    fn is_valid_label(label: &str) -> bool {
        if label.is_empty() || label.len() > 63 {
            return false;
        }

        // Must start and end with alphanumeric
        let chars: Vec<char> = label.chars().collect();
        if !chars[0].is_ascii_alphanumeric() || !chars[chars.len() - 1].is_ascii_alphanumeric() {
            return false;
        }

        // All characters must be alphanumeric or hyphen
        chars.iter().all(|&c| c.is_ascii_alphanumeric() || c == '-')
    }

    /// Sanitize a string to make it a valid hostname
    pub fn sanitize_hostname(input: &str) -> String {
        let mut result = String::new();
        let mut in_label = false;

        for c in input.chars() {
            if c.is_ascii_alphanumeric() {
                result.push(c.to_ascii_lowercase());
                in_label = true;
            } else if c == '.' && in_label {
                result.push('.');
                in_label = false;
            } else if (c == '-' || c == '_') && in_label {
                result.push('-');
            } else if c.is_ascii_whitespace() && in_label {
                result.push('-');
            }
            // Skip other characters
        }

        // Remove trailing dots and dashes
        result
            .trim_end_matches('.')
            .trim_end_matches('-')
            .to_string()
    }

    /// Generate a hostname from MAC address
    pub fn hostname_from_mac(mac: [u8; 6], prefix: &str) -> String {
        format!("{}-{:02x}{:02x}{:02x}", prefix, mac[3], mac[4], mac[5])
    }

    /// Generate a full hostname from MAC address with fallback
    pub fn generate_hostname(mac: [u8; 6], friendly_name: Option<&str>) -> String {
        if let Some(name) = friendly_name {
            let sanitized = Self::sanitize_hostname(name);
            if !sanitized.is_empty() && Self::is_valid_hostname(&sanitized) {
                return sanitized;
            }
        }

        // Fallback to MAC-based hostname
        Self::hostname_from_mac(mac, "device")
    }

    /// Check if IP address is in private range
    pub fn is_private_ip(ip: Ipv4Addr) -> bool {
        let octets = ip.octets();
        matches!(
            octets,
            [10, _, _, _] | [172, 16..=31, _, _] | [192, 168, _, _] | [169, 254, _, _] // Link-local
        )
    }

    /// Format MAC address for display
    pub fn format_mac(mac: [u8; 6]) -> String {
        format!(
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
        )
    }

    /// Parse MAC address from string
    pub fn parse_mac(mac_str: &str) -> Result<[u8; 6]> {
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

    /// Create a test DNS entry for validation
    pub fn create_test_entry(hostname: &str, ip: Ipv4Addr) -> Result<(String, Ipv4Addr)> {
        let clean_hostname = hostname.trim_end_matches(".local");
        if !Self::is_valid_hostname(clean_hostname) {
            return Err(anyhow::anyhow!("Invalid hostname: {}", hostname));
        }

        Ok((clean_hostname.to_string(), ip))
    }

    /// Generate device info string
    pub fn device_info(hostname: &str, ip: Ipv4Addr, mac: [u8; 6]) -> String {
        format!(
            "Device: {}.local -> {} (MAC: {})",
            hostname,
            ip,
            Self::format_mac(mac)
        )
    }

    /// Validate DNS configuration
    pub fn validate_config(config: &DnsConfig) -> Result<()> {
        if config.domain_suffix.is_empty() {
            return Err(anyhow::anyhow!("Domain suffix cannot be empty"));
        }

        if !config.domain_suffix.starts_with('.') {
            return Err(anyhow::anyhow!("Domain suffix must start with '.'"));
        }

        if config.cache_ttl == 0 {
            return Err(anyhow::anyhow!("Cache TTL must be greater than 0"));
        }

        if config.max_cache_entries == 0 {
            return Err(anyhow::anyhow!("Max cache entries must be greater than 0"));
        }

        Ok(())
    }
}

/// DNS test utilities
pub struct DnsTest {
    test_entries: Arc<Mutex<HashMap<String, Ipv4Addr>>>,
}

impl DnsTest {
    pub fn new() -> Self {
        Self {
            test_entries: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Add a test DNS entry
    pub fn add_test_entry(&self, hostname: &str, ip: Ipv4Addr) -> Result<()> {
        let (clean_hostname, _) = DnsUtils::create_test_entry(hostname, ip)?;

        let mut entries = self.test_entries.lock().unwrap();
        entries.insert(clean_hostname.clone(), ip);

        info!("DNS Test: Added {}.local -> {}", clean_hostname, ip);
        Ok(())
    }

    /// Remove a test DNS entry
    pub fn remove_test_entry(&self, hostname: &str) {
        let clean_hostname = hostname.trim_end_matches(".local");
        let mut entries = self.test_entries.lock().unwrap();
        if entries.remove(clean_hostname).is_some() {
            info!("DNS Test: Removed {}.local", clean_hostname);
        }
    }

    /// Query a test entry
    pub fn query_test_entry(&self, hostname: &str) -> Option<Ipv4Addr> {
        let clean_hostname = hostname.trim_end_matches(".local");
        let entries = self.test_entries.lock().unwrap();
        entries.get(clean_hostname).copied()
    }

    /// List all test entries
    pub fn list_test_entries(&self) -> Vec<(String, Ipv4Addr)> {
        let entries = self.test_entries.lock().unwrap();
        entries
            .iter()
            .map(|(hostname, ip)| (format!("{}.local", hostname), *ip))
            .collect()
    }

    /// Run basic DNS functionality tests
    pub fn run_basic_tests(&self) -> Result<()> {
        info!("Running DNS basic functionality tests...");

        // Test 1: Hostname validation
        let test_hostnames = vec![
            ("valid-hostname", true),
            ("test123", true),
            ("my-device-01", true),
            ("", false),
            ("-invalid", false),
            ("invalid-", false),
            (
                "too.many.dots.in.hostname.for.test.purposes.exceeding.limits",
                false,
            ),
        ];

        for (hostname, should_be_valid) in test_hostnames {
            let is_valid = DnsUtils::is_valid_hostname(hostname);
            if is_valid != should_be_valid {
                return Err(anyhow::anyhow!(
                    "Hostname validation test failed for '{}': expected {}, got {}",
                    hostname,
                    should_be_valid,
                    is_valid
                ));
            }
        }
        info!("✓ Hostname validation tests passed");

        // Test 2: Hostname sanitization
        let test_sanitization = vec![
            ("My Device!", "my-device"),
            ("Test_123", "test-123"),
            ("  spaced  out  ", "spaced-out"),
            ("dots.and.spaces test", "dots.and.spaces-test"),
        ];

        for (input, expected) in test_sanitization {
            let sanitized = DnsUtils::sanitize_hostname(input);
            if sanitized != expected {
                return Err(anyhow::anyhow!(
                    "Sanitization test failed for '{}': expected '{}', got '{}'",
                    input,
                    expected,
                    sanitized
                ));
            }
        }
        info!("✓ Hostname sanitization tests passed");

        // Test 3: MAC address formatting and parsing
        let test_mac = [0x00, 0x1A, 0x2B, 0x3C, 0x4D, 0x5E];
        let formatted = DnsUtils::format_mac(test_mac);
        let expected_format = "00:1a:2b:3c:4d:5e";

        if formatted != expected_format {
            return Err(anyhow::anyhow!(
                "MAC formatting test failed: expected '{}', got '{}'",
                expected_format,
                formatted
            ));
        }

        let parsed = DnsUtils::parse_mac(&formatted)?;
        if parsed != test_mac {
            return Err(anyhow::anyhow!(
                "MAC parsing test failed: round-trip conversion failed"
            ));
        }
        info!("✓ MAC address formatting/parsing tests passed");

        // Test 4: IP address validation
        let private_ips = vec!["10.0.0.1", "172.16.0.1", "192.168.1.1", "169.254.1.1"];

        for ip_str in private_ips {
            let ip: Ipv4Addr = ip_str.parse()?;
            if !DnsUtils::is_private_ip(ip) {
                return Err(anyhow::anyhow!(
                    "Private IP test failed: {} should be considered private",
                    ip
                ));
            }
        }

        let public_ip: Ipv4Addr = "8.8.8.8".parse()?;
        if DnsUtils::is_private_ip(public_ip) {
            return Err(anyhow::anyhow!(
                "Public IP test failed: {} should not be considered private",
                public_ip
            ));
        }
        info!("✓ IP address validation tests passed");

        info!("All DNS basic functionality tests passed! ✓");
        Ok(())
    }

    /// Run performance tests (simulate load)
    pub fn run_performance_tests(&self, num_entries: usize) -> Result<()> {
        info!(
            "Running DNS performance tests with {} entries...",
            num_entries
        );

        // Clear existing entries
        {
            let mut entries = self.test_entries.lock().unwrap();
            entries.clear();
        }

        // Add test entries
        let start = std::time::Instant::now();
        for i in 0..num_entries {
            let hostname = format!("test-device-{:04}", i);
            let ip = Ipv4Addr::new(192, 168, 4, (i % 250 + 2) as u8);
            self.add_test_entry(&hostname, ip)?;
        }
        let add_duration = start.elapsed();

        // Query all entries
        let start = std::time::Instant::now();
        for i in 0..num_entries {
            let hostname = format!("test-device-{:04}", i);
            if self.query_test_entry(&hostname).is_none() {
                return Err(anyhow::anyhow!("Failed to query entry: {}", hostname));
            }
        }
        let query_duration = start.elapsed();

        info!("Performance test results:");
        info!(
            "  Add {} entries: {:?} ({:.2} entries/sec)",
            num_entries,
            add_duration,
            num_entries as f64 / add_duration.as_secs_f64()
        );
        info!(
            "  Query {} entries: {:?} ({:.2} queries/sec)",
            num_entries,
            query_duration,
            num_entries as f64 / query_duration.as_secs_f64()
        );

        // Clean up
        {
            let mut entries = self.test_entries.lock().unwrap();
            entries.clear();
        }

        info!("Performance tests completed! ✓");
        Ok(())
    }

    /// Print DNS test status
    pub fn print_status(&self) {
        let entries = self.list_test_entries();
        info!("DNS Test Status: {} test entries", entries.len());
        if !entries.is_empty() {
            info!("Test entries:");
            for (hostname, ip) in entries {
                info!("  {} -> {}", hostname, ip);
            }
        }
    }
}

impl Default for DnsTest {
    fn default() -> Self {
        Self::new()
    }
}
