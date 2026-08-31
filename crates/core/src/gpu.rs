//! GPU vendor detection.
//!
//! OptiScaler's own `auto` settings already adapt to the GPU at runtime (its
//! Nvidia spoofing defaults to on for AMD/Intel and off for Nvidia), so
//! nothing needs rewriting per vendor by default. Knowing the vendor still
//! matters for the UI: it decides whether the "use DLSS inputs" choice is
//! worth showing at all, and lets the app say what it found instead of asking
//! the user a quiz question the way the official setup script does.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Vendor {
    /// Priority order matters: with an iGPU + dGPU pair, the discrete card is
    /// the one games render on, and Intel iGPUs are the most common second
    /// GPU, so higher variants win when several are present.
    Intel,
    Amd,
    Nvidia,
}

impl Vendor {
    pub fn label(self) -> &'static str {
        match self {
            Vendor::Nvidia => "Nvidia",
            Vendor::Amd => "AMD",
            Vendor::Intel => "Intel",
        }
    }

    /// Whether OptiScaler needs Nvidia spoofing tricks on this GPU.
    pub fn needs_spoofing(self) -> bool {
        self != Vendor::Nvidia
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuInfo {
    pub vendor: Vendor,
    /// Marketing name when the platform exposes one.
    pub name: Option<String>,
}

impl fmt::Display for GpuInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.name {
            Some(name) => write!(f, "{name}"),
            None => write!(f, "{} GPU", self.vendor.label()),
        }
    }
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
/// Maps a PCI vendor id (as sysfs reports it, e.g. `0x10de`) to a vendor.
/// Only the Linux detector reads PCI ids.
fn vendor_from_pci_id(id: &str) -> Option<Vendor> {
    match id.trim() {
        "0x10de" => Some(Vendor::Nvidia),
        "0x1002" | "0x1022" => Some(Vendor::Amd),
        "0x8086" => Some(Vendor::Intel),
        _ => None,
    }
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
/// Classifies a display adapter's marketing name. Only the Windows detector
/// sees marketing names.
fn vendor_from_name(name: &str) -> Option<Vendor> {
    let lower = name.to_lowercase();
    if lower.contains("nvidia") || lower.contains("geforce") || lower.contains("quadro") {
        Some(Vendor::Nvidia)
    } else if lower.contains("amd") || lower.contains("radeon") || lower.contains("ati ") {
        Some(Vendor::Amd)
    } else if lower.contains("intel") || lower.contains("arc ") {
        Some(Vendor::Intel)
    } else {
        None
    }
}

#[cfg_attr(not(any(target_os = "windows", target_os = "linux")), allow(dead_code))]
/// The strongest candidate from a set of detected adapters.
fn pick(candidates: Vec<GpuInfo>) -> Option<GpuInfo> {
    candidates.into_iter().max_by_key(|gpu| gpu.vendor)
}

/// Detects the GPU games will render on. `None` when nothing recognizable is
/// found, in which case the UI stays quiet rather than guessing.
pub fn detect() -> Option<GpuInfo> {
    #[cfg(target_os = "windows")]
    {
        detect_windows()
    }
    #[cfg(target_os = "linux")]
    {
        detect_linux()
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        None
    }
}

/// Windows: enumerate display adapters from the driver registry, the same
/// data Device Manager shows. The presence of the Nvidia driver's
/// `nvapi64.dll` in system32 is used as a corroborating signal, exactly like
/// OptiScaler's own setup script.
#[cfg(target_os = "windows")]
fn detect_windows() -> Option<GpuInfo> {
    const DISPLAY_CLASS: &str =
        r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";

    let mut candidates = Vec::new();
    if let Ok(class) = windows_registry::LOCAL_MACHINE.open(DISPLAY_CLASS) {
        for index in 0..16u32 {
            let Ok(adapter) = class.open(format!("{index:04}")) else {
                continue;
            };
            let Ok(name) = adapter.get_string("DriverDesc") else {
                continue;
            };
            if let Some(vendor) = vendor_from_name(&name) {
                candidates.push(GpuInfo {
                    vendor,
                    name: Some(name),
                });
            }
        }
    }

    if candidates.is_empty() {
        // Driver files as a fallback signal.
        let system32 = std::env::var_os("WINDIR").map(std::path::PathBuf::from)?;
        if system32.join("system32/nvapi64.dll").is_file() {
            return Some(GpuInfo {
                vendor: Vendor::Nvidia,
                name: None,
            });
        }
        return None;
    }
    pick(candidates)
}

/// Linux: read PCI vendor ids straight from sysfs, no external tools needed.
#[cfg(target_os = "linux")]
fn detect_linux() -> Option<GpuInfo> {
    let mut candidates = Vec::new();
    let entries = std::fs::read_dir("/sys/class/drm").ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // card0, card1, ... — skip the card0-DP-1 style connector entries.
        if !name.starts_with("card") || name.contains('-') {
            continue;
        }
        let vendor_path = entry.path().join("device/vendor");
        let Ok(id) = std::fs::read_to_string(vendor_path) else {
            continue;
        };
        if let Some(vendor) = vendor_from_pci_id(&id) {
            candidates.push(GpuInfo { vendor, name: None });
        }
    }
    pick(candidates)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_marketing_names() {
        assert_eq!(
            vendor_from_name("NVIDIA GeForce RTX 4070"),
            Some(Vendor::Nvidia)
        );
        assert_eq!(vendor_from_name("AMD Radeon RX 7800 XT"), Some(Vendor::Amd));
        assert_eq!(
            vendor_from_name("Intel(R) Arc(TM) B580"),
            Some(Vendor::Intel)
        );
        assert_eq!(vendor_from_name("Microsoft Basic Display Adapter"), None);
    }

    #[test]
    fn classifies_pci_ids() {
        assert_eq!(vendor_from_pci_id("0x10de\n"), Some(Vendor::Nvidia));
        assert_eq!(vendor_from_pci_id("0x1002"), Some(Vendor::Amd));
        assert_eq!(vendor_from_pci_id("0x8086"), Some(Vendor::Intel));
        assert_eq!(vendor_from_pci_id("0x1234"), None);
    }

    #[test]
    fn discrete_gpu_beats_the_igpu() {
        let picked = pick(vec![
            GpuInfo {
                vendor: Vendor::Intel,
                name: Some("Intel UHD Graphics".into()),
            },
            GpuInfo {
                vendor: Vendor::Amd,
                name: Some("AMD Radeon RX 9070".into()),
            },
        ])
        .unwrap();
        assert_eq!(picked.vendor, Vendor::Amd);
    }

    #[test]
    fn only_nvidia_skips_spoofing() {
        assert!(!Vendor::Nvidia.needs_spoofing());
        assert!(Vendor::Amd.needs_spoofing());
        assert!(Vendor::Intel.needs_spoofing());
    }
}
