# Fusion

**Fusion** is a terminal-first AI coding agent — **made by Fusion AI**.

| Desktop | Mobile (Termux) |
|:---:|:---:|
| ![Fusion TUI](docs/screenshot.png) | ![Fusion on Termux](docs/screenshot_mobile.png) |


## Install

```bash
curl -sSL https://raw.githubusercontent.com/theaungmyatmoe/fusion/main/scripts/install.sh | sh
```


## Usage

```bash
fusion login              # authenticate
fusion                    # interactive TUI
fusion --minimal          # phone-friendly / scrollback mode
fusion -p "fix the bug"   # headless one-shot
fusion --yolo -p "…"      # auto-approve tools
```

> For configuration, build instructions, architecture details, and the full CLI reference see **[docs/DETAILS.md](docs/DETAILS.md)**.


## License

MIT OR Apache-2.0
