use embedded_svc::{
    wifi::{AuthMethod, ClientConfiguration, Configuration},
};
use esp_idf_hal::{delay::FreeRtos, prelude::*, gpio::{PinDriver, Input, Pull}};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    nvs::EspDefaultNvsPartition,
    wifi::{BlockingWifi, EspWifi},
};
use esp_idf_sys as _;
use log::*;
use std::sync::Mutex;

include!(concat!(env!("OUT_DIR"), "/device_names.rs"));
include!(concat!(env!("OUT_DIR"), "/wifi_networks.rs"));

/// RSSI to distance estimation constants
/// These are rough estimates and can vary significantly based on:
/// - Environment (obstacles, interference)
/// - Antenna characteristics
/// - Transmit power
const RSSI_REF: f32 = -30.0; // RSSI at 1 meter reference distance (dBm)
const PATH_LOSS_EXPONENT: f32 = 3.0; // Free space path loss exponent

/// Current Wi-Fi network index (shared state)
static CURRENT_NETWORK_INDEX: Mutex<usize> = Mutex::new(0);

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

/// Get current Wi-Fi network credentials
fn get_current_network() -> Option<&'static WifiCredentials> {
    let index = *CURRENT_NETWORK_INDEX.lock().unwrap();
    get_network(index)
}

/// Cycle to next Wi-Fi network
fn switch_to_next_network() -> Option<&'static WifiCredentials> {
    let mut current_index = CURRENT_NETWORK_INDEX.lock().unwrap();
    *current_index = cycle_to_next_network(*current_index);
    info!("Switched to network index: {}", *current_index);
    get_network(*current_index)
}

/// Check if button is pressed (GPIO0 typically used for boot button)
fn is_button_pressed(button: &mut PinDriver<'_, impl esp_idf_hal::gpio::InputPin, Input>) -> bool {
    button.is_low()
}

/// Main client function that connects to Wi-Fi and monitors RSSI with network cycling
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

    // Check available networks
    let network_count = get_network_count();
    if network_count == 0 {
        error!("No Wi-Fi networks configured! Please check your .env file.");
        return Err(anyhow::anyhow!("No Wi-Fi networks configured"));
    }
    
    info!("Found {} Wi-Fi networks configured", network_count);
    for i in 0..network_count {
        if let Some(network) = get_network(i) {
            info!("  Network {}: {}", i + 1, network.ssid);
        }
    }

    // Initialize button (GPIO0 - boot button on most ESP32 boards)
    let mut button = PinDriver::input(peripherals.pins.gpio0)?;
    button.set_pull(Pull::Up)?;
    
    // Initialize Wi-Fi
    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(peripherals.modem, sys_loop.clone(), Some(nvs))?,
        sys_loop,
    )?;

    info!("Starting Wi-Fi station mode...");

    // Get initial network
    let mut current_network = get_current_network()
        .ok_or_else(|| anyhow::anyhow!("Failed to get current network"))?;
    
    let mut last_button_state = false;
    let mut connected = false;

    loop {
        // Check button press for network cycling
        let button_pressed = is_button_pressed(&mut button);
        
        // Detect button press (rising edge)
        if button_pressed && !last_button_state {
            info!("Button pressed! Cycling to next network...");
            
            // Disconnect if currently connected
            if connected {
                info!("Disconnecting from current network...");
                let _ = wifi.disconnect();
                connected = false;
            }
            
            // Cycle to next network
            current_network = switch_to_next_network()
                .ok_or_else(|| anyhow::anyhow!("Failed to get next network"))?;
            
            FreeRtos::delay_ms(500); // Debounce delay
        }
        last_button_state = button_pressed;

        // Try to connect if not connected
        if !connected {
            info!("Attempting to connect to: {}", current_network.ssid);
            
            // Configure Wi-Fi for current network
            wifi.set_configuration(&Configuration::Client(ClientConfiguration {
                ssid: current_network.ssid.try_into().unwrap(),
                bssid: None,
                auth_method: AuthMethod::WPA2Personal,
                password: current_network.password.try_into().unwrap(),
                channel: None,
                ..Default::default()
            }))?;

            // Start and connect
            wifi.start()?;
            match wifi.connect() {
                Ok(_) => {
                    info!("Connected to Wi-Fi: {}", current_network.ssid);
                    match wifi.wait_netif_up() {
                        Ok(_) => {
                            info!("Network interface is up!");
                            
                            // Get IP configuration
                            let ip_info = wifi.wifi().sta_netif().get_ip_info()?;
                            info!("IP Info: IP: {}, Subnet: {}, Gateway: {}", 
                                  ip_info.ip, ip_info.subnet.mask, ip_info.subnet.gateway);
                            
                            connected = true;
                        }
                        Err(e) => {
                            warn!("Failed to get IP: {:?}", e);
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to connect to {}: {:?}", current_network.ssid, e);
                    FreeRtos::delay_ms(5000); // Wait before retry
                }
            }
        } else {
            // Monitor RSSI when connected
            match wifi.scan() {
                Ok(ap_infos) => {
                    // Find our connected AP
                    if let Some(ap_info) = ap_infos.iter().find(|ap| ap.ssid == current_network.ssid) {
                        let rssi = ap_info.signal_strength;
                        let distance = estimate_distance_from_rssi(rssi);
                        let distance_class = classify_distance(distance);
                        
                        info!("AP: {} | RSSI: {}dBm | Distance: {:.1}m | Range: {}", 
                              current_network.ssid, rssi, distance, distance_class);
                        
                        // Optional: Log additional AP details
                        debug!("AP Details - Channel: {}, Auth: {:?}", 
                               ap_info.channel, ap_info.auth_method);
                    }
                }
                Err(e) => {
                    warn!("Failed to scan for APs: {:?}", e);
                }
            }

            // Check connection status
            if !wifi.is_connected()? {
                warn!("Lost connection to AP: {}", current_network.ssid);
                connected = false;
            }
        }

        // Sleep before next iteration
        FreeRtos::delay_ms(1000); // 1 second intervals
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

/// Display available Wi-Fi networks
pub fn show_available_networks() {
    info!("=== Available Wi-Fi Networks ===");
    let network_count = get_network_count();
    if network_count == 0 {
        warn!("No Wi-Fi networks configured in .env file!");
        info!("Please add networks in format:");
        info!("ST_SSID_1=YourWifi1");
        info!("ST_PASS_1=YourPassword1");
        info!("ST_SSID_2=YourWifi2");
        info!("ST_PASS_2=YourPassword2");
    } else {
        info!("Found {} networks:", network_count);
        for i in 0..network_count {
            if let Some(network) = get_network(i) {
                info!("  {}. {} (password: {})", i + 1, network.ssid, "*".repeat(network.password.len()));
            }
        }
        info!("Press the button to cycle through networks!");
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
