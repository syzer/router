# Justfile ── run `just run` or `just run -- --release`

# Regex that strips ANSI colour codes
#COLOR_RE := '\x1B\[[0-9;]*[mGK]'
COLOR_RE := '\x1B\[([0-9]{1,3}(;[0-9]{1,3})*)?[mGK]'

# Default recipe
run *args:
    # Show coloured output in the terminal,
    # copy a colour-stripped log to the clipboard
    unbuffer cargo run {{args}} 2>&1 \
      | tee /dev/tty \
      | sed -r 's/${COLOR_RE}//g' \
      | pbcopy

where_my_esp_at:
  ls -lt /dev/tty.usb* /dev/cu.usb* 2>/dev/null  | awk '{print $NF}'

build:
  cargo build --release --target riscv32imac-esp-espidf

flash:
  espflash flash --monitor --chip esp32c6 target/riscv32imac-esp-espidf/release/esp-wifi-ap