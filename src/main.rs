use esp_idf_svc::hal::prelude::*;
use esp_idf_svc::hal::modem::Modem;
use esp_idf_svc::wifi::*;
use esp_idf_svc::nvs::*;
use embedded_svc::wifi::*;
use log::info;
use heapless::String as HeapString;

const AP_SSID: &str = env!("AP_SSID");
const AP_PASS: &str = env!("AP_PASS");

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    info!("Booting up Wi-Fi AP example...");

    let modem = unsafe { Modem::new() };

    let sysloop = esp_idf_svc::eventloop::EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    let mut wifi = EspWifi::new(modem, sysloop.clone(), Some(nvs))?;

    let mut ssid = heapless::String::<32>::new();
    ssid.push_str(AP_SSID).expect("SSID too long");

    let mut password = heapless::String::<64>::new();
    password.push_str(AP_PASS).expect("Password too long");

    let ap_config = Configuration::AccessPoint(AccessPointConfiguration {
        ssid,
        password,
        channel: 6,
        auth_method: AuthMethod::WPA2Personal,
        ..Default::default()
    });

    wifi.set_configuration(&ap_config)?;
    wifi.start()?;

    info!(
        "Access point started! SSID: {}, password: {}",
        AP_SSID,
        AP_PASS
    );

    loop {
        std::thread::sleep(std::time::Duration::from_secs(5));
    }
}