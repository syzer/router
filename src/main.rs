use esp_idf_svc::hal::modem::Modem;
use esp_idf_svc::wifi::*;
use esp_idf_svc::nvs::*;
use log::info;
use esp_idf_svc::log::EspLogger;
use esp_idf_hal::{
    peripherals::Peripherals,
    prelude::*,
    rmt::{config::TransmitConfig, TxRmtDriver},
};
use smart_leds::{SmartLedsWrite, RGB8, colors::BLACK};     // RGB8 is here
use ws2812_esp32_rmt_driver::LedPixelEsp32Rmt;              // new alias

const LED_GPIO: i32 = 8;                 // on-board LED data pin

const AP_SSID: &str = env!("AP_SSID");
const AP_PASS: &str = env!("AP_PASS");

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();


    let p = Peripherals::take()?;

    // Give the channel object & the GPIO pin directly:
    let mut ws = LedPixelEsp32Rmt::new(
        p.rmt.channel0,      // implements `Peripheral<P = CHANNEL0>`
        p.pins.gpio8         // data pin on DevKit-C6
    ).unwrap();              // <- Result, unwrap once

    let on  = [RGB8::new(0, 0, 50)];   // dim blue
    let off = [BLACK];                 // LED off

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

    sysloop.subscribe::<WifiEvent<'static>, _>(move |ev| {
        match ev {
            WifiEvent::ApStaConnected { .. }   =>
                { ws.write(on.iter().copied()).ok(); }
            WifiEvent::ApStaDisconnected { .. }=>
                { ws.write(off.iter().copied()).ok(); }
            _ => {}
        }
    })?;;

    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}