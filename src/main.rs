use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::modem::Modem;
use esp_idf_svc::hal::{
    gpio::{InterruptType, PinDriver, Pull},
    peripherals::Peripherals,
    task::notification::Notification,
};
use esp_idf_svc::handle::RawHandle;
use esp_idf_svc::netif::EspNetif;
use esp_idf_svc::netif::IpEvent;
use esp_idf_svc::nvs::*;
use esp_idf_svc::wifi::*;
use esp_idf_sys as sys;
use esp_wifi_ap::{
    dns_server::DnsServer, mac_hostname_config::MacHostnameConfig, mdns_service::MdnsService, RGB8,
    WS2812RMT,
};
use heapless::String as HeapString;
use log::{info, warn};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};
use std::thread;
use sys::esp_netif_napt_enable;

include!(concat!(env!("OUT_DIR"), "/wifi_networks.rs"));
include!(concat!(env!("OUT_DIR"), "/mac_hostname_mappings.rs"));

// a global map MAC → human-readable name
static MAC_NAMES: Lazy<Mutex<HashMap<[u8; 6], String>>> = Lazy::new(|| Mutex::new(HashMap::new()));

// Fresh pool of 100 names, regenerated every boot
static NAME_POOL: Lazy<Mutex<Vec<String>>> = Lazy::new(|| {
    let mut g = names::Generator::default();
    let mut v = Vec::with_capacity(100);
    for _ in 0..100 {
        v.push(g.next().unwrap());
    }
    Mutex::new(v)
});

static CLIENT_GOT_CONNECTED: AtomicBool = AtomicBool::new(false); // for blinking led everytime someone connected

// Current Wi-Fi network index for STA mode (shared state)
static CURRENT_NETWORK_INDEX: AtomicUsize = AtomicUsize::new(0);

// --- RSSI‑to‑distance calibration constants -------------------------------
/// RSSI you measure at exactly 1 m from the AP (calibrate for your room!)
const MEASURED_POWER_DBM: i8 = -46;
/// Indoor path‑loss exponent (2.0 = open space; ~3.0 = typical office)
const PATH_LOSS_EXPONENT: f32 = 3.0;
// --------------------------------------------------------------------------

const AP_SSID: &str = env!("AP_SSID");
const AP_PASS: &str = env!("AP_PASS");

/// Get current Wi-Fi network for STA mode
fn get_current_sta_network() -> Option<&'static WifiCredentials> {
    let index = CURRENT_NETWORK_INDEX.load(Ordering::SeqCst);
    get_network(index)
}

/// Cycle to next Wi-Fi network for STA mode
fn switch_to_next_sta_network() -> Option<&'static WifiCredentials> {
    let current_index = CURRENT_NETWORK_INDEX.load(Ordering::SeqCst);
    let next_index = cycle_to_next_network(current_index);
    CURRENT_NETWORK_INDEX.store(next_index, Ordering::SeqCst);
    info!(
        "Switched STA to network index: {} -> {}",
        current_index, next_index
    );
    get_network(next_index)
}

/// Create STA configuration from current network
fn create_sta_config() -> anyhow::Result<ClientConfiguration> {
    let network = get_current_sta_network()
        .ok_or_else(|| anyhow::anyhow!("No Wi-Fi networks configured for STA mode"))?;

    info!("Using network cycling STA config: {}", network.ssid);

    let mut ssid: HeapString<32> = HeapString::<32>::new();
    ssid.push_str(network.ssid)
        .map_err(|_| anyhow::anyhow!("SSID too long"))?;

    let mut password: HeapString<64> = HeapString::<64>::new();
    password
        .push_str(network.password)
        .map_err(|_| anyhow::anyhow!("Password too long"))?;

    Ok(ClientConfiguration {
        ssid,
        password,
        ..Default::default()
    })
}

fn main() -> anyhow::Result<()> {
    let client_ips = Mutex::new(HashMap::<[u8; 6], Ipv4Addr>::new());

    // Initialize MAC hostname configuration from generated mappings
    let static_mappings = get_static_mac_mappings();
    let mac_config = Arc::new(MacHostnameConfig::with_mappings(static_mappings));

    // Print configured static mappings
    if mac_config.mapping_count() > 0 {
        info!(
            "Loaded {} static MAC hostname mappings from configuration",
            mac_config.mapping_count()
        );
        mac_config.print_mappings();
    } else {
        info!("No static MAC hostname mappings configured - using dynamic names");
        info!("To add static mappings, set MAC_HOSTNAMES in .env file");
        info!("Format: MAC_HOSTNAMES=aa:bb:cc:dd:ee:ff:hostname1,11:22:33:44:55:66:hostname2");
    }

    // Initialize DNS and mDNS services
    let dns_server = Arc::new(DnsServer::new());
    let mut mdns_service = MdnsService::new();

    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    // button start
    let peripherals = Peripherals::take()?; // singleton?

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
            if let Some(val) = NonZeroU32::new(1) {
                // .unwrap() is fine, this is just more explicit
                notifier.notify_and_yield(val);
            }
        }) {
            Ok(_) => {
                info!(
                    "Successfully subscribed to button interrupt on GPIO {}",
                    button.pin()
                );
            }
            Err(e) => {
                info!(
                    "Failed to subscribe to button interrupt on GPIO {}: {:?}",
                    button.pin(),
                    e
                );
                () // javascript :D
            }
        }
    }
    // button end

    let led = Arc::new(Mutex::new(WS2812RMT::new(
        peripherals.pins.gpio8,   // ESP32‑C6 built‑in RGB LED
        peripherals.rmt.channel0, // any free TX channel
    )?));

    info!(".....Booting up Wi-Fi AP + STA bridge........");

    // Check available networks for STA mode
    let network_count = get_network_count();
    if network_count == 0 {
        warn!("No Wi-Fi networks configured for STA mode!");
    } else {
        info!(
            "Found {} Wi-Fi networks configured for STA cycling",
            network_count
        );
        for i in 0..network_count {
            if let Some(network) = get_network(i) {
                info!("  STA Network {}: {}", i + 1, network.ssid);
            }
        }
    }

    let modem = unsafe { Modem::new() };
    let sysloop = esp_idf_svc::eventloop::EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;
    let mut wifi = EspWifi::new(modem, sysloop.clone(), Some(nvs))?;

    let mut ap_ssid = heapless::String::<32>::new();
    ap_ssid.push_str(AP_SSID).expect("SSID too long");

    let mut ap_pass = heapless::String::<64>::new();
    ap_pass.push_str(AP_PASS).expect("Password too long");

    let ap_cfg = AccessPointConfiguration {
        ssid: ap_ssid,
        password: ap_pass,
        channel: 11, // or 6
        auth_method: AuthMethod::WPA2Personal,
        ..Default::default()
    };

    // Create initial STA configuration from current network
    let sta_cfg = create_sta_config()?;

    wifi.set_configuration(&Configuration::Mixed(sta_cfg.clone(), ap_cfg.clone()))?;
    wifi.start()?;
    wifi.connect()?;

    // Initialize mDNS service after WiFi is configured
    mdns_service.init().map_err(|e| {
        warn!("Failed to initialize mDNS service: {:?}", e);
        e
    })?;

    // Clone DNS services and MAC config for use in the subscription closure
    let dns_clone = Arc::clone(&dns_server);
    let mac_config_clone = Arc::clone(&mac_config);
    let mdns_clone = Arc::new(Mutex::new(mdns_service));
    let mdns_for_subscription = Arc::clone(&mdns_clone);

    // Subscribe for IP events so we can see which IP each station gets
    let _ip_subscription = sysloop.subscribe::<IpEvent, _>(move |event: IpEvent| {
        if let IpEvent::ApStaIpAssigned(assignment) = event {
            let mac = assignment.mac();
            let ip = assignment.ip();

            // Get hostname: first check static MAC mappings, then generate friendly name
            let (friendly_name, hostname) = {
                // Check if this MAC has a static hostname mapping
                if let Some(static_hostname) = mac_config_clone.get_hostname(mac) {
                    info!("Using static hostname for MAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}: {}.local",
                          mac[0], mac[1], mac[2], mac[3], mac[4], mac[5], static_hostname);
                    (static_hostname.clone(), static_hostname)
                } else {
                    // Generate dynamic hostname from name pool
                    let mut map = MAC_NAMES.lock().unwrap();
                    let friendly_name = if let Some(name) = map.get(&mac) {
                        name.clone()
                    } else {
                        let mut pool = NAME_POOL.lock().unwrap();
                        let candidate = pool.pop().unwrap_or_else(|| {
                            format!("device-{:02x}{:02x}{:02x}", mac[3], mac[4], mac[5])
                        });
                        map.insert(mac, candidate.clone());
                        candidate
                    };
                    let hostname = friendly_name.replace(' ', "-").to_lowercase();
                    (friendly_name, hostname)
                }
            };

            // Register with DNS server
            dns_clone.register_hostname(hostname.clone(), ip);

            // Register with mDNS service
            if let Ok(mdns) = mdns_for_subscription.lock() {
                if let Err(e) = mdns.register_device(mac, &friendly_name, ip) {
                    warn!(
                        "Failed to register device {} with mDNS: {:?}",
                        friendly_name, e
                    );
                }
            }

            println!(
                "Client got IP {} – MAC {} – Hostname: {}.local",
                ip,
                mac.iter()
                    .map(|byte| format!("{:02x}", byte))
                    .collect::<Vec<String>>()
                    .join(":"),
                hostname
            );
            info!(
                "STA {} ({}) joined (RSSI will appear in 5\u{202f}s logger)",
                mac.iter()
                    .map(|b| format!("{:02x}", b))
                    .collect::<Vec<_>>()
                    .join(":"),
                friendly_name
            );

            if let Ok(mut map) = client_ips.lock() {
                map.insert(mac, ip);
            }
            CLIENT_GOT_CONNECTED.store(true, Ordering::SeqCst);
        }
    })?;

    // Keep mdns_service wrapped for later use if needed
    // let mdns_service = mdns_clone;

    info!("RustyAP up → SSID `{}`  pass `{}`", AP_SSID, AP_PASS);

    if let Some(network) = get_current_sta_network() {
        info!("Connecting STA to `{}` …", network.ssid);
    } else {
        info!("No STA networks configured for cycling");
    }

    info!(
        "Access point started! SSID: {}, password: {}",
        AP_SSID, AP_PASS
    );

    let ap = wifi.ap_netif();
    enable_nat(&ap)?;
    info!("NAPT enabled – AP clients have Internet!");

    // Start DNS server on AP interface
    if let Err(e) = dns_server.start(&ap) {
        warn!("Failed to start DNS server: {:?}", e);
    } else {
        info!("DNS server started successfully");
    }

    // Configure DHCP to advertise DNS server
    if let Err(e) = dns_server.configure_dhcp_dns(&ap) {
        warn!("Failed to configure DHCP DNS: {:?}", e);
    } else {
        info!("DHCP configured to advertise router as DNS server");
    }

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
                        let _ = led.set_pixel(RGB8::new(0, 0, 0)); // off
                        FreeRtos::delay_ms(200);
                        let _ = led.set_pixel(RGB8::new(25, 0, 25)); // pink
                        FreeRtos::delay_ms(200);
                    }
                } else {
                    FreeRtos::delay_ms(50);
                }
            }
        })?;

    thread::Builder::new()
        .name("sta_rssi_logger".into())
        .stack_size(4096)
        .spawn(|| loop {
            log_all_sta_distances();
            FreeRtos::delay_ms(3_000);
        })?;

    // Spawn DNS status reporting task
    let dns_reporter = Arc::clone(&dns_server);
    thread::Builder::new()
        .name("dns_status_reporter".into())
        .stack_size(4096)
        .spawn(move || loop {
            let hostnames = dns_reporter.list_hostnames();
            if !hostnames.is_empty() {
                info!("🏠 Registered hostnames ({}):", hostnames.len());
                for (hostname, ip) in hostnames {
                    info!("   {} -> {}", hostname, ip);
                }
            }
            FreeRtos::delay_ms(30_000); // Report every 30 seconds
        })?;

    // Log initial DNS configuration
    info!("🌐 DNS Server Configuration:");
    info!("   - mDNS service initialized and running");
    info!("   - Router hostname: esp-router.local");
    info!("   - DNS resolution enabled for .local domains");
    info!("   - DHCP clients will use router as DNS server");
    info!("   - Static MAC mappings: {}", mac_config.mapping_count());

    loop {
        button.enable_interrupt()?;
        if notification.wait(50).is_some() {
            button.disable_interrupt()?;
            {
                let mut led_guard = led.lock().unwrap();
                led_guard.set_pixel(RGB8::new(32, 0, 0))?;
            }

            // Switch to next network and reconnect
            switch_to_next_sta_network();
            if let Some(current_network) = get_current_sta_network() {
                info!(
                    "🔄 Button pressed - switching STA to network: {}",
                    current_network.ssid
                );
            }

            match create_sta_config() {
                Ok(new_sta_cfg) => {
                    reconnect_sta(&mut wifi, &new_sta_cfg, &ap_cfg);
                }
                Err(e) => {
                    info!("Failed to create STA config: {:?}", e);
                }
            }

            FreeRtos::delay_ms(5_000);
            {
                let mut led_guard = led.lock().unwrap();
                led_guard.set_pixel(RGB8::new(0, 32, 0))?;
            }
        } else {
            button.disable_interrupt()?;
        }
    }
}

/// Log RSSI and distance for every connected station on the Soft‑AP.
fn log_all_sta_distances() {
    unsafe {
        let mut sta_list: sys::wifi_sta_list_t = core::mem::zeroed();

        if sys::esp_wifi_ap_get_sta_list(&mut sta_list as *mut _) != sys::ESP_OK {
            info!("Failed to fetch STA list for RSSI/dist logging");
            return;
        }

        sta_list.sta[0..(sta_list.num as usize)]
            .iter()
            .filter(|sta| sta.rssi != 0)  // Filter out entries with no RSSI data
            .for_each(|sta| {
                let rssi = sta.rssi as i8;
                let distance_m = rssi_to_distance(
                    rssi,
                    MEASURED_POWER_DBM,
                    PATH_LOSS_EXPONENT,
                );

                let mac = sta.mac;
                let mac_key = mac; // treat it as a key: `[u8; 6]`

                let human_name = {
                    let mut map = MAC_NAMES.lock().unwrap();
                    if let Some(name) = map.get(&mac_key) {
                        name.clone()
                    } else {
                        let mut pool = NAME_POOL.lock().unwrap();
                        let candidate = pool.pop().unwrap_or_else(|| "nameless-device".into());
                        map.insert(mac_key, candidate.clone());
                        candidate
                    }
                };

                info!(
                    "📶 RSSI {:>3} dBm → ≈{:.1} m (client {} / {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x})",
                    rssi,
                    distance_m,
                    human_name,
                    mac[0], mac[1], mac[2],
                    mac[3], mac[4], mac[5],
                );
            });
    }
}

pub fn enable_nat(ap_netif_handle: &EspNetif) -> anyhow::Result<()> {
    info!(
        "Attempting to enable NAPT on netif handle: {:?}",
        ap_netif_handle.handle()
    );
    unsafe {
        let result = esp_netif_napt_enable(ap_netif_handle.handle());
        if result == sys::ESP_OK {
            info!("esp_netif_napt_enable call succeeded.");
            Ok(())
        } else {
            info!(
                "esp_netif_napt_enable call failed with error code: {}",
                result
            );
            Err(anyhow::anyhow!(
                "Failed to enable NAPT, ESP error code: {}",
                result
            ))
        }
    }
}

fn reconnect_sta(
    wifi: &mut EspWifi<'_>,
    sta_cfg: &ClientConfiguration,
    ap_cfg: &AccessPointConfiguration,
) {
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

pub fn rssi_to_distance(rssi_dbm: i8, measured_power_dbm: i8, path_loss_exponent: f32) -> f32 {
    // delta = how many dB weaker than the 1-metre reference
    let delta_db = (measured_power_dbm as i16 - rssi_dbm as i16) as f32;
    10_f32.powf(delta_db / (10.0 * path_loss_exponent))
}
