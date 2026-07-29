# CodexPeek – Codex Usage Monitor for Windows

**Languages:** [English (default)](README.md) · [한국어](docs/translations/README.ko.md) · [Español](docs/translations/README.es.md) · [Português (Brasil)](docs/translations/README.pt-BR.md) · [Bahasa Indonesia](docs/translations/README.id.md) · [日本語](docs/translations/README.ja.md) · [हिन्दी](docs/translations/README.hi.md) · [Deutsch](docs/translations/README.de.md) · [Français](docs/translations/README.fr.md) · [Tiếng Việt](docs/translations/README.vi.md) · [Türkçe](docs/translations/README.tr.md) · [العربية](docs/translations/README.ar.md)

Codex Usage Monitor is a small native Windows widget for checking your Codex usage at a glance.
It shows the primary and secondary rate-limit windows in the taskbar, a floating widget, and the system tray.

![Codex Usage Monitor taskbar widget](docs/images/taskbar-widget-en.png)

## Highlights

- Shows primary and secondary Codex usage windows, including reset times.
- Uses the installed Codex CLI's `app-server` interface instead of parsing authentication files.
- Lets you manually choose among as many as eight isolated usage profiles.
- Supports showing the widget on every taskbar or only on the primary monitor.
- Falls back safely to a floating widget and tray icon when taskbar attachment is unavailable.
- Supports manual refresh, automatic refresh intervals, Windows startup, diagnostics, and localized UI.

## How it works

The monitor starts `codex app-server --stdio` as a local child process and exchanges JSONL messages over standard input and output.
The installed Codex CLI handles its own authentication and may contact OpenAI under its existing configuration and network policy.

The monitor requests only the signed-in state and usage windows needed for display.
It does not start a Codex task or call `codex exec`.

## Usage profiles

The non-removable **Default Codex account** system profile uses the Codex home inherited when
CodexPeek starts, or the CLI default when `CODEX_HOME` is not set. You can add managed
profiles, each with a separate Codex home under
`%APPDATA%\CodexPeek\profiles`. The limit is eight profiles in total, including
the system profile.

Profile labels are names you provide. CodexPeek does not inspect account email addresses
or IDs, so confirm the intended ChatGPT account in the browser when adding or signing in
again. Selecting a profile changes only the usage that CodexPeek polls and displays. It
does not change sign-in for terminals, IDEs, the Codex app, WSL, Remote SSH, or Dev
Containers.

Selection is always manual. CodexPeek does not rotate profiles automatically, select one
from its remaining limit, or route Codex work through a profile. Deleting a managed
profile permanently removes its local profile data, including the separate CLI
credentials stored there; check the confirmation carefully.

See [Account and credential storage](docs/ACCOUNT_STORAGE.md) for the exact on-disk layout,
legacy-path migration rules, deletion behavior, and security limitations.

## Requirements

- Windows 10 or Windows 11, x64.
- A signed-in [Codex CLI](https://github.com/openai/codex) with support for `account/read` and `account/rateLimits/read`.

## Download and run

First verify that Codex CLI is installed and signed in:

```powershell
codex --version
codex login status
```

### Installer (recommended)

1. Download `CodexPeek-Setup-v<version>-x64.exe` from the
   [latest GitHub Release](https://github.com/lch5518/CodexPeek/releases/latest).
2. Run setup and follow the prompts. Administrator access is not required.
3. Start **Codex Usage Monitor** from the Start Menu.

### Portable

1. Download `codex-peek-v<version>-windows-x86_64-portable.zip` from the
   latest release.
2. Extract the ZIP completely to a writable folder.
3. Run `codex-peek.exe` from the extracted folder.

### Build from source

This option requires Rust 1.85 or later, Visual Studio 2022 C++ Build Tools, and a
Windows SDK. It runs the app from the cloned repository and does not create a Start
Menu shortcut or an uninstaller.

```powershell
git clone https://github.com/lch5518/CodexPeek.git
Set-Location .\CodexPeek
cargo build --release
.\target\release\codex-peek.exe
```

To check the build and Codex CLI connection without opening the UI:

```powershell
.\target\release\codex-peek.exe --diagnose
```

### Ask Codex to install it

Copy the prompt below into Codex. It prefers the verified Installer and falls back to a
source build only when compatible Release assets are unavailable.

```text
Install CodexPeek on this Windows x64 computer and complete the verification for me.

1. Confirm that this is Windows x64, then run `codex --version` and `codex login status`.
2. Use only the official repository and its Releases:
   https://github.com/lch5518/CodexPeek
3. Prefer the latest `CodexPeek-Setup-v<version>-x64.exe`. Download it together with
   `SHA256SUMS.txt`, find the exact Installer entry in that file, calculate the
   Installer's SHA-256, and continue only if the hashes match. Do not disable security
   controls or run a file whose checksum is missing or different.
4. Install it for the current user without requesting administrator access. Preserve
   existing CodexPeek settings and do not stop a running app or unrelated process;
   tell me if I need to close the app myself.
5. Only if compatible Release assets are unavailable, clone the official repository
   into a new user-writable directory and run `cargo build --release`. If Git, Rust
   1.85+, Visual Studio 2022 C++ Build Tools, or a Windows SDK must be installed, first
   explain exactly what will change and ask for my approval.
6. Never read or print the contents of `%USERPROFILE%\.codex\auth.json`. Authentication
   must be handled only through the installed Codex CLI.
7. After installation or build, run the resulting `codex-peek.exe --diagnose`. If it
   succeeds, launch CodexPeek.
8. Report the selected installation method, installed version, executable location,
   checksum result, and diagnostic result. If anything fails, stop safely and explain
   the exact blocker without exposing sensitive information.
```

The Installer and Portable editions use `%APPDATA%\CodexPeek\settings.json`, so
settings are shared if you switch between them. The installer adds a Start Menu shortcut
but does not enable Windows startup by default.

If the new data root does not exist, CodexPeek moves an existing
`%APPDATA%\CodexUsageMonitor` directory to `%APPDATA%\CodexPeek` without opening or copying
the profile authentication files. If both roots already exist, the new root wins and no
automatic merge is attempted.

Initial releases are not code-signed and may trigger Microsoft Defender SmartScreen.
Download only from the official release and verify the file against `SHA256SUMS.txt`.

See the [detailed installation guide (Korean)](docs/INSTALL.md) for hash verification,
updates, uninstall behavior, diagnostics, and troubleshooting.

## Using the monitor

Use the tray menu to refresh usage, choose a 1/5/10/15/30-minute refresh interval, and show or hide the widget.
It also provides Windows startup, startup view, authentication refresh, automatic authentication refresh, language, and diagnostics settings.
Choose **Widget: all monitors** or **Widget: primary monitor only** to control multi-monitor placement; the selection is remembered across restarts.

By default, the UI language follows the Windows locale when it matches a supported language. You can also choose a language manually from the tray menu. Supported languages are Korean, English, Spanish, Brazilian Portuguese, Indonesian, Japanese, Hindi, German, French, Vietnamese, Turkish, and Arabic.

The taskbar widget uses the Windows light/dark system theme for its text and lets the native taskbar material show through its background.

Only one usage request runs at a time. Failed requests retry with increasing delays while the last successful values remain visible.

If the taskbar widget cannot be attached after an Explorer restart or taskbar layout change, the tray icon remains available and the monitor retries safely.

## Privacy and security

The monitor never reads or parses the contents of `%USERPROFILE%\.codex\auth.json`.
Diagnostics check only whether that path exists.

Raw RPC responses are processed only long enough to extract the login type and the displayed rate-limit fields.
Tokens, account IDs, email addresses, authentication-file contents, and proxy values are not stored or written to logs.

CodexPeek never reads, parses, or copies any profile's `auth.json`. For a managed profile,
only the corresponding child `codex app-server` process receives its isolated
`CODEX_HOME` and the file credential-store override. Windows environment variables, the
system profile, CLI/IDE configuration, and default authentication files are not changed.
Diagnostics report aggregate profile counts and result categories only; they do not
include labels, internal profile IDs, paths, or account details.

Settings are stored in `%APPDATA%\CodexPeek\settings.json`.
A bounded diagnostic log is stored in `%TEMP%\codex-peek.log`.

For the full data-handling and vulnerability-reporting guidance, see [SECURITY.md](SECURITY.md).

## Troubleshooting

| Problem | What to do |
| --- | --- |
| Codex CLI is not found | Run `codex --version` and `where.exe codex`, then ensure Codex CLI is on `PATH`. |
| The CLI is unsupported | Update Codex CLI. Required RPC support matters more than the displayed version number. |
| Logged out or authentication expired | Complete the normal login flow in Codex CLI, then choose **Refresh authentication** in the tray menu. |
| A managed usage profile needs login | Open **Usage profiles**, choose the profile, and start login again. Confirm the intended account in the browser. Cancelling leaves the profile available for retry or explicit deletion. |
| One profile cannot refresh | Select another profile if needed. Each profile keeps independent last-good usage and retry state, so one failure does not clear the others. |
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
