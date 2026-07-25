# Codex Usage Monitor

[한국어 버전](README.ko.md)

Codex Usage Monitor is a small native Windows widget for checking your Codex usage at a glance.
It shows the primary and secondary rate-limit windows in the taskbar, a floating widget, and the system tray.

![Codex Usage Monitor taskbar widget](docs/images/taskbar-widget.png)

## Highlights

- Shows primary and secondary Codex usage windows, including reset times.
- Uses the installed Codex CLI's `app-server` interface instead of parsing authentication files.
- Supports showing the widget on every taskbar or only on the primary monitor.
- Falls back safely to a floating widget and tray icon when taskbar attachment is unavailable.
- Supports manual refresh, automatic refresh intervals, Windows startup, diagnostics, and Korean or English UI.

## How it works

The monitor starts `codex app-server --stdio` as a local child process and exchanges JSONL messages over standard input and output.
The installed Codex CLI handles its own authentication and may contact OpenAI under its existing configuration and network policy.

The monitor requests only the signed-in state and usage windows needed for display.
It does not start a Codex task or call `codex exec`.

## Requirements

- Windows 10 or Windows 11, x64.
- A signed-in [Codex CLI](https://github.com/openai/codex) with support for `account/read` and `account/rateLimits/read`.

## Download and run

Download the latest files from [GitHub Releases](https://github.com/lch5518/CodexPeek/releases/latest).
The release provides two Windows x64 options:

- **Installer (recommended):** Download `CodexUsageMonitor-Setup-v<version>-x64.exe`.
  It installs for the current user without administrator access, adds a Start Menu shortcut,
  and offers to launch the monitor when setup finishes. It does not enable Windows startup
  automatically.
- **Portable:** Download `codex-usage-monitor-v<version>-windows-x86_64-portable.zip`,
  extract it to a writable folder, and run `codex-usage-monitor.exe`. Nothing is installed.

Both editions use `%APPDATA%\CodexUsageMonitor\settings.json`, so settings are shared if
you switch between them. Uninstalling preserves settings and diagnostics but removes the
monitor's Windows autostart registration.

The initial releases are not code-signed, so Microsoft Defender SmartScreen may show an
unrecognized-app warning. Download only from this repository and compare the file with the
release's `SHA256SUMS.txt`:

```powershell
$file = ".\CodexUsageMonitor-Setup-v0.1.2-x64.exe"
(Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant()
Get-Content .\SHA256SUMS.txt
```

Run the following command to check the CLI, app-server connection, and local settings without opening the UI:

```powershell
& "$env:LOCALAPPDATA\Programs\CodexUsageMonitor\codex-usage-monitor.exe" --diagnose
```

For the portable edition, run the same `--diagnose` option from its extracted directory.
`--startup` is intended only for the Windows startup registration created through the tray menu.

## Using the monitor

Use the tray menu to refresh usage, choose a 1/5/10/15/30-minute refresh interval, and show or hide the widget.
It also provides Windows startup, startup view, authentication refresh, automatic authentication refresh, language, and diagnostics settings.
Choose **Widget: all monitors** or **Widget: primary monitor only** to control multi-monitor placement; the selection is remembered across restarts.

The taskbar widget uses the Windows light/dark system theme for its text and lets the native taskbar material show through its background.

Only one usage request runs at a time. Failed requests retry with increasing delays while the last successful values remain visible.

If the taskbar widget cannot be attached after an Explorer restart or taskbar layout change, the tray icon remains available and the monitor retries safely.

## Privacy and security

The monitor never reads or parses the contents of `%USERPROFILE%\.codex\auth.json`.
Diagnostics check only whether that path exists.

Raw RPC responses are processed only long enough to extract the login type and the displayed rate-limit fields.
Tokens, account IDs, email addresses, authentication-file contents, and proxy values are not stored or written to logs.

Settings are stored in `%APPDATA%\CodexUsageMonitor\settings.json`.
A bounded diagnostic log is stored in `%TEMP%\codex-usage-monitor.log`.

For the full data-handling and vulnerability-reporting guidance, see [SECURITY.md](SECURITY.md).

## Troubleshooting

| Problem | What to do |
| --- | --- |
| Codex CLI is not found | Run `codex --version` and `where.exe codex`, then ensure Codex CLI is on `PATH`. |
| The CLI is unsupported | Update Codex CLI. Required RPC support matters more than the displayed version number. |
| Logged out or authentication expired | Complete the normal login flow in Codex CLI, then choose **Refresh authentication** in the tray menu. |
| The taskbar widget is on the wrong monitor | Choose **Widget: all monitors** or **Widget: primary monitor only** from the tray menu. |
| The taskbar widget is missing | Use the floating widget or tray icon, restart Explorer if needed, and select the preferred widget monitor mode. |
| More detail is needed | Run `--diagnose` or open **Diagnostics** from the tray menu. |

## Development

Source builds require Rust 1.85 or later, Visual Studio 2022 C++ Build Tools, and a
Windows SDK. Build and validate from the repository root:

```powershell
git clone https://github.com/lch5518/CodexPeek.git
Set-Location .\CodexPeek
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo build --release
```

Automated checks do not replace the Windows, DPI, multi-monitor, and Explorer recovery scenarios in the [release checklist](docs/RELEASE_CHECKLIST.md).

## ❤️ Support

If CodexPeek saves you time, consider supporting its development.

- ⭐ Star this repository
- ❤️ [Sponsor on GitHub](https://github.com/sponsors/lch5518)

Every sponsorship helps keep the project actively maintained.

## License

This project is available under the [MIT License](LICENSE).
See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for third-party notices.
