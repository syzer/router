use ::log::info;
use esp_idf_svc::hal::modem::Modem;
use esp_idf_svc::wifi::*;
use esp_idf_svc::nvs::*;
use heapless::String as HeapString;
use embedded_svc::wifi::*;
use esp_idf_svc::*;
use esp_idf_svc::handle::RawHandle;
use esp_idf_sys as sys;
use sys::{esp, esp_netif_napt_enable};
use esp_idf_svc::netif::EspNetif;

const AP_SSID: &str = env!("AP_SSID");
const AP_PASS: &str = env!("AP_PASS");

const ST_SSID: &str = env!("ST_SSID");
const ST_PASS: &str = env!("ST_PASS");

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    info!(".....Booting up Wi-Fi AP + STA bridge........");

    let modem   = unsafe { Modem::new() };
    let sysloop = esp_idf_svc::eventloop::EspSystemEventLoop::take()?;
    let nvs     = EspDefaultNvsPartition::take()?;
    let mut wifi = EspWifi::new(modem, sysloop.clone(), Some(nvs))?;

    let mut ap_ssid = heapless::String::<32>::new();
    ap_ssid.push_str(AP_SSID).expect("SSID too long");

    let mut ap_pass = heapless::String::<64>::new();
    ap_pass.push_str(AP_PASS).expect("Password too long");

    let ap_cfg =  AccessPointConfiguration {
        ssid: ap_ssid,
        password: ap_pass,
        channel: 6,
        auth_method: AuthMethod::WPA2Personal,
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

    wifi.set_configuration(&Configuration::Mixed(sta_cfg.clone(), ap_cfg))?;
    wifi.start()?;
    wifi.connect()?;

    info!("RustyAP up → SSID `{}`  pass `{}`", AP_SSID, AP_PASS);
    info!("Connecting STA to `{}` …", ST_SSID);

    info!(
        "Access point started! SSID: {}, password: {}",
        AP_SSID,
        AP_PASS
    );

    // #[cfg(feature = "experimental")]
    // {
    //     let sta = wifi.sta_netif();
    //     let ap  = wifi.ap_netif();
    //
    //     let _nat = EspNetifNat::new(sta, ap)?;
    //     info!("NAT enabled: AP clients can now reach the Internet");
    // }


    unsafe fn enable_nat(ap: &esp_idf_svc::netif::EspNetif) -> anyhow::Result<()> {
        esp!(esp_netif_napt_enable(ap.handle()))?;
        Ok(())
    }

    let ap  = wifi.ap_netif();
    unsafe { enable_nat(&ap)?; }
    info!("NAPT enabled – AP clients have Internet!");

    loop {
        info!(".");
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}