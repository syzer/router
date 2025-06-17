use ::log::info;
use esp_idf_svc::hal::modem::Modem;
use esp_idf_svc::wifi::*;
use esp_idf_svc::nvs::*;
use heapless::String as HeapString;
use esp_idf_svc::hal::{
    peripherals::Peripherals,
};
use esp_idf_svc::hal::delay::FreeRtos;
use esp_wifi_ap::{WS2812RMT, RGB8};
use std::sync::{Arc, Mutex};
use esp_idf_sys as sys;
use std::thread;
use std::time::Duration;

const AP_SSID: &str = env!("AP_SSID");  // Connect to your AP
const AP_PASS: &str = env!("AP_PASS");

// RTT measurement constants
const RTT_SCAN_INTERVAL_MS: u128 = 5000; // Scan every 5 seconds

fn start_rtt_scan(target_mac: [u8; 6]) -> anyhow::Result<()> {
    info!("🎯 Starting RTT scan to target MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}", 
        target_mac[0], target_mac[1], target_mac[2], 
        target_mac[3], target_mac[4], target_mac[5]);

    unsafe {
        // Configure FTM initiator
        let mut ftm_cfg = sys::wifi_ftm_initiator_cfg_t {
            frm_count: 16,           // Number of FTM frames
            burst_period: 2,         // Burst period
            resp_mac: target_mac,    // Target AP MAC address
            channel: 1,              // Channel (you might need to detect this)
            use_get_report_api: true, // Use report API
        };

        let err = sys::esp_wifi_ftm_initiate_session(&mut ftm_cfg as *mut _);
        if err != sys::ESP_OK {
            anyhow::bail!("Failed to initiate FTM session: {}", err);
        }
        
        info!("✅ RTT session initiated successfully");
        
        // Wait a bit and try to get the report
        thread::sleep(Duration::from_millis(1000));
        
        // Try to get FTM report using the correct API
        let mut report = [sys::wifi_ftm_report_entry_t::default(); 1];
        let num_entries = 1u8;
        
        let err = sys::esp_wifi_ftm_get_report(report.as_mut_ptr(), num_entries);
        if err == sys::ESP_OK {
            let entry = &report[0];
            info!("📡 RTT Report:");
            info!("  RTT: {} ns", entry.rtt);
            info!("  RSSI: {} dBm", entry.rssi);
            info!("  Dialog token: {}", entry.dlog_token);
            
            // Calculate distance from RTT (speed of light = 3e8 m/s)
            // Distance = (RTT * speed_of_light) / 2
            // RTT is in nanoseconds, so: distance_cm = (rtt_ns * 30) / 2 = rtt_ns * 15
            let distance_cm = (entry.rtt as f32 * 0.15) / 10.0; // More conservative calculation
            let distance_m = distance_cm / 100.0;
            info!("  📏 Calculated Distance: {:.2} cm ({:.3} meters)", distance_cm, distance_m);
        } else {
            info!("⚠️  No RTT report available yet (error: {})", err);
        }
    }
    
    Ok(())
}

fn get_ap_mac_from_scan() -> anyhow::Result<Option<[u8; 6]>> {
    info!("🔍 Scanning for AP to get MAC address...");
    
    unsafe {
        // Start WiFi scan
        let scan_config = sys::wifi_scan_config_t {
            ssid: std::ptr::null_mut(),
            bssid: std::ptr::null_mut(),
            channel: 0,
            show_hidden: false,
            scan_type: sys::wifi_scan_type_t_WIFI_SCAN_TYPE_ACTIVE,
            scan_time: sys::wifi_scan_time_t {
                active: sys::wifi_active_scan_time_t {
                    min: 100,
                    max: 300,
                },
                passive: 0,
            },
            home_chan_dwell_time: 30,
        };

        let err = sys::esp_wifi_scan_start(&scan_config as *const _, false);
        if err != sys::ESP_OK {
            anyhow::bail!("Failed to start WiFi scan: {}", err);
        }

        // Wait for scan to complete
        thread::sleep(Duration::from_millis(2000));

        let mut ap_count: u16 = 0;
        let err = sys::esp_wifi_scan_get_ap_num(&mut ap_count as *mut _);
        if err != sys::ESP_OK {
            anyhow::bail!("Failed to get AP count: {}", err);
        }

        info!("📋 Found {} APs", ap_count);

        if ap_count > 0 {
            let mut ap_records = vec![sys::wifi_ap_record_t::default(); ap_count as usize];
            let mut actual_count = ap_count;
            
            let err = sys::esp_wifi_scan_get_ap_records(&mut actual_count as *mut _, ap_records.as_mut_ptr());
            if err != sys::ESP_OK {
                anyhow::bail!("Failed to get AP records: {}", err);
            }

            // Look for our target AP
            for ap in ap_records.iter().take(actual_count as usize) {
                let ssid_bytes = &ap.ssid[..ap.ssid.iter().position(|&x| x == 0).unwrap_or(ap.ssid.len())];
                if let Ok(ssid) = std::str::from_utf8(ssid_bytes) {
                    info!("🏠 Found AP: {} (MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x})", 
                        ssid,
                        ap.bssid[0], ap.bssid[1], ap.bssid[2],
                        ap.bssid[3], ap.bssid[4], ap.bssid[5]);
                    
                    if ssid == AP_SSID {
                        info!("🎯 Found target AP! MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}", 
                            ap.bssid[0], ap.bssid[1], ap.bssid[2],
                            ap.bssid[3], ap.bssid[4], ap.bssid[5]);
                        return Ok(Some(ap.bssid));
                    }
                }
            }
        }
    }
    
    Ok(None)
}

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    
    // LED for status indication
    let led = Arc::new(Mutex::new(
        WS2812RMT::new(
            peripherals.pins.gpio8,
            peripherals.rmt.channel0,
        )?
    ));

    info!("🔗 Starting Wi-Fi RTT Client - connecting to AP '{}'", AP_SSID);

    let modem = unsafe { Modem::new() };
    let sysloop = esp_idf_svc::eventloop::EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;
    let mut wifi = EspWifi::new(modem, sysloop.clone(), Some(nvs))?;

    // Configure as client only (STA mode)
    let mut ssid: HeapString<32> = HeapString::new();
    ssid.push_str(AP_SSID).expect("SSID too long");

    let mut password: HeapString<64> = HeapString::new();
    password.push_str(AP_PASS).expect("Password too long");

    let sta_cfg = ClientConfiguration {
        ssid,
        password,
        ..Default::default()
    };

    wifi.set_configuration(&Configuration::Client(sta_cfg))?;
    wifi.start()?;

    // Blink blue while connecting
    {
        let mut led_guard = led.lock().unwrap();
        led_guard.set_pixel(RGB8::new(0, 0, 25))?; // Blue
    }

    info!("🔄 Connecting to AP...");
    wifi.connect()?;

    // Wait for connection
    loop {
        if wifi.is_connected()? {
            info!("✅ Connected to AP!");
            
            // Get IP address
            let ip_info = wifi.sta_netif().get_ip_info()?;
            info!("📱 Client IP: {}", ip_info.ip);
            info!("🌐 Gateway: {}", ip_info.subnet.gateway);
            info!("🔧 Netmask: {}", ip_info.subnet.mask);

            // Blink green when connected
            {
                let mut led_guard = led.lock().unwrap();
                led_guard.set_pixel(RGB8::new(0, 25, 0))?; // Green
            }
            break;
        }
        
        FreeRtos::delay_ms(1000);
        info!("⏳ Still connecting...");
    }

    // Get AP MAC address for RTT measurements
    let mut ap_mac: Option<[u8; 6]> = None;
    
    // Try to get AP MAC from scan
    match get_ap_mac_from_scan() {
        Ok(Some(mac)) => {
            ap_mac = Some(mac);
            info!("📍 Target AP MAC obtained from scan");
        }
        Ok(None) => {
            info!("⚠️  Could not find target AP in scan results");
        }
        Err(e) => {
            info!("❌ Scan failed: {:?}", e);
        }
    }

    // Main loop with RTT scanning
    let mut counter = 0;
    let mut last_rtt_scan = std::time::Instant::now();
    
    loop {
        if wifi.is_connected()? {
            // Slow green blink when connected
            let mut led_guard = led.lock().unwrap();
            if counter % 4 == 0 {
                led_guard.set_pixel(RGB8::new(0, 10, 0))?; // Dim green
            } else {
                led_guard.set_pixel(RGB8::new(0, 0, 0))?;  // Off
            }
            
            // Perform RTT scan periodically
            if let Some(target_mac) = ap_mac {
                if last_rtt_scan.elapsed().as_millis() >= RTT_SCAN_INTERVAL_MS {
                    info!("🎯 Initiating RTT measurement...");
                    
                    match start_rtt_scan(target_mac) {
                        Ok(()) => {
                            info!("✅ RTT scan initiated");
                            // Blink cyan during RTT scan
                            led_guard.set_pixel(RGB8::new(0, 25, 25))?; // Cyan
                        }
                        Err(e) => {
                            info!("❌ RTT scan failed: {:?}", e);
                            // Blink yellow on RTT error
                            led_guard.set_pixel(RGB8::new(25, 25, 0))?; // Yellow
                        }
                    }
                    
                    last_rtt_scan = std::time::Instant::now();
                }
            } else {
                // Try to get AP MAC again if we don't have it
                if counter % 20 == 0 { // Every ~10 seconds
                    info!("🔍 Retrying AP MAC scan...");
                    if let Ok(Some(mac)) = get_ap_mac_from_scan() {
                        ap_mac = Some(mac);
                        info!("📍 Target AP MAC obtained!");
                    }
                }
            }
            
            if counter % 10 == 0 {
                info!("📡 Client alive - connected to AP (RTT enabled)");
            }
        } else {
            // Fast red blink when disconnected
            let mut led_guard = led.lock().unwrap();
            if counter % 2 == 0 {
                led_guard.set_pixel(RGB8::new(25, 0, 0))?; // Red
            } else {
                led_guard.set_pixel(RGB8::new(0, 0, 0))?;  // Off
            }
            
            info!("❌ Client disconnected - trying to reconnect");
            let _ = wifi.connect();
        }
        
        counter += 1;
        FreeRtos::delay_ms(500);
    }
}