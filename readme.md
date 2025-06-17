# WAT 
When you have an ESP but want Router
Station(Sta) + Access Point(Ap) in mixed mode with NAT 
# How
```bash
cp .env.example .env
```

install ESP_IDF (google it!)
run `export.bash` or `export.fish` from ESP_IDF
get tag 5.2.2
It has to say sth like:
```bash
idf.py --version
ESP-IDF v5.2.3-dirty
```

## run
```bash
cargo install
just run
```

cargo run --bin client


## or manually
```bash
cargo build --release --target riscv32imac-esp-espidf


ls -lt /dev/tty.usb* /dev/cu.usb* 2>/dev/null

crw-rw-rw-  1 root  wheel  0x9000009 May 14 17:37 /dev/cu.usbmodem101
crw-rw-rw-  1 root  wheel  0x9000008 May 14 17:37 /dev/tty.usbmodem101
# if its not usbmodem then you connected via UART

espflash flash target/riscv32imac-esp-espidf/release/esp-wifi-ap --port /dev/tty.usbmodem101 --monitor
```
