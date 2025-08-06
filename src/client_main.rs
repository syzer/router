use esp_idf_svc::log::EspLogger;
use esp_wifi_ap::client;
use log::*;

fn main() -> anyhow::Result<()> {
    // Initialize logger
    EspLogger::initialize_default();

    info!("Starting ESP32 Wi-Fi Station Client");

    // Test RSSI calculations first
    client::test_rssi_calculations();

    // Run the main client loop
    client::run_wifi_client()?;

    Ok(())
}
