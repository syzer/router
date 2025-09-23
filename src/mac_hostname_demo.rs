use crate::mac_hostname_config::{MacHostnameConfig, StaticMappingsBuilder};
use anyhow::Result;
use log::{info, warn};

/// Demo and testing utilities for MAC hostname functionality
pub struct MacHostnameDemo {
    config: MacHostnameConfig,
}

impl MacHostnameDemo {
    /// Create new demo instance
    pub fn new() -> Self {
        Self {
            config: MacHostnameConfig::new(),
        }
    }

    /// Create demo with sample device mappings
    pub fn with_sample_devices() -> Result<Self> {
        let config = StaticMappingsBuilder::new()
            .add("aa:bb:cc:dd:ee:ff", "johns-macbook")?
            .add("11:22:33:44:55:66", "raspberry-pi-4b")?
            .add("77:88:99:aa:bb:cc", "arduino-weather")?
            .add("dd:ee:ff:11:22:33", "security-cam-front")?
            .add("44:55:66:77:88:99", "smart-tv-living")?
            .add("00:11:22:33:44:55", "thermostat-main")?
            .add("66:77:88:99:aa:bb", "doorbell-camera")?
            .add("cc:dd:ee:ff:00:11", "garage-door-sensor")?
            .add("22:33:44:55:66:77", "kitchen-tablet")?
            .add("88:99:aa:bb:cc:dd", "office-printer")?
            .build();

        Ok(Self { config })
    }

    /// Run comprehensive demo
    pub fn run_demo(&self) -> Result<()> {
        info!("🎯 Starting MAC Hostname Demo");
        info!("");

        self.demo_static_mappings()?;
        self.demo_hostname_resolution()?;
        self.demo_conflict_resolution()?;
        self.demo_validation()?;
        self.demo_configuration_loading()?;
        self.demo_real_world_scenarios()?;

        info!("✅ MAC Hostname Demo completed successfully!");
        Ok(())
    }

    /// Demo static MAC mappings
    fn demo_static_mappings(&self) -> Result<()> {
        info!("📋 Demo: Static MAC Mappings");
        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

        self.config.print_mappings();

        info!("Total static mappings: {}", self.config.mapping_count());
        info!("");
        Ok(())
    }

    /// Demo hostname resolution
    fn demo_hostname_resolution(&self) -> Result<()> {
        info!("🔍 Demo: Hostname Resolution");
        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

        let test_cases = vec![
            ([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff], "johns-macbook"),
            ([0x11, 0x22, 0x33, 0x44, 0x55, 0x66], "raspberry-pi-4b"),
            ([0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc], "arduino-weather"),
            ([0xff, 0xff, 0xff, 0xff, 0xff, 0xff], "unknown-device"), // Should fail
        ];

        for (mac, expected_hostname) in test_cases {
            match self.config.get_hostname(mac) {
                Some(hostname) if hostname == expected_hostname => {
                    info!(
                        "✅ MAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} → {}.local",
                        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5], hostname
                    );
                }
                Some(hostname) => {
                    warn!("⚠️  MAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} → {}.local (expected {})",
                          mac[0], mac[1], mac[2], mac[3], mac[4], mac[5], hostname, expected_hostname);
                }
                None => {
                    if expected_hostname == "unknown-device" {
                        info!("✅ MAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} → No static mapping (as expected)",
                              mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);
                    } else {
                        warn!("❌ MAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} → No mapping found (expected {})",
                              mac[0], mac[1], mac[2], mac[3], mac[4], mac[5], expected_hostname);
                    }
                }
            }
        }

        info!("");
        Ok(())
    }

    /// Demo conflict resolution
    fn demo_conflict_resolution(&self) -> Result<()> {
        info!("⚡ Demo: Conflict Resolution");
        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

        let test_config = MacHostnameConfig::new();

        // Add first mapping
        let mac1 = [0x11, 0x11, 0x11, 0x11, 0x11, 0x11];
        match test_config.add_mapping(mac1, "test-device".to_string()) {
            Ok(()) => info!(
                "✅ Added mapping: test-device → MAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                mac1[0], mac1[1], mac1[2], mac1[3], mac1[4], mac1[5]
            ),
            Err(e) => warn!("❌ Failed to add first mapping: {}", e),
        }

        // Try to add conflicting mapping
        let mac2 = [0x22, 0x22, 0x22, 0x22, 0x22, 0x22];
        match test_config.add_mapping(mac2, "test-device".to_string()) {
            Ok(()) => warn!("❌ Should have failed to add duplicate hostname"),
            Err(e) => info!("✅ Correctly rejected duplicate hostname: {}", e),
        }

        // Add different hostname for second MAC
        match test_config.add_mapping(mac2, "test-device-2".to_string()) {
            Ok(()) => info!("✅ Added second mapping: test-device-2 → MAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                           mac2[0], mac2[1], mac2[2], mac2[3], mac2[4], mac2[5]),
            Err(e) => warn!("❌ Failed to add second mapping: {}", e),
        }

        info!("");
        Ok(())
    }

    /// Demo hostname validation
    fn demo_validation(&self) -> Result<()> {
        info!("✅ Demo: Hostname Validation");
        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

        let test_config = MacHostnameConfig::new();
        let test_mac = [0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa];

        let long_hostname = "a".repeat(64);
        let test_hostnames = vec![
            ("valid-hostname", true, "Standard valid hostname"),
            ("test123", true, "Alphanumeric hostname"),
            ("device-01", true, "Hostname with numbers"),
            ("My Device!", false, "Contains spaces and special chars"),
            ("-invalid", false, "Starts with hyphen"),
            ("invalid-", false, "Ends with hyphen"),
            ("", false, "Empty hostname"),
            (long_hostname.as_str(), false, "Too long (>63 chars)"),
            ("Test_Device", true, "Will be sanitized to test-device"),
        ];

        for (hostname, should_succeed, description) in test_hostnames {
            match test_config.add_mapping(test_mac, hostname.to_string()) {
                Ok(()) => {
                    if should_succeed {
                        let actual = test_config.get_hostname(test_mac).unwrap_or_default();
                        info!("✅ '{}' → '{}' ({})", hostname, actual, description);
                    } else {
                        warn!(
                            "⚠️  '{}' succeeded but should have failed ({})",
                            hostname, description
                        );
                    }
                    // Clean up for next test
                    test_config.remove_mapping(test_mac);
                }
                Err(e) => {
                    if should_succeed {
                        warn!(
                            "❌ '{}' failed but should have succeeded: {} ({})",
                            hostname, e, description
                        );
                    } else {
                        info!(
                            "✅ '{}' correctly rejected: {} ({})",
                            hostname, e, description
                        );
                    }
                }
            }
        }

        info!("");
        Ok(())
    }

    /// Demo configuration loading
    fn demo_configuration_loading(&self) -> Result<()> {
        info!("📤 Demo: Configuration Loading/Exporting");
        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

        let test_config = MacHostnameConfig::new();

        // Test configuration string loading
        let config_str =
            "aa:bb:cc:dd:ee:ff:laptop,11:22:33:44:55:66:raspberry,77:88:99:aa:bb:cc:arduino";

        info!("Loading configuration string:");
        info!("  {}", config_str);

        match test_config.load_from_config(config_str) {
            Ok(count) => {
                info!("✅ Loaded {} mappings successfully", count);

                // Verify loaded mappings
                let mac_laptop = [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff];
                let mac_raspberry = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
                let mac_arduino = [0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc];

                info!("Verifying loaded mappings:");
                if let Some(hostname) = test_config.get_hostname(mac_laptop) {
                    info!("  ✅ laptop mapping: {}", hostname);
                }
                if let Some(hostname) = test_config.get_hostname(mac_raspberry) {
                    info!("  ✅ raspberry mapping: {}", hostname);
                }
                if let Some(hostname) = test_config.get_hostname(mac_arduino) {
                    info!("  ✅ arduino mapping: {}", hostname);
                }

                // Test export
                let exported = test_config.export_to_config();
                info!("Exported configuration:");
                info!("  {}", exported);
            }
            Err(e) => warn!("❌ Failed to load configuration: {}", e),
        }

        info!("");
        Ok(())
    }

    /// Demo real-world scenarios
    fn demo_real_world_scenarios(&self) -> Result<()> {
        info!("🌍 Demo: Real-World Scenarios");
        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

        info!("Scenario 1: Home Office Setup");
        let home_office = self.create_home_office_config()?;
        info!(
            "  Configured {} devices for home office",
            home_office.mapping_count()
        );
        home_office.print_mappings();

        info!("Scenario 2: IoT Smart Home");
        let smart_home = self.create_smart_home_config()?;
        info!(
            "  Configured {} devices for smart home",
            smart_home.mapping_count()
        );
        smart_home.print_mappings();

        info!("Scenario 3: Small Business Network");
        let business = self.create_business_config()?;
        info!(
            "  Configured {} devices for business",
            business.mapping_count()
        );
        business.print_mappings();

        info!("");
        Ok(())
    }

    /// Create home office configuration
    fn create_home_office_config(&self) -> Result<MacHostnameConfig> {
        StaticMappingsBuilder::new()
            .add("aa:bb:cc:11:22:33", "work-laptop")?
            .add("dd:ee:ff:44:55:66", "personal-desktop")?
            .add("11:22:33:77:88:99", "printer-home-office")?
            .add("44:55:66:aa:bb:cc", "webcam-desk")?
            .add("77:88:99:dd:ee:ff", "tablet-notes")?
            .build();

        Ok(StaticMappingsBuilder::new()
            .add("aa:bb:cc:11:22:33", "work-laptop")?
            .add("dd:ee:ff:44:55:66", "personal-desktop")?
            .add("11:22:33:77:88:99", "printer-home-office")?
            .add("44:55:66:aa:bb:cc", "webcam-desk")?
            .add("77:88:99:dd:ee:ff", "tablet-notes")?
            .build())
    }

    /// Create smart home configuration
    fn create_smart_home_config(&self) -> Result<MacHostnameConfig> {
        Ok(StaticMappingsBuilder::new()
            .add("10:20:30:40:50:60", "thermostat-living")?
            .add("11:21:31:41:51:61", "doorbell-front")?
            .add("12:22:32:42:52:62", "camera-backyard")?
            .add("13:23:33:43:53:63", "smart-speaker-kitchen")?
            .add("14:24:34:44:54:64", "light-switch-bedroom")?
            .add("15:25:35:45:55:65", "motion-sensor-hallway")?
            .add("16:26:36:46:56:66", "smart-lock-front-door")?
            .add("17:27:37:47:57:67", "garage-door-opener")?
            .build())
    }

    /// Create business configuration
    fn create_business_config(&self) -> Result<MacHostnameConfig> {
        Ok(StaticMappingsBuilder::new()
            .add("b0:b1:b2:b3:b4:b5", "server-main")?
            .add("b1:b2:b3:b4:b5:b6", "workstation-admin")?
            .add("b2:b3:b4:b5:b6:b7", "laptop-sales-01")?
            .add("b3:b4:b5:b6:b7:b8", "laptop-sales-02")?
            .add("b4:b5:b6:b7:b8:b9", "printer-office-main")?
            .add("b5:b6:b7:b8:b9:ba", "scanner-reception")?
            .add("b6:b7:b8:b9:ba:bb", "voip-phone-reception")?
            .add("b7:b8:b9:ba:bb:bc", "access-point-floor1")?
            .add("b8:b9:ba:bb:bc:bd", "security-camera-entrance")?
            .build())
    }

    /// Performance test
    pub fn run_performance_test(&self, num_mappings: usize) -> Result<()> {
        info!("⚡ Performance Test: {} MAC mappings", num_mappings);
        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

        let test_config = MacHostnameConfig::new();

        // Add mappings performance test
        let start = std::time::Instant::now();
        for i in 0..num_mappings {
            let mac = [0xaa, 0xbb, 0xcc, (i >> 16) as u8, (i >> 8) as u8, i as u8];
            let hostname = format!("test-device-{:04}", i);

            if let Err(e) = test_config.add_mapping(mac, hostname) {
                warn!("Failed to add mapping {}: {}", i, e);
            }
        }
        let add_duration = start.elapsed();

        // Lookup performance test
        let start = std::time::Instant::now();
        let mut found_count = 0;
        for i in 0..num_mappings {
            let mac = [0xaa, 0xbb, 0xcc, (i >> 16) as u8, (i >> 8) as u8, i as u8];
            if test_config.get_hostname(mac).is_some() {
                found_count += 1;
            }
        }
        let lookup_duration = start.elapsed();

        info!("Performance Results:");
        info!(
            "  Add {} mappings: {:?} ({:.0} mappings/sec)",
            num_mappings,
            add_duration,
            num_mappings as f64 / add_duration.as_secs_f64()
        );
        info!(
            "  Lookup {} mappings: {:?} ({:.0} lookups/sec)",
            num_mappings,
            lookup_duration,
            num_mappings as f64 / lookup_duration.as_secs_f64()
        );
        info!("  Found {}/{} mappings", found_count, num_mappings);
        info!("  Memory usage: ~{}KB", (num_mappings * (6 + 20)) / 1024); // Rough estimate

        info!("");
        Ok(())
    }

    /// Interactive demo menu
    pub fn interactive_demo(&self) -> Result<()> {
        info!("🎮 Interactive MAC Hostname Demo");
        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        info!("Available demos:");
        info!("  1. Static mappings overview");
        info!("  2. Hostname resolution test");
        info!("  3. Conflict resolution demo");
        info!("  4. Validation examples");
        info!("  5. Configuration loading demo");
        info!("  6. Real-world scenarios");
        info!("  7. Performance test (100 mappings)");
        info!("  8. Performance test (1000 mappings)");
        info!("  9. Run all demos");
        info!("");

        // For embedded systems, we'll just run all demos
        // In a full implementation, you could add input handling
        info!("Running all demos automatically...");
        self.run_demo()
    }
}

impl Default for MacHostnameDemo {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a comprehensive test suite
pub fn run_comprehensive_tests() -> Result<()> {
    info!("🧪 Running Comprehensive MAC Hostname Tests");
    info!("═══════════════════════════════════════════════");

    // Basic functionality tests
    let demo = MacHostnameDemo::with_sample_devices()?;
    demo.run_demo()?;

    // Performance tests
    demo.run_performance_test(100)?;
    demo.run_performance_test(1000)?;

    // Edge case tests
    run_edge_case_tests()?;

    info!("🎉 All MAC hostname tests completed successfully!");
    Ok(())
}

/// Run edge case tests
fn run_edge_case_tests() -> Result<()> {
    info!("🔬 Edge Case Tests");
    info!("━━━━━━━━━━━━━━━━━━━");

    let config = MacHostnameConfig::new();

    // Test very long hostname
    let long_hostname = "a".repeat(100);
    let result = config.add_mapping([0x01, 0x02, 0x03, 0x04, 0x05, 0x06], long_hostname);
    match result {
        Ok(()) => warn!("❌ Should have rejected very long hostname"),
        Err(_) => info!("✅ Correctly rejected very long hostname"),
    }

    // Test hostname with unicode characters
    let unicode_hostname = "café-réseau";
    let result = config.add_mapping(
        [0x02, 0x03, 0x04, 0x05, 0x06, 0x07],
        unicode_hostname.to_string(),
    );
    match result {
        Ok(()) => {
            if let Some(sanitized) = config.get_hostname([0x02, 0x03, 0x04, 0x05, 0x06, 0x07]) {
                info!(
                    "✅ Unicode hostname sanitized: '{}' → '{}'",
                    unicode_hostname, sanitized
                );
            }
        }
        Err(e) => info!("✅ Unicode hostname handled: {}", e),
    }

    // Test maximum valid hostname (63 chars)
    let max_hostname = "a".repeat(63);
    let result = config.add_mapping([0x03, 0x04, 0x05, 0x06, 0x07, 0x08], max_hostname.clone());
    match result {
        Ok(()) => info!(
            "✅ Maximum length hostname ({} chars) accepted",
            max_hostname.len()
        ),
        Err(e) => warn!("❌ Maximum length hostname should be valid: {}", e),
    }

    info!("");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_demo_creation() {
        let demo = MacHostnameDemo::new();
        assert_eq!(demo.config.mapping_count(), 0);
    }

    #[test]
    fn test_sample_devices_demo() {
        let demo = MacHostnameDemo::with_sample_devices().unwrap();
        assert!(demo.config.mapping_count() > 0);
    }

    #[test]
    fn test_home_office_config() {
        let demo = MacHostnameDemo::new();
        let config = demo.create_home_office_config().unwrap();
        assert_eq!(config.mapping_count(), 5);
    }

    #[test]
    fn test_performance_test() {
        let demo = MacHostnameDemo::new();
        assert!(demo.run_performance_test(10).is_ok());
    }
}
