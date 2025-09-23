use std::env;
use std::fs::File;
use std::io::Write;
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

    // Generate device names for MAC address mapping
    generate_device_names();

    // Generate MAC hostname mappings
    generate_mac_hostname_mappings();

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

fn generate_mac_hostname_mappings() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("mac_hostname_mappings.rs");
    let mut f = File::create(&dest_path).unwrap();

    // Read MAC hostname mappings from environment variables
    let mut mac_mappings = Vec::new();

    // Support MAC_HOSTNAME_1, MAC_HOSTNAME_2, etc.
    for i in 1..=20 {
        let key = format!("MAC_HOSTNAME_{}", i);
        if let Ok(mapping) = std::env::var(&key) {
            // Format: "aa:bb:cc:dd:ee:ff:hostname"
            let parts: Vec<&str> = mapping.split(':').collect();
            if parts.len() == 7 {
                // 6 MAC parts + 1 hostname
                let mac_parts: Result<Vec<u8>, _> = parts[0..6]
                    .iter()
                    .map(|s| u8::from_str_radix(s, 16))
                    .collect();

                if let Ok(mac_bytes) = mac_parts {
                    if mac_bytes.len() == 6 {
                        mac_mappings.push((mac_bytes, parts[6].to_string()));
                    }
                }
            }
        }
    }

    // Also support single MAC_HOSTNAMES variable with comma-separated values
    if let Ok(mappings_str) = std::env::var("MAC_HOSTNAMES") {
        for entry in mappings_str.split(',') {
            let entry = entry.trim();
            if entry.is_empty() {
                continue;
            }

            let parts: Vec<&str> = entry.split(':').collect();
            if parts.len() == 7 {
                let mac_parts: Result<Vec<u8>, _> = parts[0..6]
                    .iter()
                    .map(|s| u8::from_str_radix(s, 16))
                    .collect();

                if let Ok(mac_bytes) = mac_parts {
                    if mac_bytes.len() == 6 {
                        mac_mappings.push((mac_bytes, parts[6].to_string()));
                    }
                }
            }
        }
    }

    writeln!(f, "// Auto-generated MAC hostname mappings").unwrap();
    writeln!(f, "").unwrap();

    writeln!(
        f,
        "pub fn get_static_mac_mappings() -> std::collections::HashMap<[u8; 6], String> {{"
    )
    .unwrap();
    writeln!(f, "    #[allow(unused_mut)]").unwrap();
    writeln!(
        f,
        "    let mut mappings = std::collections::HashMap::new();"
    )
    .unwrap();

    for (mac_bytes, hostname) in &mac_mappings {
        writeln!(f, "    mappings.insert([0x{:02x}, 0x{:02x}, 0x{:02x}, 0x{:02x}, 0x{:02x}, 0x{:02x}], \"{}\".to_string());",
                mac_bytes[0], mac_bytes[1], mac_bytes[2], mac_bytes[3], mac_bytes[4], mac_bytes[5], hostname).unwrap();
    }

    writeln!(f, "    mappings").unwrap();
    writeln!(f, "}}").unwrap();
    writeln!(f, "").unwrap();

    writeln!(f, "pub fn get_mac_mapping_count() -> usize {{").unwrap();
    writeln!(f, "    {}", mac_mappings.len()).unwrap();
    writeln!(f, "}}").unwrap();

    println!("cargo:rerun-if-changed=build.rs");
}
