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

    for key in ["ST_SSID", "ST_PASS"] {
        if let Ok(val) = std::env::var(key) {
            println!("cargo:rustc-env={key}={val}");
        }
    }

    // Generate device names for MAC address mapping
    generate_device_names();

    embuild::espidf::sysenv::output();
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
