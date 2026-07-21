#[cfg(windows)]
mod build_support;

#[cfg(windows)]
fn main() -> std::io::Result<()> {
    use std::{env, fs, path::PathBuf};

    use winres::{VersionInfo, WindowsResource};

    let output_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let icon_path = output_dir.join("codex-usage-monitor.ico");
    fs::write(&icon_path, build_support::usage_meter_icon())?;

    let version = env!("CARGO_PKG_VERSION");
    let packed_version = build_support::version_quad(version)
        .map_err(|message| std::io::Error::new(std::io::ErrorKind::InvalidInput, message))?;
    let mut resource = WindowsResource::new();
    resource
        .set_icon(icon_path.to_string_lossy().as_ref())
        .set("ProductName", "Codex Usage Monitor")
        .set("FileDescription", "Codex Usage Monitor")
        .set("InternalName", "codex-usage-monitor")
        .set("OriginalFilename", "codex-usage-monitor.exe")
        .set("ProductVersion", version)
        .set("FileVersion", version)
        .set_version_info(VersionInfo::PRODUCTVERSION, packed_version)
        .set_version_info(VersionInfo::FILEVERSION, packed_version)
        .set_manifest(APP_MANIFEST)
        .compile()?;
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=build_support.rs");
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=build_support.rs");
}

#[cfg(windows)]
const APP_MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity version="1.0.0.0" processorArchitecture="*" name="CodexUsageMonitor" type="win32" />
  <description>Codex Usage Monitor</description>
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*" />
    </dependentAssembly>
  </dependency>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false" />
      </requestedPrivileges>
    </security>
  </trustInfo>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2, PerMonitor</dpiAwareness>
      <longPathAware xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">true</longPathAware>
    </windowsSettings>
  </application>
</assembly>"#;
