# Justfile ── run `just run` or `just run -- --release`

# Regex that strips ANSI colour codes
COLOR_RE := '\x1B\[[0-9;]*[mGK]'

# Default recipe
run *args:
    # Show coloured output in the terminal,
    # copy a colour-stripped log to the clipboard
    unbuffer cargo run {{args}} 2>&1 \
      | tee /dev/tty \
      | sed -E 's/${COLOR_RE}//g' \
      | pbcopy

where_my_esp_at:
  ls -lt /dev/tty.usb* /dev/cu.usb* 2>/dev/null  | awk '{print $NF}'