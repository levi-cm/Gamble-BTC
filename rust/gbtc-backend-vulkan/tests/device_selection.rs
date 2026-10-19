use ash::vk;
use gbtc_backend_vulkan::device::{is_cpu_device, select_index, DeviceInfo, Limits};

fn integrated_entry(
    name: &str,
    vendor: u32,
    device: u32,
    drm_major: Option<i64>,
    drm_minor: Option<i64>,
) -> (vk::PhysicalDevice, DeviceInfo, Limits) {
    (
        vk::PhysicalDevice::null(),
        DeviceInfo {
            name: name.to_string(),
            vendor_id: vendor,
            device_id: device,
            device_type: vk::PhysicalDeviceType::INTEGRATED_GPU,
            api_version: vk::make_api_version(0, 1, 1, 0),
            driver_version: 1,
            drm_render_major: drm_major,
            drm_render_minor: drm_minor,
        },
        Limits {
            max_compute_work_group_invocations: 256,
            max_compute_work_group_size: [256, 256, 64],
            max_compute_work_group_count: [65535, 65535, 65535],
            max_storage_buffer_range: 1 << 20,
            max_push_constants_size: 128,
            min_storage_buffer_offset_alignment: 4,
            non_coherent_atom_size: 64,
        },
    )
}

fn discrete_entry(name: &str) -> (vk::PhysicalDevice, DeviceInfo, Limits) {
    let mut e = integrated_entry(name, 0x8086, 0x9a49, None, None);
    e.1.device_type = vk::PhysicalDeviceType::DISCRETE_GPU;
    e
}

fn cpu_entry(name: &str) -> (vk::PhysicalDevice, DeviceInfo, Limits) {
    let mut e = integrated_entry(name, 0x1234, 0x5678, None, None);
    e.1.device_type = vk::PhysicalDeviceType::CPU;
    e
}

fn stat_render_node() -> (i64, i64) {
    let meta = std::fs::metadata("/dev/dri/renderD128").expect("render node present");
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let rdev = meta.rdev();
        let major = ((rdev >> 8) & 0xfff) as i64;
        let minor = ((rdev & 0xff) | ((rdev >> 12) & 0xfff00)) as i64;
        (major, minor)
    }
    #[cfg(not(unix))]
    {
        (226, 128)
    }
}

#[test]
fn cpu_device_detected() {
    let e = cpu_entry("lavapipe");
    assert!(is_cpu_device(&e.1));
}

#[test]
fn selects_matching_render_node() {
    let (maj, min) = stat_render_node();
    let wrong = integrated_entry("Iris", 0x8086, 0x9a49, Some(maj), Some(min + 1));
    let right = integrated_entry("Iris", 0x8086, 0x9a49, Some(maj), Some(min));
    let entries = vec![wrong, right];
    let idx = select_index(&entries, Some("/dev/dri/renderD128")).unwrap();
    assert_eq!(idx, 1);
}

#[test]
fn same_name_ambiguous_without_identity() {
    let a = integrated_entry("Iris Xe", 0x8086, 0x9a49, None, None);
    let b = integrated_entry("Iris Xe", 0x8086, 0x9a49, None, None);
    let entries = vec![a, b];
    assert!(select_index(&entries, None).is_err());
}

#[test]
fn wrong_render_node_rejected() {
    let (maj, min) = stat_render_node();
    let e = integrated_entry("Iris", 0x8086, 0x9a49, Some(maj), Some(min));
    let entries = vec![e];
    assert!(select_index(&entries, Some("/dev/dri/renderD999999")).is_err());
}

#[test]
fn cpu_and_discrete_rejected() {
    let entries = vec![cpu_entry("cpu"), discrete_entry("disc")];
    assert!(select_index(&entries, None).is_err());
}

#[test]
fn valid_default_single_integrated() {
    let (maj, min) = stat_render_node();
    let e = integrated_entry("Iris", 0x8086, 0x9a49, Some(maj), Some(min));
    let entries = vec![e];
    let idx = select_index(&entries, Some("/dev/dri/renderD128")).unwrap();
    assert_eq!(idx, 0);
}

#[test]
fn name_only_request_rejected() {
    let (maj, min) = stat_render_node();
    let e = integrated_entry("Iris Xe", 0x8086, 0x9a49, Some(maj), Some(min));
    let entries = vec![e];
    assert!(select_index(&entries, Some("Iris Xe")).is_err());
}

#[test]
fn major_minor_string_matches() {
    let (maj, min) = stat_render_node();
    let e = integrated_entry("Iris", 0x8086, 0x9a49, Some(maj), Some(min));
    let entries = vec![e];
    let req = format!("{}:{}", maj, min);
    let idx = select_index(&entries, Some(&req)).unwrap();
    assert_eq!(idx, 0);
}
