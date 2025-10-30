#[cfg(feature = "esp32s3")]
use core::sync::atomic::AtomicU8;
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
use esp_wifi_ap::{format_mac, render_sta_table, RssiDbm, RssiRange, StaSnapshot, RGB8, WS2812RMT};
use heapless::String as HeapString;
use log::{info, warn};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::num::NonZeroU32;
use std::ptr;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use sys::esp_netif_napt_enable;

use dhcp::{DhcpServerState, DHCP_RESERVATIONS};
use esp_idf_sys::esp_wifi_deauth_sta;

#[derive(Clone, Copy)]
struct NetifHandle(*mut sys::esp_netif_t);

unsafe impl Send for NetifHandle {}
unsafe impl Sync for NetifHandle {}

mod dhcp;
mod led;

include!(concat!(env!("OUT_DIR"), "/wifi_networks.rs"));
include!(concat!(env!("OUT_DIR"), "/dhcp_leases.rs"));

// a global map MAC → human-readable name
static MAC_NAMES: Lazy<Mutex<HashMap<[u8; 6], String>>> = Lazy::new(|| Mutex::new(HashMap::new()));
static LAST_KNOWN_IPS: Lazy<Mutex<HashMap<[u8; 6], Ipv4Addr>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static STA_AIDS: Lazy<Mutex<HashMap<[u8; 6], u16>>> = Lazy::new(|| Mutex::new(HashMap::new()));

// Fresh pool of 100 names, regenerated every boot
static NAME_POOL: Lazy<Mutex<Vec<String>>> = Lazy::new(|| {
    let mut g = names::Generator::default();
    let mut v = Vec::with_capacity(100);
    for _ in 0..100 {
        v.push(g.next().unwrap());
    }
    Mutex::new(v)
});

#[cfg(not(feature = "esp32s3"))]
static CLIENT_GOT_CONNECTED: AtomicBool = AtomicBool::new(false); // for blinking led everytime someone connected
static STA_RECONNECT_PENDING: AtomicBool = AtomicBool::new(false);
#[cfg(feature = "esp32s3")]
static LED_BLINK_EVENT: AtomicU8 = AtomicU8::new(0);

// Current Wi-Fi network index for STA mode (shared state)
static CURRENT_NETWORK_INDEX: AtomicUsize = AtomicUsize::new(0);

#[cfg(feature = "esp32s3")]
const LED_EVENT_CONNECT: u8 = 1;
#[cfg(feature = "esp32s3")]
const LED_EVENT_DISCONNECT: u8 = 2;

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
    let client_ips = Arc::new(Mutex::new(HashMap::<[u8; 6], Ipv4Addr>::new()));

    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    unsafe {
        // Raise global ESP-IDF log verbosity to DEBUG for richer diagnostics
        esp_idf_sys::esp_log_level_set(ptr::null(), esp_idf_sys::esp_log_level_t_ESP_LOG_DEBUG);
    }

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

    let channel0 = peripherals.rmt.channel0;
    #[cfg(feature = "esp32s3")]
    let led_driver = led::create_status_led(peripherals.pins.gpio21, channel0)?;
    #[cfg(not(feature = "esp32s3"))]
    let led_driver = led::create_status_led(peripherals.pins.gpio8, channel0)?;
    let led = Arc::new(Mutex::new(led_driver));

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
        max_connections: 16,
        ..Default::default()
    };

    // Create initial STA configuration from current network
    let sta_cfg = create_sta_config()?;

    wifi.set_configuration(&Configuration::Mixed(sta_cfg.clone(), ap_cfg.clone()))?;
    wifi.start()?;
    wifi.connect()?;

    let ap = wifi.ap_netif();
    let ap_handle = NetifHandle(ap.handle());

    let dhcp_state = Arc::new(Mutex::new(init_dhcp_state(ap_handle)));

    let dhcp_state_for_wifi = Arc::clone(&dhcp_state);
    let client_ips_for_wifi = Arc::clone(&client_ips);
    let led_for_wifi = Arc::clone(&led);
    let _wifi_subscription =
        sysloop.subscribe::<WifiEvent, _>(move |event: WifiEvent| match event {
            WifiEvent::StaDisconnected(details) => {
                let ssid = String::from_utf8_lossy(details.ssid());
                let reason = details.reason();
                warn!(
                    "STA disconnected from `{}` (reason {}) – scheduling reconnect",
                    ssid, reason
                );
                STA_RECONNECT_PENDING.store(true, Ordering::SeqCst);
                #[cfg(feature = "esp32s3")]
                {
                    LED_BLINK_EVENT.store(LED_EVENT_DISCONNECT, Ordering::SeqCst);
                }
                #[cfg(not(feature = "esp32s3"))]
                if let Ok(mut led) = led_for_wifi.lock() {
                    let color = led::color_red();
                    info!(
                        "Status LED -> red (reconnect) (r={}, g={}, b={})",
                        color.r, color.g, color.b
                    );
                    let _ = led.set_pixel(color); // red indicates reconnecting
                }
            }
            WifiEvent::StaStopped => {
                warn!("STA interface stopped – scheduling reconnect");
                STA_RECONNECT_PENDING.store(true, Ordering::SeqCst);
                #[cfg(feature = "esp32s3")]
                {
                    LED_BLINK_EVENT.store(LED_EVENT_DISCONNECT, Ordering::SeqCst);
                }
                #[cfg(not(feature = "esp32s3"))]
                if let Ok(mut led) = led_for_wifi.lock() {
                    let color = led::color_red();
                    info!(
                        "Status LED -> red (STA stopped) (r={}, g={}, b={})",
                        color.r, color.g, color.b
                    );
                    let _ = led.set_pixel(color);
                }
            }
            WifiEvent::StaBeaconTimeout => {
                warn!("STA beacon timeout – scheduling reconnect");
                STA_RECONNECT_PENDING.store(true, Ordering::SeqCst);
                #[cfg(feature = "esp32s3")]
                {
                    LED_BLINK_EVENT.store(LED_EVENT_DISCONNECT, Ordering::SeqCst);
                }
                #[cfg(not(feature = "esp32s3"))]
                if let Ok(mut led) = led_for_wifi.lock() {
                    let color = led::color_red();
                    info!(
                        "Status LED -> red (beacon timeout) (r={}, g={}, b={})",
                        color.r, color.g, color.b
                    );
                    let _ = led.set_pixel(color);
                }
            }
            WifiEvent::StaConnected(details) => {
                let ssid = String::from_utf8_lossy(details.ssid());
                info!("STA connected to `{}`", ssid);
                STA_RECONNECT_PENDING.store(false, Ordering::SeqCst);
                #[cfg(feature = "esp32s3")]
                {
                    LED_BLINK_EVENT.store(LED_EVENT_CONNECT, Ordering::SeqCst);
                }
                #[cfg(not(feature = "esp32s3"))]
                if let Ok(mut led) = led_for_wifi.lock() {
                    let color = led::color_green();
                    info!(
                        "Status LED -> green (STA connected) (r={}, g={}, b={})",
                        color.r, color.g, color.b
                    );
                    let _ = led.set_pixel(color); // green indicates STA link up
                }
            }
            WifiEvent::ApStaConnected(conn) => {
                let mac = conn.mac();
                if let Ok(mut aids) = STA_AIDS.lock() {
                    aids.insert(mac, u16::from(conn.aid()));
                }
                let prev_ip = LAST_KNOWN_IPS
                    .lock()
                    .ok()
                    .and_then(|mut last| last.remove(&mac));

                if let Some(prev_ip) = prev_ip {
                    if let Ok(mut map) = client_ips_for_wifi.lock() {
                        map.insert(mac, prev_ip);
                    }
                }
                if DHCP_RESERVATIONS.contains_key(&mac) {
                    if let Ok(mut guard) = dhcp_state_for_wifi.lock() {
                        if ensure_dhcp_state(&mut guard, ap_handle) {
                            if let Some(state) = guard.as_ref() {
                                state.touch_static_lease(&mac);
                            }
                        }
                    }
                }
            }
            WifiEvent::ApStaDisconnected(disc) => {
                let mac = disc.mac();
                if let Ok(mut aids) = STA_AIDS.lock() {
                    aids.remove(&mac);
                }
                let previous_ip = if let Ok(mut map) = client_ips_for_wifi.lock() {
                    map.remove(&mac)
                } else {
                    None
                };
                if let Some(ip) = previous_ip {
                    if let Ok(mut last) = LAST_KNOWN_IPS.lock() {
                        last.insert(mac, ip);
                    }
                }
            }
            WifiEvent::ApStarted => {
                if let Ok(mut guard) = dhcp_state_for_wifi.lock() {
                    *guard = init_dhcp_state(ap_handle);
                }
            }
            WifiEvent::ApStopped => {
                if let Ok(mut guard) = dhcp_state_for_wifi.lock() {
                    if guard.take().is_some() {
                        info!("Static DHCP reservations suspended (SoftAP stopped)");
                    }
                }
            }
            _ => {}
        })?;

    let dhcp_state_for_ip = Arc::clone(&dhcp_state);
    let client_ips_for_ip = Arc::clone(&client_ips);
    let _ip_subscription = sysloop.subscribe::<IpEvent, _>(move |event: IpEvent| {
        if let IpEvent::ApStaIpAssigned(assignment) = event {
            let mac = assignment.mac();
            let ip = Ipv4Addr::from(assignment.ip().octets());
            let mac_str = format_mac(&mac);

            println!("Client got IP {} – MAC {}", ip, mac_str);
            info!(
                "STA {} joined (RSSI will appear in 5\u{202f}s logger)",
                mac_str.to_lowercase()
            );

            if let Ok(mut guard) = dhcp_state_for_ip.lock() {
                if ensure_dhcp_state(&mut guard, ap_handle) {
                    if let Some(state) = guard.as_ref() {
                        state.clamp_dynamic_cursor();
                        if DHCP_RESERVATIONS.contains_key(&mac) {
                            state.touch_static_lease(&mac);
                        }
                    }
                }
            }

            resolve_ip_conflicts(&mac, ip, &client_ips_for_ip, &dhcp_state_for_ip);
            if let Ok(mut last) = LAST_KNOWN_IPS.lock() {
                last.insert(mac, ip);
            }
            #[cfg(not(feature = "esp32s3"))]
            CLIENT_GOT_CONNECTED.store(true, Ordering::SeqCst);
        }
    })?;

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

    enable_nat(&ap)?;
    info!("NAPT enabled – AP clients have Internet!");

    #[cfg(not(feature = "esp32s3"))]
    {
        // Spawn a dedicated task that blinks pink whenever CLIENT_GOT_CONNECTED is set
        let led_task = led.clone();
        thread::Builder::new()
            .name("client_blink".into())
            .stack_size(8192)
            .spawn(move || loop {
                if CLIENT_GOT_CONNECTED.swap(false, Ordering::SeqCst) {
                    let mut led = led_task.lock().unwrap();
                    for _ in 0..5 {
                        let color_off = led::color_off();
                        info!(
                            "Status LED -> off (blink cycle) (r={}, g={}, b={})",
                            color_off.r, color_off.g, color_off.b
                        );
                        let _ = led.set_pixel(color_off);
                        FreeRtos::delay_ms(200);
                        let color_pink = led::color_pink();
                        info!(
                            "Status LED -> pink (blink cycle) (r={}, g={}, b={})",
                            color_pink.r, color_pink.g, color_pink.b
                        );
                        let _ = led.set_pixel(color_pink);
                        FreeRtos::delay_ms(200);
                    }
                } else {
                    FreeRtos::delay_ms(50);
                }
            })?;
    }

    #[cfg(feature = "esp32s3")]
    {
        let led_task = led.clone();
        thread::Builder::new()
            .name("status_led".into())
            .stack_size(4096)
            .spawn(move || loop {
                let event = LED_BLINK_EVENT.swap(0, Ordering::SeqCst);
                if event == LED_EVENT_CONNECT {
                    let mut led = led_task.lock().unwrap();
                    for idx in 0..3 {
                        let color = led::color_green();
                        info!(
                            "Status LED -> green blink {}/3 (r={}, g={}, b={})",
                            idx + 1,
                            color.r,
                            color.g,
                            color.b
                        );
                        let _ = led.set_pixel(color);
                        FreeRtos::delay_ms(150);
                        let off = led::color_off();
                        let _ = led.set_pixel(off);
                        FreeRtos::delay_ms(150);
                    }
                } else if event == LED_EVENT_DISCONNECT {
                    let mut led = led_task.lock().unwrap();
                    for idx in 0..2 {
                        let color = led::color_red();
                        info!(
                            "Status LED -> red blink {}/2 (r={}, g={}, b={})",
                            idx + 1,
                            color.r,
                            color.g,
                            color.b
                        );
                        let _ = led.set_pixel(color);
                        FreeRtos::delay_ms(150);
                        let off = led::color_off();
                        let _ = led.set_pixel(off);
                        FreeRtos::delay_ms(150);
                    }
                } else {
                    FreeRtos::delay_ms(50);
                }
            })?;
    }

    let client_ips_for_rssi = Arc::clone(&client_ips);
    const TABLE_OUTPUT_INTERVAL_SECS: u64 = 10;
    const RSSI_COLLECTION_INTERVAL_MS: u32 = 1_000;
    thread::Builder::new()
        .name("sta_rssi_logger".into())
        .stack_size(12288)
        .spawn(move || {
            let mut rssi_stats: HashMap<[u8; 6], RssiRange> = HashMap::new();
            let mut last_table = Instant::now();

            loop {
                match collect_sta_snapshots(&client_ips_for_rssi) {
                    Ok(snapshots) => {
                        for snap in &snapshots {
                            rssi_stats
                                .entry(snap.mac)
                                .and_modify(|range| range.update(snap.rssi))
                                .or_insert(RssiRange::new(snap.rssi));
                        }

                        if last_table.elapsed() >= Duration::from_secs(TABLE_OUTPUT_INTERVAL_SECS) {
                            if let Some(table) = render_sta_table(&snapshots, &rssi_stats) {
                                info!("\n{}", table);
                            }
                            rssi_stats.clear();
                            last_table = Instant::now();
                        }
                    }
                    Err(err) => {
                        warn!("Failed to update STA stats: {:?}", err);
                    }
                }

                FreeRtos::delay_ms(RSSI_COLLECTION_INTERVAL_MS);
            }
        })?;

    let mut last_sta_reconnect_attempt: Option<Instant> = None;

    loop {
        button.enable_interrupt()?;
        if notification.wait(50).is_some() {
            button.disable_interrupt()?;
            {
                let mut led_guard = led.lock().unwrap();
                let color = led::color_red();
                info!(
                    "Status LED -> red (button pressed) (r={}, g={}, b={})",
                    color.r, color.g, color.b
                );
                led_guard.set_pixel(color)?;
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
                let color = led::color_green();
                info!(
                    "Status LED -> green (button release) (r={}, g={}, b={})",
                    color.r, color.g, color.b
                );
                led_guard.set_pixel(color)?;
            }
        } else {
            button.disable_interrupt()?;
        }

        if STA_RECONNECT_PENDING.load(Ordering::SeqCst) {
            let should_attempt = match last_sta_reconnect_attempt {
                Some(last) => last.elapsed() >= Duration::from_secs(20),
                None => true,
            };

            if should_attempt {
                info!("STA reconnect timer elapsed – attempting to reconnect");
                if let Ok(mut led_guard) = led.lock() {
                    let color = led::color_red();
                    info!(
                        "Status LED -> red (reconnect attempt) (r={}, g={}, b={})",
                        color.r, color.g, color.b
                    );
                    let _ = led_guard.set_pixel(color);
                }
                last_sta_reconnect_attempt = Some(Instant::now());
                match create_sta_config() {
                    Ok(new_sta_cfg) => {
                        reconnect_sta(&mut wifi, &new_sta_cfg, &ap_cfg);
                    }
                    Err(err) => {
                        warn!("Unable to prepare STA config for reconnect: {:?}", err);
                    }
                }
            }
        } else {
            last_sta_reconnect_attempt = None;
        }
    }
}

fn collect_sta_snapshots(
    client_ips: &Arc<Mutex<HashMap<[u8; 6], Ipv4Addr>>>,
) -> anyhow::Result<Vec<StaSnapshot>> {
    unsafe {
        let mut sta_list: sys::wifi_sta_list_t = core::mem::zeroed();
        if sys::esp_wifi_ap_get_sta_list(&mut sta_list as *mut _) != sys::ESP_OK {
            anyhow::bail!("Failed to fetch STA list for RSSI/dist logging");
        }

        let mut snapshots = Vec::new();

        for sta in sta_list.sta[0..(sta_list.num as usize)]
            .iter()
            .filter(|sta| sta.rssi != 0)
        {
            let rssi = sta.rssi as RssiDbm;
            let distance = rssi_to_distance(rssi, MEASURED_POWER_DBM, PATH_LOSS_EXPONENT);
            let mac = sta.mac;

            let name = {
                let mut map = MAC_NAMES.lock().unwrap();
                if let Some(existing) = map.get(&mac) {
                    existing.clone()
                } else {
                    let mut pool = NAME_POOL.lock().unwrap();
                    let candidate = pool.pop().unwrap_or_else(|| "nameless-device".into());
                    map.insert(mac, candidate.clone());
                    candidate
                }
            };

            let ip = client_ips
                .lock()
                .ok()
                .and_then(|ips| ips.get(&mac).copied());

            snapshots.push(StaSnapshot {
                mac,
                name,
                rssi,
                distance,
                ip,
            });
        }

        Ok(snapshots)
    }
}

fn resolve_ip_conflicts(
    new_mac: &[u8; 6],
    new_ip: Ipv4Addr,
    client_ips: &Arc<Mutex<HashMap<[u8; 6], Ipv4Addr>>>,
    _dhcp_state: &Arc<Mutex<Option<DhcpServerState>>>,
) {
    if let Some(reserved_ip) = DHCP_RESERVATIONS.get(new_mac) {
        if *reserved_ip != new_ip {
            warn!(
                "Reservation mismatch for {}: expected {}, got {}. Forcing reconnect.",
                format_mac(new_mac),
                reserved_ip,
                new_ip
            );
            deauth_mac(new_mac);
            if let Ok(mut map) = client_ips.lock() {
                map.remove(new_mac);
            }
            return;
        }
    }

    let mut conflict_mac: Option<[u8; 6]> = None;
    if let Ok(map) = client_ips.lock() {
        for (mac, ip) in map.iter() {
            if mac != new_mac && *ip == new_ip {
                conflict_mac = Some(*mac);
                break;
            }
        }
    }

    if let Some(conflict) = conflict_mac {
        let conflict_reserved = DHCP_RESERVATIONS.contains_key(&conflict);
        let new_reserved = DHCP_RESERVATIONS.contains_key(new_mac);
        let conflict_ip_reserved = DHCP_RESERVATIONS
            .get(new_mac)
            .map(|reserved| *reserved == new_ip)
            .unwrap_or(false);

        let target = if new_reserved && !conflict_reserved {
            conflict
        } else if conflict_reserved && !conflict_ip_reserved {
            *new_mac
        } else if !new_reserved && conflict_reserved {
            *new_mac
        } else {
            conflict
        };

        if let Some(aid) = STA_AIDS
            .lock()
            .ok()
            .and_then(|map| map.get(&target).copied())
        {
            warn!(
                "Deauthenticating {} to resolve IP {} conflict",
                format_mac(&target),
                new_ip
            );
            unsafe {
                let err = esp_wifi_deauth_sta(aid);
                if err != sys::ESP_OK {
                    warn!(
                        "Failed to deauth {} (AID {}): {:?}",
                        format_mac(&target),
                        aid,
                        err
                    );
                }
            }
        } else {
            warn!(
                "Unable to deauth {} for IP {} conflict (missing AID entry)",
                format_mac(&target),
                new_ip
            );
        }
    }

    if let Ok(mut map) = client_ips.lock() {
        map.insert(*new_mac, new_ip);
    }
}

fn deauth_mac(mac: &[u8; 6]) {
    let aid_opt = STA_AIDS.lock().ok().and_then(|map| map.get(mac).copied());
    if let Some(aid) = aid_opt {
        unsafe {
            let err = esp_wifi_deauth_sta(aid);
            if err != sys::ESP_OK {
                warn!(
                    "Failed to deauth {} (AID {}): {:?}",
                    format_mac(mac),
                    aid,
                    err
                );
            } else {
                info!(
                    "Deauthenticated {} (AID {}) to force DHCP renewal",
                    format_mac(mac),
                    aid
                );
            }
        }
    } else {
        warn!(
            "Unable to deauth {} – missing association ID",
            format_mac(mac)
        );
    }
}

fn init_dhcp_state(handle: NetifHandle) -> Option<DhcpServerState> {
    if DHCP_RESERVATIONS.is_empty() {
        info!("No static DHCP reservations configured");
        return None;
    }

    match DhcpServerState::new(handle.0) {
        Ok(state) => {
            info!(
                "Static DHCP reservations enabled for {} device(s)",
                DHCP_RESERVATIONS.len()
            );
            Some(state)
        }
        Err(err) => {
            warn!(
                "Failed to initialize static DHCP reservations: {:?}. Reservations disabled.",
                err
            );
            None
        }
    }
}

fn ensure_dhcp_state(state: &mut Option<DhcpServerState>, handle: NetifHandle) -> bool {
    if state.is_some() {
        return true;
    }

    if DHCP_RESERVATIONS.is_empty() {
        return false;
    }

    match DhcpServerState::new(handle.0) {
        Ok(new_state) => {
            info!(
                "Static DHCP reservations re-enabled for {} device(s)",
                DHCP_RESERVATIONS.len()
            );
            *state = Some(new_state);
            true
        }
        Err(err) => {
            warn!(
                "Failed to initialize static DHCP reservations: {:?}. Reservations disabled.",
                err
            );
            false
        }
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
