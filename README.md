# Codex Usage Monitor

[한국어 버전](README.ko.md)

Codex Usage Monitor is a small native Windows widget for checking your Codex usage at a glance.
It shows the primary and secondary rate-limit windows in the taskbar, a floating widget, and the system tray.

![Codex Usage Monitor taskbar widget](docs/images/taskbar-widget.png)

## Highlights

- Shows primary and secondary Codex usage windows, including reset times.
- Uses the installed Codex CLI's `app-server` interface instead of parsing authentication files.
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
- For source builds: Rust 1.85 or later, Visual Studio 2022 C++ Build Tools, and a Windows SDK.

## Build and run

There is no installer or WinGet package yet. Build the application from source after installing and signing in to Codex CLI.

```powershell
git clone https://github.com/lch5518/CodexPeek.git
Set-Location .\CodexPeek
cargo build --release

Start-Process .\target\release\codex-usage-monitor.exe
```

Run the following command to check the CLI, app-server connection, and local settings without opening the UI:

```powershell
.\target\release\codex-usage-monitor.exe --diagnose
```

`--startup` is intended only for the Windows startup registration created through the tray menu.

## Using the monitor

Use the tray menu to refresh usage, choose a 1/5/10/15/30-minute refresh interval, and show or hide the widget.
It also provides Windows startup, startup view, authentication refresh, automatic authentication refresh, language, and diagnostics settings.

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
| The taskbar widget is missing | Use the floating widget or tray icon, restart Explorer if needed, and try the display mode again. |
| More detail is needed | Run `--diagnose` or open **Diagnostics** from the tray menu. |

## Development

Run these checks before sharing a source build:

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo build --release
```

Automated checks do not replace the Windows, DPI, multi-monitor, and Explorer recovery scenarios in the [release checklist](docs/RELEASE_CHECKLIST.md).

## ❤️ Support

If CodexPeek saves you time, consider supporting its development.

- ⭐ Star this repository
- ❤️ Sponsor on GitHub

Every sponsorship helps keep the project actively maintained.

## License

This project is available under the [MIT License](LICENSE).
See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for third-party notices.
