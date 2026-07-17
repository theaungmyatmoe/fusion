# Changelog

All notable changes to Fusion will be documented in this file.

## [0.2.3] - 2026-07-17

### Added
- **CAPTCHA-Proof Web Search on Termux/Android:** Added native integration with the Python `duckduckgo-search` package inside the local search scraper to bypass DuckDuckGo's automated traffic blocks.
- **Hosted API Search Fallback:** Configured an automatic fallback path to the hosted cloud Responses API if local scraper requests are blocked or fail.
- **Search Debug Instrumentation:** Added logging of stderr and stdout outputs from Python/curl to `/sdcard/Android/data/com.termux/files/` to simplify future network and scraping troubleshooting.
- **Auto-dependency Installer:** Updated the `install.sh` script on Termux to automatically verify, install, and upgrade missing dependencies, including `python`, `curl`, `ca-certificates`, `pip`, and the `duckduckgo-search` library.

### Changed
- Bushed Cargo.toml versions to release `0.2.3`.
- Reorganized compilation scripts and verified CI/CD workflows for `main` pushes and `release` tags.
