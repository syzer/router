fn main() {
    let _ = dotenvy::from_filename(".env");

    println!("cargo:rerun-if-changed=.env");

    for key in ["AP_SSID", "AP_PASS"] {
        if let Ok(val) = std::env::var(key) {
            println!("cargo:rustc-env={key}={val}");
        }
    }

    embuild::espidf::sysenv::output();
}
