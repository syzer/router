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
use esp_idf_svc::hal::{
    gpio::{InterruptType, PinDriver, Pull},
    peripherals::Peripherals,
    task::notification::Notification,
};
use smart_leds_trait::SmartLedsWrite;
use std::num::NonZeroU32;
use esp_idf_svc::hal::delay::FreeRtos;

const AP_SSID: &str = env!("AP_SSID");
const AP_PASS: &str = env!("AP_PASS");

const ST_SSID: &str = env!("ST_SSID");
const ST_PASS: &str = env!("ST_PASS");

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    // button start
    let peripherals = Peripherals::take()?;            // singleton?

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
            if let Some(val) = NonZeroU32::new(1) { // .unwrap() is fine, this is just more explicit
                notifier.notify_and_yield(val);
            }
        }) {
            Ok(_) => {
                info!("Successfully subscribed to button interrupt on GPIO {}", button.pin());
            }
            Err(e) => {
                info!("Failed to subscribe to button interrupt on GPIO {}: {:?}", button.pin(), e);
                () // javascript :D
            }
        }
    }
    // button end

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

    let ap  = wifi.ap_netif();
    enable_nat(&ap)?;
    info!("NAPT enabled – AP clients have Internet!");

    loop {
        // Arm the interrupt and wait
        button.enable_interrupt()?;
        notification.wait(esp_idf_svc::hal::delay::BLOCK);
        button.disable_interrupt()?;       // disarm

        println!(".");

        // FreeRtos::delay_ms(60000);
        FreeRtos::delay_ms(1000);
    }

    pub fn enable_nat(ap_netif_handle: &EspNetif) -> anyhow::Result<()> {
        info!("Attempting to enable NAPT on netif handle: {:?}", ap_netif_handle.handle());
        // Ensure the netif handle is valid and the interface is up.
        unsafe {
            let result = esp_netif_napt_enable(ap_netif_handle.handle());
            if result == sys::ESP_OK {
                info!("esp_netif_napt_enable call succeeded.");
                Ok(())
            } else {
                info!("esp_netif_napt_enable call failed with error code: {}", result);
                Err(anyhow::anyhow!("Failed to enable NAPT, ESP error code: {}", result))
            }
        }
    }

}