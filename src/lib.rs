use anyhow::Result;
use core::time::Duration;
use esp_idf_hal::{
    gpio::OutputPin,
    peripheral::Peripheral,
    rmt::{config::TransmitConfig, FixedLengthSignal, PinState, Pulse, RmtChannel, TxRmtDriver},
};
use std::cmp::Ordering as CmpOrdering;
use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::net::Ipv4Addr;

pub use rgb::RGB8;

// Export client module for Wi-Fi station functionality
pub mod client;

pub struct WS2812RMT<'a> {
    tx_rtm_driver: TxRmtDriver<'a>,
}

impl<'d> WS2812RMT<'d> {
    // Rust ESP Board gpio2, ESP32-C3-DevKitC-02 gpio8
    pub fn new(
        led: impl Peripheral<P = impl OutputPin> + 'd,
        channel: impl Peripheral<P = impl RmtChannel> + 'd,
    ) -> Result<Self> {
        let config = TransmitConfig::new().clock_divider(2);
        let tx = TxRmtDriver::new(channel, led, &config)?;
        Ok(Self { tx_rtm_driver: tx })
    }

    pub fn set_pixel(&mut self, rgb: RGB8) -> Result<()> {
        let color: u32 = ((rgb.g as u32) << 16) | ((rgb.r as u32) << 8) | rgb.b as u32;
        let ticks_hz = self.tx_rtm_driver.counter_clock()?;
        let t0h = Pulse::new_with_duration(ticks_hz, PinState::High, &ns(350))?;
        let t0l = Pulse::new_with_duration(ticks_hz, PinState::Low, &ns(800))?;
        let t1h = Pulse::new_with_duration(ticks_hz, PinState::High, &ns(700))?;
        let t1l = Pulse::new_with_duration(ticks_hz, PinState::Low, &ns(600))?;
        let mut signal = FixedLengthSignal::<24>::new();
        for i in (0..24).rev() {
            let p = 2_u32.pow(i);
            let bit = p & color != 0;
            let (high_pulse, low_pulse) = if bit { (t1h, t1l) } else { (t0h, t0l) };
            signal.set(23 - i as usize, &(high_pulse, low_pulse))?;
        }
        self.tx_rtm_driver.start_blocking(&signal)?;

        Ok(())
    }
}

fn ns(nanos: u64) -> Duration {
    Duration::from_nanos(nanos)
}

pub type RssiDbm = i8;

pub fn format_mac(mac: &[u8; 6]) -> String {
    let mut out = String::with_capacity(17);
    for (idx, byte) in mac.iter().enumerate() {
        if idx > 0 {
            out.push(':');
        }
        FmtWrite::write_fmt(&mut out, format_args!("{:02X}", byte)).unwrap();
    }
    out
}

#[derive(Clone, Copy)]
pub struct RssiRange {
    pub min: RssiDbm,
    pub max: RssiDbm,
}

impl RssiRange {
    pub fn new(value: RssiDbm) -> Self {
        Self {
            min: value,
            max: value,
        }
    }

    pub fn update(&mut self, sample: RssiDbm) {
        self.min = self.min.min(sample);
        self.max = self.max.max(sample);
    }
}

pub struct StaSnapshot {
    pub mac: [u8; 6],
    pub name: String,
    pub rssi: RssiDbm,
    pub distance: f32,
    pub ip: Option<Ipv4Addr>,
}

pub fn render_sta_table(
    snapshots: &[StaSnapshot],
    stats: &HashMap<[u8; 6], RssiRange>,
) -> Option<String> {
    if snapshots.is_empty() {
        return None;
    }

    let mut rows: Vec<(&StaSnapshot, RssiRange)> = snapshots
        .iter()
        .map(|snap| {
            let range = stats
                .get(&snap.mac)
                .copied()
                .unwrap_or_else(|| RssiRange::new(snap.rssi));
            (snap, range)
        })
        .collect();

    rows.sort_by(|(a, _), (b, _)| {
        a.distance
            .partial_cmp(&b.distance)
            .unwrap_or(CmpOrdering::Equal)
    });

    struct TableRow {
        name: String,
        mac: String,
        rssi_range: String,
        distance: String,
        ip: String,
    }

    let mut ip_counts: HashMap<Option<Ipv4Addr>, usize> = HashMap::new();
    for (snap, _) in &rows {
        *ip_counts.entry(snap.ip).or_insert(0) += 1;
    }

    let table_rows: Vec<TableRow> = rows
        .into_iter()
        .map(|(snap, range)| TableRow {
            name: snap.name.clone(),
            mac: format_mac(&snap.mac),
            rssi_range: range.min.to_string(),
            distance: format!("{:.1}", snap.distance),
            ip: snap.ip.map_or_else(
                || "-".into(),
                |ip| {
                    let mut display = ip.to_string();
                    if ip_counts.get(&Some(ip)).copied().unwrap_or(0) > 1 {
                        display.push_str(" *");
                    }
                    display
                },
            ),
        })
        .collect();

    let headers = ["Client", "MAC", "RSSI(dBm)", "Dist(m)", "IP"];

    let client_width = table_rows
        .iter()
        .map(|row| row.name.len())
        .max()
        .unwrap_or(0)
        .max(headers[0].len());
    let mac_width = table_rows
        .iter()
        .map(|row| row.mac.len())
        .max()
        .unwrap_or(0)
        .max(headers[1].len());
    let rssi_width = table_rows
        .iter()
        .map(|row| row.rssi_range.len())
        .max()
        .unwrap_or(0)
        .max(headers[2].len());
    let dist_width = table_rows
        .iter()
        .map(|row| row.distance.len())
        .max()
        .unwrap_or(0)
        .max(headers[3].len());
    let ip_width = table_rows
        .iter()
        .map(|row| row.ip.len())
        .max()
        .unwrap_or(0)
        .max(headers[4].len());

    let widths = [client_width, mac_width, rssi_width, dist_width, ip_width];

    fn border(left: char, sep: char, right: char, widths: &[usize]) -> String {
        let mut line = String::new();
        line.push(left);
        for (idx, width) in widths.iter().enumerate() {
            let segment: String = std::iter::repeat('─').take(*width + 2).collect();
            line.push_str(&segment);
            if idx == widths.len() - 1 {
                line.push(right);
            } else {
                line.push(sep);
            }
        }
        line
    }

    fn format_row(values: &[(String, usize)]) -> String {
        let mut line = String::new();
        line.push('│');
        for (value, width) in values {
            line.push(' ');
            line.push_str(&format!("{:<width$}", value, width = *width));
            line.push(' ');
            line.push('│');
        }
        line
    }

    let mut table = String::new();
    table.push_str(&border('┌', '┬', '┐', &widths));
    table.push('\n');
    table.push_str(&format_row(&[
        (headers[0].to_string(), client_width),
        (headers[1].to_string(), mac_width),
        (headers[2].to_string(), rssi_width),
        (headers[3].to_string(), dist_width),
        (headers[4].to_string(), ip_width),
    ]));
    table.push('\n');
    table.push_str(&border('├', '┼', '┤', &widths));

    for row in table_rows {
        table.push('\n');
        table.push_str(&format_row(&[
            (row.name, client_width),
            (row.mac, mac_width),
            (row.rssi_range, rssi_width),
            (row.distance, dist_width),
            (row.ip, ip_width),
        ]));
    }

    table.push('\n');
    table.push_str(&border('└', '┴', '┘', &widths));

    Some(table)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_sta_table_formats_expected_output() {
        let snapshots = vec![
            StaSnapshot {
                mac: [0x10, 0x20, 0xBA, 0x46, 0x48, 0x38],
                name: "messy-eggnog".into(),
                rssi: -88,
                distance: 25.1,
                ip: Some(Ipv4Addr::new(192, 168, 71, 244)),
            },
            StaSnapshot {
                mac: [0x10, 0x20, 0xBA, 0x46, 0x4E, 0x8C],
                name: "alluring-glass".into(),
                rssi: -88,
                distance: 25.1,
                ip: Some(Ipv4Addr::new(192, 168, 71, 242)),
            },
            StaSnapshot {
                mac: [0x30, 0xED, 0xA0, 0xAE, 0x0E, 0xB0],
                name: "unbiased-place".into(),
                rssi: -93,
                distance: 36.9,
                ip: None,
            },
        ];

        let stats = HashMap::from([
            ([0x10, 0x20, 0xBA, 0x46, 0x48, 0x38], RssiRange::new(-88)),
            ([0x10, 0x20, 0xBA, 0x46, 0x4E, 0x8C], RssiRange::new(-88)),
            ([0x30, 0xED, 0xA0, 0xAE, 0x0E, 0xB0], RssiRange::new(-93)),
        ]);

        let rendered = render_sta_table(&snapshots, &stats).expect("table");
        let expected = [
            "┌────────────────┬───────────────────┬───────────┬─────────┬────────────────┐",
            "│ Client         │ MAC               │ RSSI(dBm) │ Dist(m) │ IP             │",
            "├────────────────┼───────────────────┼───────────┼─────────┼────────────────┤",
            "│ messy-eggnog   │ 10:20:BA:46:48:38 │ -88       │ 25.1    │ 192.168.71.244 │",
            "│ alluring-glass │ 10:20:BA:46:4E:8C │ -88       │ 25.1    │ 192.168.71.242 │",
            "│ unbiased-place │ 30:ED:A0:AE:0E:B0 │ -93       │ 36.9    │ -              │",
            "└────────────────┴───────────────────┴───────────┴─────────┴────────────────┘",
        ]
        .join("\n");

        assert_eq!(rendered, expected);
    }
}
