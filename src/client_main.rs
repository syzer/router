use esp_idf_svc::log::EspLogger;
use esp_wifi_ap::client;
use log::*;

fn main() -> anyhow::Result<()> {
    // Initialize logger
    EspLogger::initialize_default();

    info!("Starting ESP32 Wi-Fi Station Client with Network Cycling");

    // Show available networks
    client::show_available_networks();

    // Test RSSI calculations
    client::test_rssi_calculations();

    // Run the main client loop with network cycling
    client::run_wifi_client()?;

    Ok(())
}
