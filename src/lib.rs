// #![no_std]
//
// use esp_hal_smartled::{smartLedBuffer, SmartLedsAdapter};
// use smart_leds::{hsv::{Hsv, hsv2rgb}, brightness, gamma, SmartLedsWrite};
// use esp_hal::rmt::{TxChannel, TxChannelCreator};
// use esp_hal::gpio::OutputPin;
// use esp_hal::peripheral::Peripheral;
//
// /// Single‐LED WS2812 driver that steps its hue on each call.
// pub struct Led<TX>
// where
//     TX: TxChannel,
// {
//     adapter: SmartLedsAdapter<TX, { smartLedBuffer!(1).len() }>,
//     hue: u8,
// }
//
// impl<TX> Led<TX>
// where
//     TX: TxChannel,
// {
//     /// Create a new WS2812 driver for exactly 1 LED.
//     ///
//     /// `channel` is your `rmt.channel0` (a ChannelCreator),
//     /// `pin` is the GPIO pin (e.g. `per.GPIO8`).
//     pub fn new<C, O>(channel: C, pin: O) -> Self
//     where
//         C: TxChannelCreator<'static, TX, O>,
//         O: OutputPin + Peripheral<P = O> + 'static,
//     {
//         let buffer = smartLedBuffer!(1);
//         let adapter = SmartLedsAdapter::new(channel, pin, buffer);
//         Self { adapter, hue: 0 }
//     }
//
//     /// Advance the hue, compute the next RGB value, and write it
//     /// at `bright/255` brightness.
//     pub fn random_color(&mut self, bright: u8) -> Result<(), ()> {
//         let hsv = Hsv {
//             hue: self.hue,
//             sat: 255,
//             val: 255,
//         };
//         let rgb = hsv2rgb(hsv);
//         self.hue = self.hue.wrapping_add(10);
//
//         self.adapter
//             .write(brightness(gamma([rgb].iter().cloned()), bright))
//             .map_err(|_| ())
//     }
// }
