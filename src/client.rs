use embedded_svc::{
    wifi::{AuthMethod, ClientConfiguration, Configuration},
};
use esp_idf_hal::{delay::FreeRtos, prelude::*};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    nvs::EspDefaultNvsPartition,
    wifi::{BlockingWifi, EspWifi},
};
use esp_idf_sys as _;
use log::*;

include!(concat!(env!("OUT_DIR"), "/device_names.rs"));

// Environment variables from build.rs
const ST_SSID: &str = env!("AP_SSID");
const ST_PASS: &str = env!("AP_PASS");

/// RSSI to distance estimation constants
/// These are rough estimates and can vary significantly based on:
/// - Environment (obstacles, interference)
/// - Antenna characteristics
/// - Transmit power
const RSSI_REF: f32 = -30.0; // RSSI at 1 meter reference distance (dBm)
const PATH_LOSS_EXPONENT: f32 = 3.0; // Free space path loss exponent

/// Estimate distance based on RSSI
/// Formula: Distance = 10^((RSSI_ref - RSSI) / (10 * n))
/// Where n is the path loss exponent (typically 2-4)
fn estimate_distance_from_rssi(rssi: i8) -> f32 {
    let rssi_f32 = rssi as f32;
    let exponent = (RSSI_REF - rssi_f32) / (10.0 * PATH_LOSS_EXPONENT);
    10.0_f32.powf(exponent)
}

/// Classify distance into ranges for easier interpretation
fn classify_distance(distance: f32) -> &'static str {
    match distance {
        d if d < 1.0 => "Very Close (<1m)",
        d if d < 5.0 => "Close (1-5m)",
        d if d < 15.0 => "Medium (5-15m)",
        d if d < 50.0 => "Far (15-50m)",
        _ => "Very Far (>50m)",
    }
}

/// Get chip MAC address for device naming
fn get_mac_address() -> [u8; 6] {
    let mut mac = [0u8; 6];
    unsafe {
        esp_idf_sys::esp_wifi_get_mac(esp_idf_sys::wifi_interface_t_WIFI_IF_STA, mac.as_mut_ptr());
    }
    mac
}

/// Main client function that connects to Wi-Fi and monitors RSSI
pub fn run_wifi_client() -> anyhow::Result<()> {
    let peripherals = Peripherals::take()?;
    let sys_loop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    // Get device MAC and friendly name
    let mac = get_mac_address();
    let device_name = mac_to_name(&mac);
    
    info!("=== ESP32 Wi-Fi Station Client ===");
    info!("Device MAC: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}", 
          mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);
    info!("Device Name: {}", device_name);

    // Initialize Wi-Fi
    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(peripherals.modem, sys_loop.clone(), Some(nvs))?,
        sys_loop,
    )?;

    info!("Starting Wi-Fi station mode...");

    // Configure as station
    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: ST_SSID.try_into().unwrap(),
        bssid: None,
        auth_method: AuthMethod::WPA2Personal,
        password: ST_PASS.try_into().unwrap(),
        channel: None,
        ..Default::default()
    }))?;

    // Start Wi-Fi
    wifi.start()?;
    info!("Wi-Fi started, connecting to SSID: {}", ST_SSID);

    // Connect to the AP
    wifi.connect()?;
    info!("Connected to Wi-Fi successfully!");

    // Wait for IP assignment
    wifi.wait_netif_up()?;
    info!("Network interface is up!");

    // Get IP configuration
    let ip_info = wifi.wifi().sta_netif().get_ip_info()?;
    info!("IP Info: IP: {}, Subnet: {}, Gateway: {}", 
          ip_info.ip, ip_info.subnet.mask, ip_info.subnet.gateway);

    // Monitor RSSI and estimate distance
    loop {
        // Perform a scan to get AP information including RSSI
        let ap_infos = match wifi.scan() {
            Ok(infos) => infos,
            Err(e) => {
                warn!("Failed to scan for APs: {:?}", e);
                continue;
            }
        };

        // Find our connected AP
        if let Some(ap_info) = ap_infos.iter().find(|ap| ap.ssid == ST_SSID) {
            let rssi = ap_info.signal_strength;
            let distance = estimate_distance_from_rssi(rssi);
            let distance_class = classify_distance(distance);
            
            info!("AP: {} | RSSI: {}dBm | Distance: {:.1}m | Range: {}", 
                  ST_SSID, rssi, distance, distance_class);
            
            // Optional: Log additional AP details
            debug!("AP Details - Channel: {}, Auth: {:?}", 
                   ap_info.channel, ap_info.auth_method);
        }

        // Check connection status
        if !wifi.is_connected()? {
            warn!("Lost connection to AP, attempting to reconnect...");
            match wifi.connect() {
                Ok(_) => {
                    info!("Reconnected successfully!");
                    wifi.wait_netif_up()?;
                }
                Err(e) => {
                    error!("Failed to reconnect: {:?}", e);
                }
            }
        }

        // Sleep before next measurement
        FreeRtos::delay_ms(5000); // 5 second intervals
    }
}

/// Alternative function for continuous RSSI monitoring without scanning
/// This uses the connected AP's RSSI directly (if available)
pub fn monitor_connected_rssi() -> anyhow::Result<()> {
    info!("Starting continuous RSSI monitoring...");
    
    // This would require direct ESP-IDF APIs to get RSSI of connected AP
    // For now, we'll use the scan-based approach above
    warn!("Direct RSSI monitoring not yet implemented, use run_wifi_client() instead");
    
    Ok(())
}

/// Test function to demonstrate RSSI to distance calculations
pub fn test_rssi_calculations() {
    info!("=== RSSI to Distance Test ===");
    
    let test_rssi_values = [-30, -40, -50, -60, -70, -80, -90];
    
    for rssi in test_rssi_values {
        let distance = estimate_distance_from_rssi(rssi);
        let classification = classify_distance(distance);
        info!("RSSI: {}dBm => Distance: {:.1}m ({})", 
              rssi, distance, classification);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_distance_estimation() {
        // Test some known RSSI values
        assert!(estimate_distance_from_rssi(-30) < estimate_distance_from_rssi(-60));
        assert!(estimate_distance_from_rssi(-40) > 0.5);
        assert!(estimate_distance_from_rssi(-80) > 10.0);
    }

    #[test]
    fn test_distance_classification() {
        assert_eq!(classify_distance(0.5), "Very Close (<1m)");
        assert_eq!(classify_distance(3.0), "Close (1-5m)");
        assert_eq!(classify_distance(10.0), "Medium (5-15m)");
        assert_eq!(classify_distance(30.0), "Far (15-50m)");
        assert_eq!(classify_distance(100.0), "Very Far (>50m)");
    }
}
