#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    if let Err(error) = codex_usage_monitor::app::run(std::env::args_os().skip(1)) {
        eprintln!("codex-usage-monitor: {error}");
        std::process::exit(1);
    }
}
