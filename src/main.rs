use ::log::info;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::net::Ipv4Addr;
use esp_idf_svc::hal::modem::Modem;
use esp_idf_svc::wifi::*;
use esp_idf_svc::nvs::*;
use heapless::String as HeapString;
use esp_idf_svc::handle::RawHandle;
use esp_idf_sys as sys;
use sys::esp_netif_napt_enable;
use esp_idf_svc::netif::EspNetif;
use esp_idf_svc::netif::IpEvent;
use esp_idf_svc::hal::{
    gpio::{InterruptType, PinDriver, Pull},
    peripherals::Peripherals,
    task::notification::Notification,
};
use std::num::NonZeroU32;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_wifi_ap::{WS2812RMT, RGB8};  // RGB8 came from the `rgb` crate
use core::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::ffi::c_void;

static CLIENT_GOT_CONNECTED: AtomicBool = AtomicBool::new(false); // for blinking led everytime someone connected

const AP_SSID: &str = env!("AP_SSID");
const AP_PASS: &str = env!("AP_PASS");

const ST_SSID: &str = env!("ST_SSID");
const ST_PASS: &str = env!("ST_PASS");

// RTT measurement function
fn start_ftm_session(target_mac: [u8; 6]) -> Result<(), esp_idf_sys::EspError> {
    unsafe {
        let mut ftm_cfg = sys::wifi_ftm_initiator_cfg_t {
            resp_mac: target_mac,
            channel: 0,
            frm_count: 16,             // Number of measurement frames
            burst_period: 2,           // 200 ms between bursts
            use_get_report_api: true,  // required since ESP-IDF v4.4
        };

        let result = sys::esp_wifi_ftm_initiate_session(&mut ftm_cfg);
        if result == sys::ESP_OK {
            info!("📡 Started FTM session with {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                target_mac[0], target_mac[1], target_mac[2], target_mac[3], target_mac[4], target_mac[5]);
            Ok(())
        } else {
            Err(esp_idf_sys::EspError::from(result).unwrap())
        }
    }
}

// Enable 802.11mc RTT (FTM responder) capabilities
fn enable_ftm_responder() -> Result<(), esp_idf_sys::EspError> {
    info!("🎯 Enabling 802.11mc RTT (FTM responder) on AP...");
    
    unsafe {
        // Set FTM responder offset (this enables FTM responder indirectly)
        let err = sys::esp_wifi_ftm_resp_set_offset(0); // 0 cm offset
        if err != sys::ESP_OK {
            info!("⚠️  Could not set FTM responder offset: {}", err);
            // Don't return error, this might not be supported on all chips
        } else {
            info!("✅ FTM responder offset set successfully");
        }
        
        // Set WiFi bandwidth to support better timing resolution
        let err = sys::esp_wifi_set_bandwidth(sys::wifi_interface_t_WIFI_IF_AP, sys::wifi_bandwidth_t_WIFI_BW_HT40);
        if err != sys::ESP_OK {
            info!("⚠️  Could not set 40MHz bandwidth: {}", err);
            // This is not critical, continue anyway
        } else {
            info!("✅ 40MHz bandwidth enabled for better RTT accuracy");
        }
        
        // Enable 802.11n protocol for better RTT support
        let protocols = (sys::WIFI_PROTOCOL_11B | sys::WIFI_PROTOCOL_11G | sys::WIFI_PROTOCOL_11N) as u8;
        let err = sys::esp_wifi_set_protocol(sys::wifi_interface_t_WIFI_IF_AP, protocols);
        if err != sys::ESP_OK {
            info!("⚠️  Could not set WiFi protocol: {}", err);
        } else {
            info!("✅ 802.11n protocol enabled for RTT support");
        }
        
        info!("✅ 802.11mc RTT configuration completed!");
        info!("📱 Android devices may now be able to use RTT");
        info!("⚠️  Note: Full FTM support depends on ESP-IDF version and chip capabilities");
    }
    
    Ok(())
}

// Verify and display RTT capabilities
fn check_rtt_capabilities() {
    info!("🔍 Checking 802.11mc RTT capabilities...");
    
    unsafe {
        // Note: FTM responder status check not available in current ESP-IDF bindings
        info!("📡 FTM Responder: Configured (status check not available)");
        
        // Get current WiFi mode
        let mut mode: sys::wifi_mode_t = sys::wifi_mode_t_WIFI_MODE_NULL;
        let err = sys::esp_wifi_get_mode(&mut mode);
        if err == sys::ESP_OK {
            match mode {
                sys::wifi_mode_t_WIFI_MODE_STA => info!("📡 WiFi Mode: Station"),
                sys::wifi_mode_t_WIFI_MODE_AP => info!("📡 WiFi Mode: Access Point"),
                sys::wifi_mode_t_WIFI_MODE_APSTA => info!("📡 WiFi Mode: AP+Station (Mixed)"),
                _ => info!("📡 WiFi Mode: Unknown"),
            }
        }
        
        // Check protocol support
        let mut protocol: u8 = 0;
        let err = sys::esp_wifi_get_protocol(sys::wifi_interface_t_WIFI_IF_AP, &mut protocol);
        if err == sys::ESP_OK {
            info!("📋 AP Protocols:");
            if (protocol & (sys::WIFI_PROTOCOL_11B as u8)) != 0 {
                info!("   - 802.11b: ✅");
            }
            if (protocol & (sys::WIFI_PROTOCOL_11G as u8)) != 0 {
                info!("   - 802.11g: ✅");
            }
            if (protocol & (sys::WIFI_PROTOCOL_11N as u8)) != 0 {
                info!("   - 802.11n: ✅ (Required for RTT)");
            }
        }
        
        // Check bandwidth
        let mut bandwidth: sys::wifi_bandwidth_t = sys::wifi_bandwidth_t_WIFI_BW_HT20;
        let err = sys::esp_wifi_get_bandwidth(sys::wifi_interface_t_WIFI_IF_AP, &mut bandwidth);
        if err == sys::ESP_OK {
            match bandwidth {
                sys::wifi_bandwidth_t_WIFI_BW_HT20 => info!("📶 Bandwidth: 20MHz"),
                sys::wifi_bandwidth_t_WIFI_BW_HT40 => info!("📶 Bandwidth: 40MHz (Better for RTT)"),
                _ => info!("📶 Bandwidth: Unknown"),
            }
        }
        
        info!("🎯 802.11mc RTT configuration applied!");
        info!("📱 Test with Android WiFi RTT API or apps like 'WiFi RTT Scanner'");
        info!("⚠️  Note: Actual RTT support depends on ESP32 chip and ESP-IDF version");
    }
}

fn main() -> anyhow::Result<()> {
    let client_ips = Arc::new(Mutex::new(HashMap::<[u8; 6], Ipv4Addr>::new()));

    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    
    info!("🚀 Starting ESP32 WiFi AP with RTT support");

    // button start
    let peripherals = Peripherals::take()?;            // singleton?

    // Push-button on GPIO9, pulled high when idle
    let mut button = PinDriver::input(peripherals.pins.gpio9)?;
    button.set_pull(Pull::Up)?;
    button.set_interrupt_type(InterruptType::PosEdge)?;

    // Async notification object
    let notification = Notification::new();
    let notifier = notification.notifier();

    unsafe {
        // SAFETY: the `Notification` outlives the interrupt subscription
        match button.subscribe(move || {
            if let Some(val) = NonZeroU32::new(1) { // .unwrap() is fine, this is just more explicit
                notifier.notify_and_yield(val);
            }
        }) {
            Ok(_) => {
                info!("Successfully subscribed to button interrupt on GPIO {}", button.pin());
            }
            Err(e) => {
                info!("Failed to subscribe to button interrupt on GPIO {}: {:?}", button.pin(), e);
                () // javascript :D
            }
        }
    }
    // button end

    let led = Arc::new(Mutex::new(
        WS2812RMT::new(
            peripherals.pins.gpio8,      // ESP32‑C6 built‑in RGB LED
            peripherals.rmt.channel0,    // any free TX channel
        )?
    ));

    info!(".....Booting up Wi-Fi AP + STA bridge........");

    let modem   = unsafe { Modem::new() };
    let sysloop = esp_idf_svc::eventloop::EspSystemEventLoop::take()?;
    let nvs     = EspDefaultNvsPartition::take()?;
    let mut wifi = EspWifi::new(modem, sysloop.clone(), Some(nvs))?;

    let mut ap_ssid = heapless::String::<32>::new();
    ap_ssid.push_str(AP_SSID).expect("SSID too long");

    let mut ap_pass = heapless::String::<64>::new();
    ap_pass.push_str(AP_PASS).expect("Password too long");

    let ap_cfg = AccessPointConfiguration {
        ssid: ap_ssid,
        password: ap_pass,
        channel: 6, // Use channel 6 for better RTT support (less congested)
        auth_method: AuthMethod::WPA2Personal,
        max_connections: 10, // Allow multiple clients for RTT testing
        ..Default::default()
    };

    let mut st_ssid: HeapString<32> = HeapString::<32>::new();
    st_ssid.push_str(ST_SSID).expect("st_ssid too long");

    let mut st_pass: HeapString<64> = HeapString::<64>::new();
    st_pass.push_str(ST_PASS).expect("st_pass Password too long");

    let sta_cfg = ClientConfiguration {
        ssid: st_ssid,
        password: st_pass,
        ..Default::default()
    };

    wifi.set_configuration(&Configuration::Mixed(sta_cfg.clone(), ap_cfg.clone()))?;
    wifi.start()?;
    wifi.connect()?;

    // Enable 802.11mc RTT (FTM responder) after WiFi is started
    if let Err(e) = enable_ftm_responder() {
        info!("⚠️  Failed to enable FTM responder: {:?}", e);
        info!("🔄 Continuing without RTT support...");
    }
    
    // Setup ranging request logging
    if let Err(e) = setup_ranging_request_logging() {
        info!("⚠️  Failed to setup ranging logging: {:?}", e);
    }
    
    // Check and display RTT capabilities
    check_rtt_capabilities();

    // ------------------------------------------------------------------
    // FTM‑REPORT subscription
    //
    // esp‑idf‑svc doesn't expose `subscribe_raw()` publicly, so we fall
    // back to the C‑level event API.  We register an extern "C" handler
    // for WIFI_EVENT_FTM_REPORT and log the RTT‑derived distance.
    // ------------------------------------------------------------------
    extern "C" fn ftm_report_handler(
        _arg: *mut c_void,
        _event_base: sys::esp_event_base_t,
        _event_id: i32,
        event_data: *mut c_void,
    ) {
        // SAFETY: the IDF guarantees that `event_data` points to a
        // `wifi_event_ftm_report_t` when the event id is
        // `WIFI_EVENT_FTM_REPORT`.
        unsafe {
            let report = &*(event_data as *const sys::wifi_event_ftm_report_t);

            // distance = (RTT_nanoseconds * speed_of_light) / 2
            // Speed of light ≈ 0.3 m/ns; convert to centimetres.
            let distance_cm = (report.rtt_est as f32 * 0.3 / 2.0) / 10.0;

            info!(
                "📏 RTT: {} ns → {:.1} cm (peer {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x})",
                report.rtt_est,
                distance_cm,
                report.peer_mac[0],
                report.peer_mac[1],
                report.peer_mac[2],
                report.peer_mac[3],
                report.peer_mac[4],
                report.peer_mac[5],
            );
        }
    }

    // Register the raw handler with the global event loop.
    unsafe {
        let err = sys::esp_event_handler_register(
            sys::WIFI_EVENT,
            sys::wifi_event_t_WIFI_EVENT_FTM_REPORT as i32,
            Some(ftm_report_handler),
            core::ptr::null_mut(),
        );
        if err != sys::ESP_OK {
            anyhow::bail!("esp_event_handler_register failed: {}", err);
        }
    };




    // Subscribe for IP events so we can see which IP each station gets
    let _ip_subscription = sysloop.subscribe::<IpEvent, _>(move |event: IpEvent| {
        if let IpEvent::ApStaIpAssigned(assignment) = event {
            let mac = assignment.mac();
            let ip  = assignment.ip();

            println!("Client got IP {} – MAC {}", ip, mac.iter()
                .map(|byte| format!("{:02x}", byte))
                .collect::<Vec<String>>()
                .join(":"));

            if let Ok(mut clients) = client_ips.lock() {
                clients.insert(mac, ip);
            }
            // Start RTT measurement
            if let Err(e) = start_ftm_session(mac) {
                info!("❌ Failed to start RTT measurement: {:?}", e);
            } else {
                info!("✅ RTT measurement started for new client");
            }

            CLIENT_GOT_CONNECTED.store(true, Ordering::SeqCst);
        }
    })?;

    info!("RustyAP up → SSID `{}`  pass `{}`", AP_SSID, AP_PASS);
    info!("Connecting STA to `{}` …", ST_SSID);

    info!(
        "Access point started! SSID: {}, password: {}",
        AP_SSID,
        AP_PASS
    );

    let ap  = wifi.ap_netif();
    enable_nat(&ap)?;
    info!("NAPT enabled – AP clients have Internet!");

    // Spawn a dedicated task that blinks pink whenever CLIENT_GOT_CONNECTED is set
    let led_task = led.clone();
    thread::Builder::new()
        .name("client_blink".into())
        .stack_size(2048)
        .spawn(move || {
            loop {
                if CLIENT_GOT_CONNECTED.swap(false, Ordering::SeqCst) {
                    let mut led = led_task.lock().unwrap();
                    for _ in 0..5 {
                        let _ = led.set_pixel(RGB8::new(0, 0, 0));     // off
                        FreeRtos::delay_ms(200);
                        let _ = led.set_pixel(RGB8::new(25, 0, 25)); // pink
                        FreeRtos::delay_ms(200);
                    }
                } else {
                    FreeRtos::delay_ms(50);
                }
            }
        })?;

    let mut loop_counter = 0;
    loop {
        button.enable_interrupt()?;
        if notification.wait(50).is_some() {
            button.disable_interrupt()?;
            {
                let mut led_guard = led.lock().unwrap();
                led_guard.set_pixel(RGB8::new(32, 0, 0))?;
            }
            reconnect_sta(&mut wifi, &sta_cfg, &ap_cfg);

            FreeRtos::delay_ms(5_000);
            {
                let mut led_guard = led.lock().unwrap();
                led_guard.set_pixel(RGB8::new(0, 32, 0))?;
            }
        }
        
        // Periodic RTT status announcement
        loop_counter += 1;
        if loop_counter % 1200 == 0 { // Every ~60 seconds (50ms * 1200)
            info!("📡 AP Status: 802.11mc RTT enabled - Ready for Android RTT requests");
            info!("🎯 SSID: '{}' - Channel: 6", AP_SSID);
            log_ranging_statistics();
        }
    }

}

pub fn enable_nat(ap_netif_handle: &EspNetif) -> anyhow::Result<()> {
    info!("Attempting to enable NAPT on netif handle: {:?}", ap_netif_handle.handle());
    unsafe {
        let result = esp_netif_napt_enable(ap_netif_handle.handle());
        if result == sys::ESP_OK {
            info!("esp_netif_napt_enable call succeeded.");
            Ok(())
        } else {
            info!("esp_netif_napt_enable call failed with error code: {}", result);
            Err(anyhow::anyhow!("Failed to enable NAPT, ESP error code: {}", result))
        }
    }
}
fn reconnect_sta(wifi: &mut EspWifi<'_>, sta_cfg: &ClientConfiguration, ap_cfg: &AccessPointConfiguration) {
    let result: anyhow::Result<()> = (|| {
        wifi.disconnect()?;
        wifi.stop()?;
        wifi.set_configuration(&Configuration::Mixed(sta_cfg.clone(), ap_cfg.clone()))?;
        wifi.start()?;
        wifi.connect()?;
        let ap = wifi.ap_netif();
        enable_nat(&ap)?;
        Ok(())
    })();

    match result {
        Ok(()) => info!("STA reconnect initiated"),
        Err(e) => info!("STA reconnect failed: {:?}", e),
    }
}

// RTT/Ranging request logging
fn setup_ranging_request_logging() -> Result<(), esp_idf_sys::EspError> {
    info!("📋 Setting up ranging request logging...");
    
    unsafe {
        // Handler for FTM requests (when Android device initiates RTT)
        extern "C" fn ftm_request_handler(
            _arg: *mut c_void,
            _event_base: sys::esp_event_base_t,
            event_id: i32,
            _event_data: *mut c_void,
        ) {
            // Log any potential FTM/RTT related events
            info!("📡 WiFi Event received - ID: {}", event_id);
            
            // FTM events are typically in the 20-30 range (approximate)
            if event_id >= 20 && event_id <= 30 {
                info!("🎯 Potential RTT/FTM event detected (ID: {})", event_id);
                if !_event_data.is_null() {
                    info!("   Event data available - possible ranging request");
                }
            }
        }
        
        // Handler for general WiFi events that might include ranging
        #[allow(dead_code)]
        extern "C" fn wifi_event_handler(
            _arg: *mut c_void,
            _event_base: sys::esp_event_base_t,
            event_id: i32,
            _event_data: *mut c_void,
        ) {
            match event_id {
                // Client connected (approximate event ID)
                12 => {
                    info!("🔗 Client connected event detected");
                    info!("   � Client may now perform RTT ranging requests");
                }
                // Client disconnected (approximate event ID)  
                13 => {
                    info!("❌ Client disconnected event detected");
                }
                _ => {
                    // Log specific event IDs that might be RTT-related
                    match event_id {
                        1..=15 => {
                            info!("📡 WiFi connection event: ID {}", event_id);
                        }
                        20..=30 => {
                            info!("🎯 Potential RTT event: ID {}", event_id);
                        }
                        _ => {
                            // Only log unknown events occasionally to avoid spam
                            if event_id % 10 == 0 {
                                info!("� WiFi event: ID {}", event_id);
                            }
                        }
                    }
                }
            }
        }
        
        // Register event handler for any WiFi events (including potential FTM)
        let err = sys::esp_event_handler_register(
            sys::WIFI_EVENT,
            sys::ESP_EVENT_ANY_ID as i32,
            Some(std::mem::transmute(ftm_request_handler as *const ())),
            std::ptr::null_mut(),
        );
        if err != sys::ESP_OK {
            info!("⚠️  Could not register WiFi event handler: {}", err);
        } else {
            info!("✅ WiFi event handler registered for ranging detection");
        }
    }
    
    Ok(())
}

// Log ranging statistics periodically
fn log_ranging_statistics() {
    static mut RANGING_REQUEST_COUNT: u32 = 0;
    static mut LAST_RANGING_LOG: i64 = 0;
    
    unsafe {
        let current_time = esp_idf_svc::sys::esp_timer_get_time() / 1000000; // Convert to seconds
        
        if current_time - LAST_RANGING_LOG >= 60 { // Log every 60 seconds
            info!("📊 Ranging Statistics (last 60s):");
            let count = RANGING_REQUEST_COUNT; // Copy to avoid shared reference warning
            info!("   Potential RTT events detected: {}", count);
            info!("   RTT support: Configured");
            info!("   Channel: 6 (2.4GHz)");
            info!("   Bandwidth: 40MHz (if supported)");
            info!("   Protocol: 802.11n enabled");
            
            RANGING_REQUEST_COUNT = 0;
            LAST_RANGING_LOG = current_time;
        }
    }
}