use super::config::Gpu;
use anyhow::Result;
use std::path::Path;

/// Windows stores a per-program GPU choice (Settings > Graphics) in the registry:
/// HKCU\Software\Microsoft\DirectX\UserGpuPreferences, value name = exe path,
/// data "GpuPreference=1;" (integrated / power saving) or "GpuPreference=2;" (discrete).
/// Only matters on PCs with two GPUs.
#[cfg(windows)]
pub fn apply(exe: &Path, gpu: Gpu) -> Result<()> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    if !exe.is_absolute() {
        return Ok(());
    }
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu.create_subkey(r"Software\Microsoft\DirectX\UserGpuPreferences")?;
    let name = exe.to_string_lossy().to_string();
    match gpu {
        Gpu::Auto => {
            let _ = key.delete_value(&name);
        }
        Gpu::Integrated => key.set_value(&name, &"GpuPreference=1;")?,
        Gpu::Discrete => key.set_value(&name, &"GpuPreference=2;")?,
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn apply(_exe: &Path, _gpu: Gpu) -> Result<()> {
    Ok(())
}
