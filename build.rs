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
    for i in 1..=10 { // Support up to 10 networks
        let ssid_key = format!("ST_SSID_{}", i);
        let pass_key = format!("ST_PASS_{}", i);
        
        if let (Ok(ssid), Ok(pass)) = (std::env::var(&ssid_key), std::env::var(&pass_key)) {
            wifi_networks.push((ssid, pass));
            println!("cargo:rustc-env={}={}", ssid_key, std::env::var(&ssid_key).unwrap());
            println!("cargo:rustc-env={}={}", pass_key, std::env::var(&pass_key).unwrap());
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

    writeln!(f, "pub fn get_network(index: usize) -> Option<&'static WifiCredentials> {{").unwrap();
    writeln!(f, "    WIFI_NETWORKS.get(index)").unwrap();
    writeln!(f, "}}").unwrap();
    writeln!(f, "").unwrap();

    writeln!(f, "pub fn cycle_to_next_network(current_index: usize) -> usize {{").unwrap();
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
