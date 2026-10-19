use ash::vk;
use gbtc_core::BackendError;

#[derive(Clone)]
pub struct DeviceInfo {
    pub name: String,
    pub vendor_id: u32,
    pub device_id: u32,
    pub device_type: vk::PhysicalDeviceType,
    pub api_version: u32,
    pub driver_version: u32,
    pub drm_render_major: Option<i64>,
    pub drm_render_minor: Option<i64>,
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_compute_work_group_invocations: u32,
    pub max_compute_work_group_size: [u32; 3],
    pub max_compute_work_group_count: [u32; 3],
    pub max_storage_buffer_range: u64,
    pub max_push_constants_size: u32,
    pub min_storage_buffer_offset_alignment: u64,
    pub non_coherent_atom_size: u64,
}

pub fn is_cpu_device(info: &DeviceInfo) -> bool {
    if info.device_type == vk::PhysicalDeviceType::CPU {
        return true;
    }
    let lower = info.name.to_lowercase();
    lower.contains("lavapipe")
        || lower.contains("llvmpipe")
        || lower.contains("swrast")
        || lower.contains("software")
}

pub fn enumerate(
    _entry: &ash::Entry,
    instance: &ash::Instance,
) -> Result<Vec<(vk::PhysicalDevice, DeviceInfo, Limits)>, BackendError> {
    let physicals = unsafe {
        instance
            .enumerate_physical_devices()
            .map_err(|e| BackendError::driver_failure(format!("enumerate physical devices: {e}")))?
    };
    let mut out = Vec::new();
    for pd in physicals {
        let mut props2 = vk::PhysicalDeviceProperties2::default();
        let mut drm = vk::PhysicalDeviceDrmPropertiesEXT::default();
        props2.p_next =
            &mut drm as *mut vk::PhysicalDeviceDrmPropertiesEXT as *mut std::os::raw::c_void;
        unsafe { instance.get_physical_device_properties2(pd, &mut props2) };
        let props = props2.properties;

        let name = props
            .device_name
            .iter()
            .take_while(|c| **c != 0)
            .map(|c| *c as u8 as char)
            .collect::<String>();
        let (drm_render_major, drm_render_minor) = if drm.has_render != 0 {
            (Some(drm.render_major), Some(drm.render_minor))
        } else {
            (None, None)
        };
        let info = DeviceInfo {
            name,
            vendor_id: props.vendor_id,
            device_id: props.device_id,
            device_type: props.device_type,
            api_version: props.api_version,
            driver_version: props.driver_version,
            drm_render_major,
            drm_render_minor,
        };
        let limits_raw = props.limits;
        let limits = Limits {
            max_compute_work_group_invocations: limits_raw.max_compute_work_group_invocations,
            max_compute_work_group_size: limits_raw.max_compute_work_group_size,
            max_compute_work_group_count: limits_raw.max_compute_work_group_count,
            max_storage_buffer_range: limits_raw.max_storage_buffer_range as u64,
            max_push_constants_size: limits_raw.max_push_constants_size,
            min_storage_buffer_offset_alignment: limits_raw.min_storage_buffer_offset_alignment,
            non_coherent_atom_size: limits_raw.non_coherent_atom_size,
        };
        out.push((pd, info, limits));
    }
    Ok(out)
}

pub fn find_compute_queue_family(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
) -> Option<u32> {
    let props = unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
    for (i, qf) in props.iter().enumerate() {
        if qf.queue_flags.contains(vk::QueueFlags::COMPUTE) {
            return Some(i as u32);
        }
    }
    None
}

pub fn validate_limits(limits: &Limits) -> Result<(), BackendError> {
    if limits.max_compute_work_group_invocations < 64 {
        return Err(BackendError::unavailable(format!(
            "maxComputeWorkGroupInvocations {} < 64",
            limits.max_compute_work_group_invocations
        )));
    }
    if limits.max_compute_work_group_size[0] < 64 {
        return Err(BackendError::unavailable(format!(
            "maxComputeWorkGroupSize[0] {} < 64",
            limits.max_compute_work_group_size[0]
        )));
    }
    if limits.max_compute_work_group_count[0] < 1 {
        return Err(BackendError::unavailable(
            "maxComputeWorkGroupCount[0] < 1".to_string(),
        ));
    }
    if limits.max_storage_buffer_range < (1 << 20) {
        return Err(BackendError::unavailable(format!(
            "maxStorageBufferRange {} < 1 MiB",
            limits.max_storage_buffer_range
        )));
    }
    if limits.max_push_constants_size < 88 {
        return Err(BackendError::unavailable(format!(
            "maxPushConstantsSize {} < 88",
            limits.max_push_constants_size
        )));
    }
    Ok(())
}

pub fn select_index(
    entries: &[(vk::PhysicalDevice, DeviceInfo, Limits)],
    requested: Option<&str>,
) -> Result<usize, BackendError> {
    if entries.is_empty() {
        return Err(BackendError::unavailable("no physical devices".to_string()));
    }

    let non_cpu: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(_, (_, info, _))| !is_cpu_device(info))
        .map(|(i, _)| i)
        .collect();
    if non_cpu.is_empty() {
        return Err(BackendError::unavailable(
            "only CPU Vulkan implementations available".to_string(),
        ));
    }

    let req = requested.map(str::trim).filter(|s| !s.is_empty());

    if let Some(req) = req {
        // Resolve a reliable identity from the requested render node. Name-only
        // matching is intentionally not supported.
        let identity = resolve_requested_identity(req);
        let Some(identity) = identity else {
            return Err(BackendError::unavailable(format!(
                "cannot resolve a reliable DRM/PCI identity for requested device {:?}",
                req
            )));
        };

        let mut matched: Vec<usize> = non_cpu
            .iter()
            .copied()
            .filter(|&i| entry_matches_identity(&entries[i].1, &identity))
            .collect();

        // Integrated GPUs only: reject discrete/virtual even if explicitly named.
        matched.retain(|&i| entries[i].1.device_type == vk::PhysicalDeviceType::INTEGRATED_GPU);

        match matched.len() {
            0 => {
                return Err(BackendError::unavailable(format!(
                    "requested device {:?} not found among integrated GPUs",
                    req
                )))
            }
            1 => return Ok(matched[0]),
            _ => {
                return Err(BackendError::unavailable(format!(
                    "ambiguous requested device {:?}: {} integrated matches",
                    req,
                    matched.len()
                )))
            }
        }
    }

    // Default: integrated GPUs only, no discrete/virtual/CPU fallback.
    let integrated: Vec<usize> = non_cpu
        .iter()
        .copied()
        .filter(|&i| entries[i].1.device_type == vk::PhysicalDeviceType::INTEGRATED_GPU)
        .collect();
    match integrated.len() {
        0 => Err(BackendError::unavailable(
            "no integrated GPU Vulkan implementation available".to_string(),
        )),
        1 => Ok(integrated[0]),
        _ => Err(BackendError::unavailable(
            "multiple integrated GPUs; specify GBTC_DEVICE render node".to_string(),
        )),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DeviceIdentity {
    Drm { major: i64, minor: i64 },
    Pci { vendor: u32, device: u32 },
}

fn resolve_requested_identity(req: &str) -> Option<DeviceIdentity> {
    // Path: stat the render node for DRM major/minor; also try sysfs PCI.
    if std::path::Path::new(req).exists() {
        if let Ok(meta) = std::fs::metadata(req) {
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                let rdev = meta.rdev();
                let major = ((rdev >> 8) & 0xfff) as i64;
                let minor = ((rdev & 0xff) | ((rdev >> 12) & 0xfff00)) as i64;
                if major != 0 || minor != 0 {
                    return Some(DeviceIdentity::Drm { major, minor });
                }
            }
        }
        // sysfs PCI fallback
        let base = std::path::Path::new(req)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(req);
        if let Some((v, d)) = read_sysfs_pci(base) {
            return Some(DeviceIdentity::Pci {
                vendor: v,
                device: d,
            });
        }
        return None;
    }

    // "major:minor" form.
    if let Some((a, b)) = req.split_once(':') {
        if let (Ok(maj), Ok(min)) = (a.trim().parse::<i64>(), b.trim().parse::<i64>()) {
            return Some(DeviceIdentity::Drm {
                major: maj,
                minor: min,
            });
        }
        // vendor:device hex form (PCI identity), e.g. 8086:9a49
        if let (Ok(vv), Ok(dd)) = (
            u32::from_str_radix(a.trim().trim_start_matches("0x"), 16),
            u32::from_str_radix(b.trim().trim_start_matches("0x"), 16),
        ) {
            // Heuristic: if first component <= 0xffff and second <= 0xffff treat as PCI.
            return Some(DeviceIdentity::Pci {
                vendor: vv,
                device: dd,
            });
        }
    }

    None
}

fn read_sysfs_pci(name: &str) -> Option<(u32, u32)> {
    let vendor_path = format!("/sys/class/drm/{}/device/vendor", name);
    let device_path = format!("/sys/class/drm/{}/device/device", name);
    let vendor = std::fs::read_to_string(&vendor_path).ok()?;
    let device = std::fs::read_to_string(&device_path).ok()?;
    let v = u32::from_str_radix(vendor.trim().trim_start_matches("0x"), 16).ok()?;
    let d = u32::from_str_radix(device.trim().trim_start_matches("0x"), 16).ok()?;
    Some((v, d))
}

fn entry_matches_identity(info: &DeviceInfo, identity: &DeviceIdentity) -> bool {
    match identity {
        DeviceIdentity::Drm { major, minor } => {
            info.drm_render_major == Some(*major) && info.drm_render_minor == Some(*minor)
        }
        DeviceIdentity::Pci { vendor, device } => {
            info.vendor_id == *vendor && info.device_id == *device
        }
    }
}
