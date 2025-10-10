use std::collections::HashSet;
use std::env;
use std::fs::File;
use std::io::Write;
use std::net::Ipv4Addr;
use std::path::Path;

fn main() {
    let _ = dotenvy::from_filename(".env");

    println!("cargo:rerun-if-changed=.env");

    for key in ["AP_SSID", "AP_PASS"] {
        if let Ok(val) = std::env::var(key) {
            println!("cargo:rustc-env={key}={val}");
        }
    }

    for key in ["AP_SSID", "AP_PASS"] {
        if let Ok(val) = std::env::var(key) {
            println!("cargo:rustc-env={key}={val}");
        }
    }

    // Handle multiple Wi-Fi networks (ST_SSID_1, ST_PASS_1, etc.)
    let mut wifi_networks = Vec::new();
    for i in 1..=10 {
        // Support up to 10 networks
        let ssid_key = format!("ST_SSID_{}", i);
        let pass_key = format!("ST_PASS_{}", i);

        if let (Ok(ssid), Ok(pass)) = (std::env::var(&ssid_key), std::env::var(&pass_key)) {
            wifi_networks.push((ssid, pass));
            println!(
                "cargo:rustc-env={}={}",
                ssid_key,
                std::env::var(&ssid_key).unwrap()
            );
            println!(
                "cargo:rustc-env={}={}",
                pass_key,
                std::env::var(&pass_key).unwrap()
            );
        }
    }

    // Also support legacy single ST_SSID/ST_PASS for backwards compatibility
    for key in ["ST_SSID", "ST_PASS"] {
        if let Ok(val) = std::env::var(key) {
            println!("cargo:rustc-env={key}={val}");
        }
    }

    // Generate Wi-Fi networks configuration
    generate_wifi_networks(&wifi_networks);

    // Collect and generate static DHCP leases (MAC -> IP reservations)
    let static_leases = collect_static_leases();
    generate_static_leases(&static_leases);

    // Generate device names for MAC address mapping
    generate_device_names();

    embuild::espidf::sysenv::output();
}

fn generate_wifi_networks(wifi_networks: &[(String, String)]) {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("wifi_networks.rs");
    let mut f = File::create(&dest_path).unwrap();

    writeln!(f, "// Auto-generated Wi-Fi networks configuration").unwrap();
    writeln!(f, "").unwrap();

    writeln!(f, "#[derive(Debug, Clone)]").unwrap();
    writeln!(f, "pub struct WifiCredentials {{").unwrap();
    writeln!(f, "    pub ssid: &'static str,").unwrap();
    writeln!(f, "    pub password: &'static str,").unwrap();
    writeln!(f, "}}").unwrap();
    writeln!(f, "").unwrap();

    writeln!(f, "pub const WIFI_NETWORKS: &[WifiCredentials] = &[").unwrap();
    for (ssid, pass) in wifi_networks {
        writeln!(f, "    WifiCredentials {{").unwrap();
        writeln!(f, "        ssid: \"{}\",", ssid).unwrap();
        writeln!(f, "        password: \"{}\",", pass).unwrap();
        writeln!(f, "    }},").unwrap();
    }
    writeln!(f, "];").unwrap();
    writeln!(f, "").unwrap();

    writeln!(f, "pub fn get_network_count() -> usize {{").unwrap();
    writeln!(f, "    WIFI_NETWORKS.len()").unwrap();
    writeln!(f, "}}").unwrap();
    writeln!(f, "").unwrap();

    writeln!(
        f,
        "pub fn get_network(index: usize) -> Option<&'static WifiCredentials> {{"
    )
    .unwrap();
    writeln!(f, "    WIFI_NETWORKS.get(index)").unwrap();
    writeln!(f, "}}").unwrap();
    writeln!(f, "").unwrap();

    writeln!(
        f,
        "pub fn cycle_to_next_network(current_index: usize) -> usize {{"
    )
    .unwrap();
    writeln!(f, "    if WIFI_NETWORKS.is_empty() {{").unwrap();
    writeln!(f, "        0").unwrap();
    writeln!(f, "    }} else {{").unwrap();
    writeln!(f, "        (current_index + 1) % WIFI_NETWORKS.len()").unwrap();
    writeln!(f, "    }}").unwrap();
    writeln!(f, "}}").unwrap();

    println!("cargo:rerun-if-changed=build.rs");
}

fn generate_device_names() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("device_names.rs");
    let mut f = File::create(&dest_path).unwrap();

    // Generate 100 friendly device names
    let mut device_names = Vec::new();
    for _i in 0..100 {
        let name = names::Generator::default().next().unwrap();
        device_names.push(name);
    }

    writeln!(f, "// Auto-generated device names").unwrap();
    writeln!(f, "pub const DEVICE_NAMES: &[&str] = &[").unwrap();
    for name in &device_names {
        writeln!(f, "    \"{}\",", name).unwrap();
    }
    writeln!(f, "];").unwrap();

    writeln!(f, "").unwrap();
    writeln!(f, "/// Map MAC address to a friendly device name").unwrap();
    writeln!(f, "pub fn mac_to_name(mac: &[u8; 6]) -> &'static str {{").unwrap();
    writeln!(f, "    let hash = (mac[5] as usize) % DEVICE_NAMES.len();").unwrap();
    writeln!(f, "    DEVICE_NAMES[hash]").unwrap();
    writeln!(f, "}}").unwrap();

    println!("cargo:rerun-if-changed=build.rs");
}

#[derive(Debug)]
struct StaticLeaseConfig {
    mac: [u8; 6],
    ip: Ipv4Addr,
}

fn collect_static_leases() -> Vec<StaticLeaseConfig> {
    let mut leases = Vec::new();
    let mut seen_macs = HashSet::new();

    for (key, value) in env::vars() {
        let stripped = key
            .strip_prefix("DHCP_")
            .or_else(|| key.strip_prefix("MAC_"));

        let Some(raw_mac) = stripped else {
            continue;
        };

        let mac = parse_mac(raw_mac).unwrap_or_else(|err| {
            panic!(
                "Invalid MAC identifier `{}` in environment variable `{}`: {}",
                raw_mac, key, err
            )
        });

        if !seen_macs.insert(mac) {
            panic!(
                "Duplicate static DHCP reservation for MAC {:02X?}; variable `{}`",
                mac, key
            );
        }

        let ip: Ipv4Addr = value
            .trim()
            .parse()
            .unwrap_or_else(|_| panic!("Invalid IPv4 address `{}` for `{}`", value, key));

        leases.push(StaticLeaseConfig { mac, ip });
    }

    leases.sort_by(|a, b| a.mac.cmp(&b.mac));

    leases
}

fn parse_mac(input: &str) -> Result<[u8; 6], &'static str> {
    let filtered: String = input
        .chars()
        .filter(|c| !matches!(c, ':' | '-' | '_'))
        .collect();

    if filtered.len() != 12 {
        return Err("expected 12 hexadecimal characters for MAC (after removing separators)");
    }

    let mut mac = [0u8; 6];
    for (idx, chunk) in filtered.as_bytes().chunks(2).enumerate() {
        let slice = std::str::from_utf8(chunk).map_err(|_| "non-hex character in MAC")?;
        mac[idx] = u8::from_str_radix(slice, 16).map_err(|_| "invalid hex in MAC")?;
    }

    Ok(mac)
}

fn generate_static_leases(leases: &[StaticLeaseConfig]) {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("dhcp_leases.rs");
    let mut f = File::create(&dest_path).unwrap();

    writeln!(f, "// Auto-generated static DHCP reservations").unwrap();
    writeln!(f, "#[derive(Debug, Clone, Copy)]").unwrap();
    writeln!(f, "pub struct StaticLease {{").unwrap();
    writeln!(f, "    pub mac: [u8; 6],").unwrap();
    writeln!(f, "    pub ip: [u8; 4],").unwrap();
    writeln!(f, "}}").unwrap();
    writeln!(f, "").unwrap();

    writeln!(f, "pub const STATIC_LEASES: &[StaticLease] = &[").unwrap();
    for lease in leases {
        let mac_bytes: Vec<String> = lease.mac.iter().map(|b| format!("0x{:02X}", b)).collect();
        let ip_octets = lease.ip.octets();
        writeln!(f, "    StaticLease {{").unwrap();
        writeln!(f, "        mac: [{}],", mac_bytes.join(", ")).unwrap();
        writeln!(
            f,
            "        ip: [{}, {}, {}, {}],",
            ip_octets[0], ip_octets[1], ip_octets[2], ip_octets[3]
        )
        .unwrap();
        writeln!(f, "    }},").unwrap();
    }
    writeln!(f, "];").unwrap();

    println!("cargo:rerun-if-changed=build.rs");
}
