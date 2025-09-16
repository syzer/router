# Justfile ── run `just run`, `just run -- --release`, or `just release`

# Regex that strips ANSI colour codes
COLOR_RE := '\x1B\[([0-9]{1,3}(;[0-9]{1,3})*)?[mGK]'

build *args:
  MCU=esp32c6 cargo build --release --target riscv32imac-esp-espidf {{args}}

build-c3 *args:
  MCU=esp32c3 cargo build --release --target riscv32imc-esp-espidf --features esp32c3 {{args}}

flash:
  espflash flash --monitor --chip esp32c6 target/riscv32imac-esp-espidf/release/esp-wifi-ap

flash-c3:
  espflash flash --monitor --chip esp32c3 target/riscv32imc-esp-espidf/release/esp-wifi-ap

# Default recipe (ESP32-C6)
run *args:
    # Show coloured output in the terminal,
    # copy a colour-stripped log to the clipboard
    unbuffer cargo run --bin esp-wifi-ap {{args}} 2>&1 \
      | tee /dev/tty \
      | sed -r 's/${COLOR_RE}//g' \
      | pbcopy

# Release (ESP32-C6)
release *args:
    # Show coloured output in the terminal,
    # copy a colour-stripped log to the clipboard
    cargo run --release --bin esp-wifi-ap {{args}}

# Run with ESP32-C3 
run-c3 *args:
    # Show coloured output in the terminal,
    # copy a colour-stripped log to the clipboard
    unbuffer env MCU=esp32c3 cargo run --bin esp-wifi-ap --target riscv32imc-esp-espidf --features esp32c3 {{args}} 2>&1 \
      | tee /dev/tty \
      | sed -r 's/${COLOR_RE}//g' \
      | pbcopy

# Release with ESP32-C3 
release-c3 *args:
    # Show coloured output in the terminal,
    # copy a colour-stripped log to the clipboard
    env MCU=esp32c3 cargo run --release --bin esp-wifi-ap --target riscv32imc-esp-espidf --features esp32c3 {{args}}

# Run client (ESP32-C6)
run-client *args:
    # Show coloured output in the terminal,
    # copy a colour-stripped log to the clipboard
    unbuffer cargo run --bin esp-wifi-client {{args}} 2>&1 \
      | tee /dev/tty \
      | sed -r 's/${COLOR_RE}//g' \
      | pbcopy

# Run client with ESP32-C3 
run-client-c3 *args:
    # Show coloured output in the terminal,
    # copy a colour-stripped log to the clipboard
    unbuffer env MCU=esp32c3 cargo run --bin esp-wifi-client --target riscv32imc-esp-espidf --features esp32c3 {{args}} 2>&1 \
      | tee /dev/tty \
      | sed -r 's/${COLOR_RE}//g' \
      | pbcopy

where_my_esp_at:
  ls -lt /dev/tty.usb* /dev/cu.usb* 2>/dev/null  | awk '{print $NF}'
