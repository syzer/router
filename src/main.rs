use ::log::info;
use std::sync::{Arc, Mutex};
use esp_idf_svc::hal::modem::Modem;
use esp_idf_svc::wifi::*;
use esp_idf_svc::nvs::*;
use heapless::String as HeapString;
use esp_idf_svc::handle::RawHandle;
use esp_idf_sys as sys;
use sys::esp_netif_napt_enable;
use esp_idf_svc::netif::EspNetif;
use esp_idf_svc::hal::{
    gpio::{InterruptType, PinDriver, Pull},
    peripherals::Peripherals,
    task::notification::Notification,
};
use std::num::NonZeroU32;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_wifi_ap::{WS2812RMT, RGB8};  // RGB8 came from the `rgb` crate
use esp_idf_svc::wifi::WifiEvent;
use core::sync::atomic::{AtomicBool, Ordering};
use std::thread;

static CLIENT_CONNECTED: AtomicBool = AtomicBool::new(false);

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

    let led = Arc::new(Mutex::new(
        WS2812RMT::new(
            peripherals.pins.gpio8,      // ESP32‑C6 built‑in RGB LED
            peripherals.rmt.channel0,    // any free TX channel
        )?
    ));

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
        channel: 11, // or 6
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

    wifi.set_configuration(&Configuration::Mixed(sta_cfg.clone(), ap_cfg.clone()))?;
    wifi.start()?;
    wifi.connect()?;

    // keep subscription alive
    let _wifi_subscription = sysloop.subscribe::<WifiEvent, _>(move |event: WifiEvent| {
        println!("Wifi event: {:?}", event);
        if let WifiEvent::ApStaConnected(_) = event {
            println!("Client connected, blinking LED");
            CLIENT_CONNECTED.store(true, Ordering::SeqCst);
        }
    })?;

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

    // Spawn a dedicated task that blinks pink whenever CLIENT_CONNECTED is set
    let led_task = led.clone();
    thread::Builder::new()
        .name("client_blink".into())
        .stack_size(2048)
        .spawn(move || {
            loop {
                if CLIENT_CONNECTED.swap(false, Ordering::SeqCst) {
                    let mut led = led_task.lock().unwrap();
                    for _ in 0..5 {
                        let _ = led.set_pixel(RGB8::new(0, 0, 0));     // off
                        FreeRtos::delay_ms(200);
                        let _ = led.set_pixel(RGB8::new(25, 0, 25)); // pink
                        FreeRtos::delay_ms(200);
                    }
                } else {
                    FreeRtos::delay_ms(50);
                }
            }
        })?;

    loop {
        button.enable_interrupt()?;
        // Option 1
        // notification.wait(esp_idf_svc::hal::delay::BLOCK);        
        // button.disable_interrupt()?;       // disarm
        // {
        //     let mut led_guard = led.lock().unwrap();
        //     led_guard.set_pixel(RGB8::new(32, 0, 0))?;
        // }
        // reconnect_sta(&mut wifi);
        // FreeRtos::delay_ms(5_000);
        // {
        //     let mut led_guard = led.lock().unwrap();
        //     led_guard.set_pixel(RGB8::new(0, 32, 0))?;
        // }
        // Option 1 end 

        println!(".");
        // Option 2
        if notification.wait(50).is_some() {
            button.disable_interrupt()?;
            {
                let mut led_guard = led.lock().unwrap();
                led_guard.set_pixel(RGB8::new(32, 0, 0))?;
            }
            reconnect_sta(&mut wifi, &sta_cfg, &ap_cfg);

            FreeRtos::delay_ms(5_000);
            {
                let mut led_guard = led.lock().unwrap();
                led_guard.set_pixel(RGB8::new(0, 32, 0))?;
            }
        } else {
            button.disable_interrupt()?;
        }
    }

}

pub fn enable_nat(ap_netif_handle: &EspNetif) -> anyhow::Result<()> {
    info!("Attempting to enable NAPT on netif handle: {:?}", ap_netif_handle.handle());
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
fn reconnect_sta(wifi: &mut EspWifi<'_>, sta_cfg: &ClientConfiguration, ap_cfg: &AccessPointConfiguration) {
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