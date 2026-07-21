# Release checklist / 릴리스 체크리스트

## Automated gates / 자동 검증

- [x] `cargo fmt --all -- --check`
- [x] `cargo test --all-targets`
- [x] `cargo clippy --all-targets --all-features -- -D warnings`
- [x] `cargo build --release`
- [x] `git diff --check`
- [x] Release EXE VersionInfo and embedded resource inspection
- [x] `target\release\codex-usage-monitor.exe --diagnose` smoke run

## Manual QA remaining / 남은 수동 QA

- [ ] Windows 10
- [ ] Windows 11
- [ ] 100% DPI
- [ ] 125% DPI
- [ ] 150% DPI
- [ ] 200% DPI
- [ ] Multiple monitors and mixed-DPI monitor transitions
- [ ] Explorer restart and taskbar-widget recovery
- [ ] Auto-hide taskbar
- [ ] Codex CLI missing
- [ ] Codex CLI logged out
- [ ] Proxy-present environment (without recording proxy values)
- [ ] Enable, verify, and disable Windows autostart
- [ ] Update check opens only an exact release page when repository metadata is enabled

Automated checks do not replace the unchecked interactive scenarios above. Do not publish
a release as manually qualified until each applicable item has been exercised and its
result recorded.
