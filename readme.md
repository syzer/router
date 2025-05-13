cargo build --release --target riscv32imac-esp-espidf

espflash flash target/riscv32imac-esp-espidf/release/esp-wifi-ap --port /dev/tty.usbmodem101 --monitor
