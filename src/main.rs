use core::ffi::c_void;
use core::fmt::Write as FmtWrite;
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
use esp_idf_sys::ESP_OK;
use esp_wifi_ap::{RGB8, WS2812RMT};
use heapless::String as HeapString;
use log::{info, warn};
use once_cell::sync::Lazy;
use std::collections::{HashMap, VecDeque};
use std::ffi::CStr;
use std::net::Ipv4Addr;
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use sys::esp_netif_napt_enable;

include!(concat!(env!("OUT_DIR"), "/wifi_networks.rs"));
include!(concat!(env!("OUT_DIR"), "/dhcp_leases.rs"));

const DYNAMIC_POOL_START: u32 = 2;
const STATIC_POOL_START: u32 = 100;

static DHCP_RESERVATIONS: Lazy<HashMap<[u8; 6], Ipv4Addr>> = Lazy::new(|| {
    let mut map = HashMap::new();
    for lease in STATIC_LEASES {
        map.insert(
            lease.mac,
            Ipv4Addr::new(lease.ip[0], lease.ip[1], lease.ip[2], lease.ip[3]),
        );
    }
    map
});

#[repr(C)]
#[derive(Clone, Copy)]
struct DhcpsLease {
    enable: bool,
    start_ip: sys::ip4_addr_t,
    end_ip: sys::ip4_addr_t,
}

impl Default for DhcpsLease {
    fn default() -> Self {
        Self {
            enable: false,
            start_ip: sys::ip4_addr_t { addr: 0 },
            end_ip: sys::ip4_addr_t { addr: 0 },
        }
    }
}

impl DhcpsLease {
    fn from_range(start: Ipv4Addr, end: Ipv4Addr) -> Self {
        Self {
            enable: true,
            start_ip: ip4_from_ipv4(start),
            end_ip: ip4_from_ipv4(end),
        }
    }
}

#[derive(Clone, Copy)]
struct ActiveOverride {
    mac: [u8; 6],
    ip: Ipv4Addr,
}

struct DhcpReservationManager {
    handle: *mut sys::esp_netif_t,
    default: DhcpsLease,
    queue: VecDeque<[u8; 6]>,
    active: Option<ActiveOverride>,
    network_base: u32,
    netmask: u32,
    broadcast: u32,
}

unsafe impl Send for DhcpReservationManager {}

impl DhcpReservationManager {
    fn new(handle: *mut sys::esp_netif_t) -> anyhow::Result<Self> {
        let ip_info = fetch_ip_info(handle)?;
        let netmask = u32::from(esp_ip4_to_ipv4(ip_info.netmask));
        let network_base = u32::from(esp_ip4_to_ipv4(ip_info.ip)) & netmask;
        let broadcast = network_base | (!netmask);

        anyhow::ensure!(
            STATIC_POOL_START > DYNAMIC_POOL_START,
            "Static pool start must exceed dynamic pool start"
        );

        let dynamic_start = Ipv4Addr::from(network_base + DYNAMIC_POOL_START);
        let dynamic_end = Ipv4Addr::from(network_base + STATIC_POOL_START.saturating_sub(1));
        let desired_default = DhcpsLease::from_range(dynamic_start, dynamic_end);
        set_dhcp_lease(handle, &desired_default)?;

        let (start, end) = lease_range(&desired_default);
        info!("Captured default DHCP pool: {} – {}", start, end,);

        Ok(Self {
            handle,
            default: desired_default,
            queue: VecDeque::new(),
            active: None,
            network_base,
            netmask,
            broadcast,
        })
    }

    fn handle_sta_connected(&mut self, mac: [u8; 6]) -> anyhow::Result<()> {
        let Some(&ip) = DHCP_RESERVATIONS.get(&mac) else {
            return Ok(());
        };

        if self
            .active
            .as_ref()
            .map(|current| current.mac == mac)
            .unwrap_or(false)
        {
            return Ok(());
        }

        if self.active.is_some() {
            if !self.queue.iter().any(|queued| queued == &mac) {
                self.queue.push_back(mac);
                info!(
                    "Queued DHCP override for {} (waiting for current reservation to finish)",
                    format_mac(&mac)
                );
            }
            return Ok(());
        }

        self.apply_override(mac, ip)
    }

    fn handle_sta_disconnected(&mut self, mac: [u8; 6]) -> anyhow::Result<()> {
        self.queue.retain(|queued| queued != &mac);

        if self
            .active
            .as_ref()
            .map(|current| current.mac == mac)
            .unwrap_or(false)
        {
            self.restore_default()?;
            self.activate_next_in_queue()?;
        }

        Ok(())
    }

    fn handle_ip_assigned(&mut self, mac: [u8; 6], ip: Ipv4Addr) -> anyhow::Result<()> {
        if let Some(active) = self.active {
            if active.mac == mac {
                if active.ip != ip {
                    warn!(
                        "Reservation mismatch for {}: expected {}, got {}",
                        format_mac(&mac),
                        active.ip,
                        ip
                    );
                } else {
                    info!("Reserved IP {} confirmed for {}", ip, format_mac(&mac));
                }
                self.restore_default()?;
                self.activate_next_in_queue()?;
            }
        }

        Ok(())
    }

    fn apply_override(&mut self, mac: [u8; 6], ip: Ipv4Addr) -> anyhow::Result<()> {
        let lease = self.make_reservation(ip)?;
        set_dhcp_lease(self.handle, &lease)?;
        info!("Temporarily pinning {} to {}", format_mac(&mac), ip);
        self.active = Some(ActiveOverride { mac, ip });
        Ok(())
    }

    fn make_reservation(&self, ip: Ipv4Addr) -> anyhow::Result<DhcpsLease> {
        let ip_u32 = u32::from(ip);
        anyhow::ensure!(
            (ip_u32 & self.netmask) == self.network_base,
            "Reservation IP {} must be within the AP subnet",
            ip
        );

        let host_id = ip_u32
            .checked_sub(self.network_base)
            .ok_or_else(|| anyhow::anyhow!("Reservation IP {} underflows network base", ip))?;

        anyhow::ensure!(
            host_id >= STATIC_POOL_START,
            "Reservation IP {} must be >= {}",
            ip,
            Ipv4Addr::from(self.network_base + STATIC_POOL_START)
        );

        let max_reservable_host = (self
            .broadcast
            .checked_sub(self.network_base)
            .ok_or_else(|| anyhow::anyhow!("Invalid broadcast address calculation"))?)
        .saturating_sub(2);
        anyhow::ensure!(
            host_id <= max_reservable_host,
            "Reservation IP {} must be <= {}",
            ip,
            Ipv4Addr::from(self.network_base + max_reservable_host)
        );

        let next = Ipv4Addr::from(ip_u32 + 1);
        Ok(DhcpsLease::from_range(ip, next))
    }

    fn restore_default(&mut self) -> anyhow::Result<()> {
        if self.active.is_some() {
            let (start, end) = lease_range(&self.default);
            info!("Restoring default DHCP pool {} – {}", start, end);
            set_dhcp_lease(self.handle, &self.default)?;
        }
        self.active = None;
        Ok(())
    }

    fn activate_next_in_queue(&mut self) -> anyhow::Result<()> {
        while let Some(next_mac) = self.queue.pop_front() {
            if let Some(&ip) = DHCP_RESERVATIONS.get(&next_mac) {
                self.apply_override(next_mac, ip)?;
                break;
            }
        }
        Ok(())
    }
}

fn set_dhcp_lease(handle: *mut sys::esp_netif_t, lease: &DhcpsLease) -> anyhow::Result<()> {
    let stop_err = unsafe { sys::esp_netif_dhcps_stop(handle) };
    if stop_err != ESP_OK && stop_err != sys::ESP_ERR_ESP_NETIF_DHCP_ALREADY_STOPPED {
        return Err(esp_error(stop_err));
    }

    let mut lease_copy = *lease;
    let set_err = unsafe {
        sys::esp_netif_dhcps_option(
            handle,
            sys::esp_netif_dhcp_option_mode_t_ESP_NETIF_OP_SET,
            sys::esp_netif_dhcp_option_id_t_ESP_NETIF_REQUESTED_IP_ADDRESS,
            &mut lease_copy as *mut _ as *mut c_void,
            core::mem::size_of::<DhcpsLease>() as u32,
        )
    };

    let start_err = unsafe { sys::esp_netif_dhcps_start(handle) };
    if start_err != ESP_OK && start_err != sys::ESP_ERR_ESP_NETIF_DHCP_ALREADY_STARTED {
        return Err(esp_error(start_err));
    }

    if set_err == ESP_OK {
        Ok(())
    } else {
        Err(esp_error(set_err))
    }
}

fn lease_range(lease: &DhcpsLease) -> (Ipv4Addr, Ipv4Addr) {
    (ip4_to_ipv4(lease.start_ip), ip4_to_ipv4(lease.end_ip))
}

fn fetch_ip_info(handle: *mut sys::esp_netif_t) -> anyhow::Result<sys::esp_netif_ip_info_t> {
    let mut info = sys::esp_netif_ip_info_t {
        ip: sys::esp_ip4_addr_t { addr: 0 },
        netmask: sys::esp_ip4_addr_t { addr: 0 },
        gw: sys::esp_ip4_addr_t { addr: 0 },
    };
    let err = unsafe { sys::esp_netif_get_ip_info(handle, &mut info) };
    if err == ESP_OK {
        Ok(info)
    } else {
        Err(esp_error(err))
    }
}

fn ip4_from_ipv4(ip: Ipv4Addr) -> sys::ip4_addr_t {
    let addr = if cfg!(target_endian = "little") {
        u32::from_le_bytes(ip.octets())
    } else {
        u32::from_be_bytes(ip.octets())
    };
    sys::ip4_addr_t { addr }
}

fn ip4_to_ipv4(ip: sys::ip4_addr_t) -> Ipv4Addr {
    let bytes = if cfg!(target_endian = "little") {
        ip.addr.to_le_bytes()
    } else {
        ip.addr.to_be_bytes()
    };
    Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3])
}

fn esp_ip4_to_ipv4(ip: sys::esp_ip4_addr_t) -> Ipv4Addr {
    ip4_to_ipv4(sys::ip4_addr_t { addr: ip.addr })
}

fn esp_error(err: i32) -> anyhow::Error {
    let name = unsafe { CStr::from_ptr(sys::esp_err_to_name(err)) }.to_string_lossy();
    anyhow::anyhow!("ESP error {err}: {name}")
}

fn format_mac(mac: &[u8; 6]) -> String {
    let mut out = String::with_capacity(17);
    for (idx, byte) in mac.iter().enumerate() {
        if idx > 0 {
            out.push(':');
        }
        FmtWrite::write_fmt(&mut out, format_args!("{:02X}", byte)).unwrap();
    }
    out
}

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
static STA_RECONNECT_PENDING: AtomicBool = AtomicBool::new(false);

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
    let client_ips = Arc::new(Mutex::new(HashMap::<[u8; 6], Ipv4Addr>::new()));

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

    let ap = wifi.ap_netif();

    let dhcp_manager = if DHCP_RESERVATIONS.is_empty() {
        None
    } else {
        match DhcpReservationManager::new(ap.handle()) {
            Ok(manager) => Some(Arc::new(Mutex::new(manager))),
            Err(err) => {
                warn!(
                    "Failed to initialize static DHCP reservations: {:?}. Reservations disabled.",
                    err
                );
                None
            }
        }
    };

    match dhcp_manager.as_ref() {
        Some(_) => info!(
            "Static DHCP reservations enabled for {} device(s)",
            DHCP_RESERVATIONS.len()
        ),
        None if DHCP_RESERVATIONS.is_empty() => info!("No static DHCP reservations configured"),
        None => info!(
            "Static DHCP reservations currently disabled ({} reservation(s) configured)",
            DHCP_RESERVATIONS.len()
        ),
    }

    let manager_for_wifi = dhcp_manager.as_ref().map(Arc::clone);
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
            }
            WifiEvent::StaStopped => {
                warn!("STA interface stopped – scheduling reconnect");
                STA_RECONNECT_PENDING.store(true, Ordering::SeqCst);
            }
            WifiEvent::StaBeaconTimeout => {
                warn!("STA beacon timeout – scheduling reconnect");
                STA_RECONNECT_PENDING.store(true, Ordering::SeqCst);
            }
            WifiEvent::StaConnected(details) => {
                let ssid = String::from_utf8_lossy(details.ssid());
                info!("STA connected to `{}`", ssid);
                STA_RECONNECT_PENDING.store(false, Ordering::SeqCst);
            }
            WifiEvent::ApStaConnected(conn) => {
                if let Some(manager) = manager_for_wifi.as_ref() {
                    let mac = conn.mac();
                    if let Ok(mut guard) = manager.lock() {
                        if let Err(err) = guard.handle_sta_connected(mac) {
                            warn!(
                                "Failed to schedule DHCP reservation for {}: {:?}",
                                format_mac(&mac),
                                err
                            );
                        }
                    }
                }
            }
            WifiEvent::ApStaDisconnected(disc) => {
                if let Some(manager) = manager_for_wifi.as_ref() {
                    let mac = disc.mac();
                    if let Ok(mut guard) = manager.lock() {
                        if let Err(err) = guard.handle_sta_disconnected(mac) {
                            warn!(
                                "Failed to handle disconnect for {}: {:?}",
                                format_mac(&mac),
                                err
                            );
                        }
                    }
                }
            }
            _ => {}
        })?;

    let dhcp_manager_for_ip = dhcp_manager.clone();
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

            if let Some(manager) = dhcp_manager_for_ip.as_ref() {
                if let Ok(mut guard) = manager.lock() {
                    if let Err(err) = guard.handle_ip_assigned(mac, ip) {
                        warn!(
                            "Failed to finalize DHCP reservation for {}: {:?}",
                            mac_str, err
                        );
                    }
                }
            }

            if let Ok(mut map) = client_ips_for_ip.lock() {
                map.insert(mac, ip);
            }
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

    let mut last_sta_reconnect_attempt: Option<Instant> = None;

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

        if STA_RECONNECT_PENDING.load(Ordering::SeqCst) {
            let should_attempt = match last_sta_reconnect_attempt {
                Some(last) => last.elapsed() >= Duration::from_secs(20),
                None => true,
            };

            if should_attempt {
                info!("STA reconnect timer elapsed – attempting to reconnect");
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
