use anyhow::Result;
use esp_idf_hal::{gpio::OutputPin, peripheral::Peripheral, rmt::RmtChannel};

use crate::{RGB8, WS2812RMT};
use log::info;

pub fn create_status_led<'d>(
    pin: impl Peripheral<P = impl OutputPin> + 'd,
    channel: impl Peripheral<P = impl RmtChannel> + 'd,
) -> Result<WS2812RMT<'d>> {
    let mut driver = WS2812RMT::new(pin, channel)?;
    let red = color_red();
    info!("Status LED -> red (r={}, g={}, b={})", red.r, red.g, red.b);
    driver.set_pixel(red)?;
    Ok(driver)
}

#[cfg(feature = "esp32s3")]
const RED_BRIGHTNESS: u8 = 255;
#[cfg(not(feature = "esp32s3"))]
const RED_BRIGHTNESS: u8 = 32;

#[cfg(feature = "esp32s3")]
const GREEN_BRIGHTNESS: u8 = 255;
#[cfg(not(feature = "esp32s3"))]
const GREEN_BRIGHTNESS: u8 = 32;

#[cfg(feature = "esp32s3")]
const PINK_BRIGHTNESS: u8 = 128;
#[cfg(not(feature = "esp32s3"))]
const PINK_BRIGHTNESS: u8 = 25;

pub fn color_red() -> RGB8 {
    RGB8::new(RED_BRIGHTNESS, 0, 0)
}

pub fn color_green() -> RGB8 {
    RGB8::new(0, GREEN_BRIGHTNESS, 0)
}

pub fn color_off() -> RGB8 {
    RGB8::new(0, 0, 0)
}

pub fn color_pink() -> RGB8 {
    RGB8::new(PINK_BRIGHTNESS, 0, PINK_BRIGHTNESS)
}
