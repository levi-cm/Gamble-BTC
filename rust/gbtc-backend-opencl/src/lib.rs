#![allow(dead_code)]
#![allow(non_camel_case_types)]

use gbtc_core::backend::{BackendError, BackendInfo, BatchResult, ComputeBackend, WorkBatch};

use kernel::{Config, KernelVariant};
use libloading::{Library, Symbol};
use std::ffi::CString;
use std::os::raw::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

mod kernel;
pub use kernel::Config as OpenClConfig;

type cl_int = i32;
type cl_uint = u32;
type cl_ulong = u64;
type cl_bitfield = u64;
type cl_size_t = usize;
type cl_context_properties = isize;

type cl_platform_id = *mut c_void;
type cl_device_id = *mut c_void;
type cl_context = *mut c_void;
type cl_command_queue = *mut c_void;
type cl_mem = *mut c_void;
type cl_program = *mut c_void;
type cl_kernel = *mut c_void;
type cl_event = *mut c_void;

const CL_SUCCESS: cl_int = 0;
const CL_DEVICE_TYPE_GPU: cl_bitfield = 1 << 2;
const CL_MEM_READ_ONLY: cl_bitfield = 1 << 2;
const CL_MEM_READ_WRITE: cl_bitfield = 1 << 0;
const CL_PLATFORM_NAME: cl_uint = 0x0902;
const CL_DEVICE_NAME: cl_uint = 0x102B;
const CL_DEVICE_VENDOR: cl_uint = 0x102C;
const CL_DEVICE_VERSION: cl_uint = 0x102F;
const CL_DEVICE_EXTENSIONS: cl_uint = 0x1030;
const CL_DEVICE_TYPE: cl_uint = 0x1000;
const CL_DEVICE_HOST_UNIFIED_MEMORY: cl_uint = 0x1035;
const CL_DEVICE_PCI_BUS_INFO_KHR: cl_uint = 0x410F;
const CL_PROGRAM_BUILD_LOG: cl_uint = 0x1183;
const CL_KERNEL_WORK_GROUP_SIZE: cl_uint = 0x11B0;
const CL_EVENT_COMMAND_EXECUTION_STATUS: cl_uint = 0x11D3;
const CL_COMPLETE: cl_int = 0;
const CL_QUEUE_THROTTLE_KHR: cl_int = 0x1097;
const CL_QUEUE_THROTTLE_LOW_KHR: cl_bitfield = 1 << 2;

#[repr(C)]
#[derive(Copy, Clone, Default)]
struct ClPciBusInfo {
    pci_domain: cl_uint,
    pci_bus: cl_uint,
    pci_device: cl_uint,
    pci_function: cl_uint,
}

struct ClApi {
    _lib: Library,
    get_platform_ids: unsafe extern "C" fn(cl_uint, *mut cl_platform_id, *mut cl_uint) -> cl_int,
    get_platform_info: unsafe extern "C" fn(
        cl_platform_id,
        cl_uint,
        cl_size_t,
        *mut c_void,
        *mut cl_size_t,
    ) -> cl_int,
    get_device_ids: unsafe extern "C" fn(
        cl_platform_id,
        cl_bitfield,
        cl_uint,
        *mut cl_device_id,
        *mut cl_uint,
    ) -> cl_int,
    get_device_info: unsafe extern "C" fn(
        cl_device_id,
        cl_uint,
        cl_size_t,
        *mut c_void,
        *mut cl_size_t,
    ) -> cl_int,
    create_context: unsafe extern "C" fn(
        *const cl_context_properties,
        cl_uint,
        *const cl_device_id,
        Option<unsafe extern "C" fn(*const i8, *const c_void, cl_size_t, *mut c_void)>,
        *mut c_void,
        *mut cl_int,
    ) -> cl_context,
    create_command_queue: unsafe extern "C" fn(
        cl_context,
        cl_device_id,
        cl_bitfield,
        *mut cl_int,
    ) -> cl_command_queue,
    create_command_queue_with_properties: Option<
        unsafe extern "C" fn(
            cl_context,
            cl_device_id,
            *const isize,
            *mut cl_int,
        ) -> cl_command_queue,
    >,
    create_buffer: unsafe extern "C" fn(
        cl_context,
        cl_bitfield,
        cl_size_t,
        *mut c_void,
        *mut cl_int,
    ) -> cl_mem,
    create_program_with_source: unsafe extern "C" fn(
        cl_context,
        cl_uint,
        *const *const i8,
        *const cl_size_t,
        *mut cl_int,
    ) -> cl_program,
    build_program: unsafe extern "C" fn(
        cl_program,
        cl_uint,
        *const cl_device_id,
        *const i8,
        Option<unsafe extern "C" fn(cl_program, *mut c_void)>,
        *mut c_void,
    ) -> cl_int,
    get_program_build_info: unsafe extern "C" fn(
        cl_program,
        cl_device_id,
        cl_uint,
        cl_size_t,
        *mut c_void,
        *mut cl_size_t,
    ) -> cl_int,
    create_kernel: unsafe extern "C" fn(cl_program, *const i8, *mut cl_int) -> cl_kernel,
    set_kernel_arg: unsafe extern "C" fn(cl_kernel, cl_uint, cl_size_t, *const c_void) -> cl_int,
    enqueue_write_buffer: unsafe extern "C" fn(
        cl_command_queue,
        cl_mem,
        cl_uint,
        cl_size_t,
        cl_size_t,
        *const c_void,
        cl_uint,
        *const cl_event,
        *mut cl_event,
    ) -> cl_int,
    enqueue_nd_range_kernel: unsafe extern "C" fn(
        cl_command_queue,
        cl_kernel,
        cl_uint,
        *const cl_size_t,
        *const cl_size_t,
        *const cl_size_t,
        cl_uint,
        *const cl_event,
        *mut cl_event,
    ) -> cl_int,
    enqueue_read_buffer: unsafe extern "C" fn(
        cl_command_queue,
        cl_mem,
        cl_uint,
        cl_size_t,
        cl_size_t,
        *mut c_void,
        cl_uint,
        *const cl_event,
        *mut cl_event,
    ) -> cl_int,
    flush: Option<unsafe extern "C" fn(cl_command_queue) -> cl_int>,
    get_event_info: Option<
        unsafe extern "C" fn(cl_event, cl_uint, cl_size_t, *mut c_void, *mut cl_size_t) -> cl_int,
    >,
    release_event: Option<unsafe extern "C" fn(cl_event) -> cl_int>,
    get_kernel_work_group_info: unsafe extern "C" fn(
        cl_kernel,
        cl_device_id,
        cl_uint,
        cl_size_t,
        *mut c_void,
        *mut cl_size_t,
    ) -> cl_int,
    finish: unsafe extern "C" fn(cl_command_queue) -> cl_int,
    release_kernel: unsafe extern "C" fn(cl_kernel) -> cl_int,
    release_program: unsafe extern "C" fn(cl_program) -> cl_int,
    release_mem_object: unsafe extern "C" fn(cl_mem) -> cl_int,
    release_command_queue: unsafe extern "C" fn(cl_command_queue) -> cl_int,
    release_context: unsafe extern "C" fn(cl_context) -> cl_int,
}

impl ClApi {
    fn load() -> Result<Self, BackendError> {
        let lib = unsafe { Library::new("libOpenCL.so.1") }
            .map_err(|e| BackendError::unavailable(format!("dlopen libOpenCL.so.1 failed: {e}")))?;
        unsafe {
            macro_rules! get {
                ($name:expr, $ty:ty) => {{
                    let sym: Symbol<$ty> = lib.get($name).map_err(|e| {
                        BackendError::unavailable(format!(
                            "libOpenCL.so.1 has no {}: {e}",
                            String::from_utf8_lossy($name)
                        ))
                    })?;
                    *sym
                }};
            }
            let get_platform_ids = get!(
                b"clGetPlatformIDs",
                unsafe extern "C" fn(cl_uint, *mut cl_platform_id, *mut cl_uint) -> cl_int
            );
            let get_platform_info = get!(
                b"clGetPlatformInfo",
                unsafe extern "C" fn(
                    cl_platform_id,
                    cl_uint,
                    cl_size_t,
                    *mut c_void,
                    *mut cl_size_t,
                ) -> cl_int
            );
            let get_device_ids = get!(
                b"clGetDeviceIDs",
                unsafe extern "C" fn(
                    cl_platform_id,
                    cl_bitfield,
                    cl_uint,
                    *mut cl_device_id,
                    *mut cl_uint,
                ) -> cl_int
            );
            let get_device_info = get!(
                b"clGetDeviceInfo",
                unsafe extern "C" fn(
                    cl_device_id,
                    cl_uint,
                    cl_size_t,
                    *mut c_void,
                    *mut cl_size_t,
                ) -> cl_int
            );
            let create_context = get!(
                b"clCreateContext",
                unsafe extern "C" fn(
                    *const cl_context_properties,
                    cl_uint,
                    *const cl_device_id,
                    Option<unsafe extern "C" fn(*const i8, *const c_void, cl_size_t, *mut c_void)>,
                    *mut c_void,
                    *mut cl_int,
                ) -> cl_context
            );
            let create_command_queue = get!(
                b"clCreateCommandQueue",
                unsafe extern "C" fn(
                    cl_context,
                    cl_device_id,
                    cl_bitfield,
                    *mut cl_int,
                ) -> cl_command_queue
            );
            let create_command_queue_with_properties = lib
                .get::<unsafe extern "C" fn(
                    cl_context,
                    cl_device_id,
                    *const isize,
                    *mut cl_int,
                ) -> cl_command_queue>(b"clCreateCommandQueueWithProperties")
                .ok()
                .map(|s| *s);
            let create_buffer = get!(
                b"clCreateBuffer",
                unsafe extern "C" fn(
                    cl_context,
                    cl_bitfield,
                    cl_size_t,
                    *mut c_void,
                    *mut cl_int,
                ) -> cl_mem
            );
            let create_program_with_source = get!(
                b"clCreateProgramWithSource",
                unsafe extern "C" fn(
                    cl_context,
                    cl_uint,
                    *const *const i8,
                    *const cl_size_t,
                    *mut cl_int,
                ) -> cl_program
            );
            let build_program = get!(
                b"clBuildProgram",
                unsafe extern "C" fn(
                    cl_program,
                    cl_uint,
                    *const cl_device_id,
                    *const i8,
                    Option<unsafe extern "C" fn(cl_program, *mut c_void)>,
                    *mut c_void,
                ) -> cl_int
            );
            let get_program_build_info = get!(
                b"clGetProgramBuildInfo",
                unsafe extern "C" fn(
                    cl_program,
                    cl_device_id,
                    cl_uint,
                    cl_size_t,
                    *mut c_void,
                    *mut cl_size_t,
                ) -> cl_int
            );
            let create_kernel = get!(
                b"clCreateKernel",
                unsafe extern "C" fn(cl_program, *const i8, *mut cl_int) -> cl_kernel
            );
            let set_kernel_arg = get!(
                b"clSetKernelArg",
                unsafe extern "C" fn(cl_kernel, cl_uint, cl_size_t, *const c_void) -> cl_int
            );
            let enqueue_write_buffer = get!(
                b"clEnqueueWriteBuffer",
                unsafe extern "C" fn(
                    cl_command_queue,
                    cl_mem,
                    cl_uint,
                    cl_size_t,
                    cl_size_t,
                    *const c_void,
                    cl_uint,
                    *const cl_event,
                    *mut cl_event,
                ) -> cl_int
            );
            let enqueue_nd_range_kernel = get!(
                b"clEnqueueNDRangeKernel",
                unsafe extern "C" fn(
                    cl_command_queue,
                    cl_kernel,
                    cl_uint,
                    *const cl_size_t,
                    *const cl_size_t,
                    *const cl_size_t,
                    cl_uint,
                    *const cl_event,
                    *mut cl_event,
                ) -> cl_int
            );
            let enqueue_read_buffer = get!(
                b"clEnqueueReadBuffer",
                unsafe extern "C" fn(
                    cl_command_queue,
                    cl_mem,
                    cl_uint,
                    cl_size_t,
                    cl_size_t,
                    *mut c_void,
                    cl_uint,
                    *const cl_event,
                    *mut cl_event,
                ) -> cl_int
            );
            let flush = lib
                .get::<unsafe extern "C" fn(cl_command_queue) -> cl_int>(b"clFlush")
                .ok()
                .map(|s| *s);
            let get_event_info = lib
                .get::<unsafe extern "C" fn(
                    cl_event,
                    cl_uint,
                    cl_size_t,
                    *mut c_void,
                    *mut cl_size_t,
                ) -> cl_int>(b"clGetEventInfo")
                .ok()
                .map(|s| *s);
            let release_event = lib
                .get::<unsafe extern "C" fn(cl_event) -> cl_int>(b"clReleaseEvent")
                .ok()
                .map(|s| *s);
            let get_kernel_work_group_info = get!(
                b"clGetKernelWorkGroupInfo",
                unsafe extern "C" fn(
                    cl_kernel,
                    cl_device_id,
                    cl_uint,
                    cl_size_t,
                    *mut c_void,
                    *mut cl_size_t,
                ) -> cl_int
            );
            let finish = get!(
                b"clFinish",
                unsafe extern "C" fn(cl_command_queue) -> cl_int
            );
            let release_kernel = get!(
                b"clReleaseKernel",
                unsafe extern "C" fn(cl_kernel) -> cl_int
            );
            let release_program = get!(
                b"clReleaseProgram",
                unsafe extern "C" fn(cl_program) -> cl_int
            );
            let release_mem_object = get!(
                b"clReleaseMemObject",
                unsafe extern "C" fn(cl_mem) -> cl_int
            );
            let release_command_queue = get!(
                b"clReleaseCommandQueue",
                unsafe extern "C" fn(cl_command_queue) -> cl_int
            );
            let release_context = get!(
                b"clReleaseContext",
                unsafe extern "C" fn(cl_context) -> cl_int
            );

            if flush.is_none() || get_event_info.is_none() || release_event.is_none() {
                return Err(BackendError::unavailable(
                    "OpenCL event-poll API unavailable".to_string(),
                ));
            }

            Ok(Self {
                _lib: lib,
                get_platform_ids,
                get_platform_info,
                get_device_ids,
                get_device_info,
                create_context,
                create_command_queue,
                create_command_queue_with_properties,
                create_buffer,
                create_program_with_source,
                build_program,
                get_program_build_info,
                create_kernel,
                set_kernel_arg,
                enqueue_write_buffer,
                enqueue_nd_range_kernel,
                enqueue_read_buffer,
                flush,
                get_event_info,
                release_event,
                get_kernel_work_group_info,
                finish,
                release_kernel,
                release_program,
                release_mem_object,
                release_command_queue,
                release_context,
            })
        }
    }
}

// Device selection helpers
#[derive(Clone)]
struct DeviceCandidate {
    platform: cl_platform_id,
    device: cl_device_id,
    platform_name: String,
    device_name: String,
    device_version: String,
    extensions: String,
    unified: bool,
    pci: Option<ClPciBusInfo>,
}

fn cl_get_string(api: &ClApi, obj: *mut c_void, is_platform: bool, param: cl_uint) -> String {
    let mut n = 0usize;
    let rc = unsafe {
        if is_platform {
            (api.get_platform_info)(obj, param, 0, std::ptr::null_mut(), &mut n)
        } else {
            (api.get_device_info)(obj, param, 0, std::ptr::null_mut(), &mut n)
        }
    };
    if rc != CL_SUCCESS || n == 0 || n > 1024 * 1024 {
        return String::new();
    }
    let mut buf = vec![0u8; n];
    let rc = if is_platform {
        unsafe {
            (api.get_platform_info)(
                obj as cl_platform_id,
                param,
                buf.len(),
                buf.as_mut_ptr() as *mut c_void,
                &mut n,
            )
        }
    } else {
        unsafe {
            (api.get_device_info)(
                obj as cl_device_id,
                param,
                buf.len(),
                buf.as_mut_ptr() as *mut c_void,
                &mut n,
            )
        }
    };
    if rc != CL_SUCCESS || n == 0 {
        return String::new();
    }
    let end = n.min(buf.len());
    let slice = &buf[..end];
    let trimmed: Vec<u8> = slice.iter().copied().take_while(|&b| b != 0).collect();
    String::from_utf8_lossy(&trimmed).to_string()
}

fn is_fallback_platform(name: &str) -> bool {
    let l = name.to_lowercase();
    l.contains("rusticl") || l.contains("clover")
}

fn platform_matches_filter(name: &str, filter: Option<&str>) -> bool {
    match filter {
        None => true,
        Some("") => true,
        Some(f) => name.contains(f),
    }
}

fn resolve_pci_target(requested: &str) -> Option<(u32, u32, u32, u32)> {
    // If requested path exists, read sysfs PCI address.
    let path = std::path::Path::new(requested);
    if path.exists() {
        // canonical device symlink
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(requested);
        // try /sys/class/drm/{name}/device
        let dev_link = format!("/sys/class/drm/{}/device", name);
        if let Ok(canon) = std::fs::canonicalize(&dev_link) {
            let s = canon.to_string_lossy();
            // last component like 0000:00:02.0
            if let Some(last) = s.rsplit('/').next() {
                if let Some((domain_bus, dev_func)) = last.rsplit_once(':') {
                    // domain_bus like 0000:00? Actually format domain:bus
                    if let Some((domain, bus)) = domain_bus.split_once(':') {
                        // dev_func like 02.0
                        if let Some((dev, func)) = dev_func.split_once('.') {
                            if let (Ok(d), Ok(b), Ok(devn), Ok(f)) = (
                                u32::from_str_radix(domain, 16),
                                u32::from_str_radix(bus, 16),
                                u32::from_str_radix(dev, 16),
                                u32::from_str_radix(func, 16),
                            ) {
                                return Some((d, b, devn, f));
                            }
                        }
                    }
                }
            }
        }
        // fallback: read vendor/device? no, address only.
    }
    // major:minor form not applicable to PCI; vendor:device hex not address.
    None
}

fn enumerate_candidates(
    api: &ClApi,
    platform_filter: Option<&str>,
) -> Result<Vec<DeviceCandidate>, BackendError> {
    let mut nplatforms = 0u32;
    let rc = unsafe { (api.get_platform_ids)(0, std::ptr::null_mut(), &mut nplatforms) };
    if rc != CL_SUCCESS || nplatforms == 0 {
        return Err(BackendError::unavailable(
            "no OpenCL platforms found".to_string(),
        ));
    }
    let mut platforms = vec![std::ptr::null_mut(); nplatforms as usize];
    let rc =
        unsafe { (api.get_platform_ids)(nplatforms, platforms.as_mut_ptr(), std::ptr::null_mut()) };
    if rc != CL_SUCCESS {
        return Err(BackendError::unavailable(
            "clGetPlatformIDs failed".to_string(),
        ));
    }

    let mut out = Vec::new();
    for &plat in &platforms {
        let plat_name = cl_get_string(api, plat as *mut c_void, true, CL_PLATFORM_NAME);
        if !platform_matches_filter(&plat_name, platform_filter) {
            continue;
        }
        let mut ndevs = 0u32;
        let rc = unsafe {
            (api.get_device_ids)(
                plat,
                CL_DEVICE_TYPE_GPU,
                0,
                std::ptr::null_mut(),
                &mut ndevs,
            )
        };
        if rc != CL_SUCCESS || ndevs == 0 {
            continue;
        }
        let mut devs = vec![std::ptr::null_mut(); ndevs as usize];
        let rc = unsafe {
            (api.get_device_ids)(
                plat,
                CL_DEVICE_TYPE_GPU,
                ndevs,
                devs.as_mut_ptr(),
                std::ptr::null_mut(),
            )
        };
        if rc != CL_SUCCESS {
            continue;
        }
        for &dev in &devs {
            // unified memory
            let mut unified: cl_uint = 0;
            let mut sz = 0usize;
            let rc = unsafe {
                (api.get_device_info)(
                    dev,
                    CL_DEVICE_HOST_UNIFIED_MEMORY,
                    std::mem::size_of::<cl_uint>(),
                    &mut unified as *mut _ as *mut c_void,
                    &mut sz,
                )
            };
            if rc != CL_SUCCESS {
                continue;
            }
            if unified == 0 {
                continue; // discrete/non-unified
            }
            let dev_name = cl_get_string(api, dev as *mut c_void, false, CL_DEVICE_NAME);
            let dev_version = format!(
                "{} / driver {}",
                cl_get_string(api, dev as *mut c_void, false, CL_DEVICE_VERSION),
                cl_get_string(api, dev as *mut c_void, false, 0x102d)
            );
            let extensions = cl_get_string(api, dev as *mut c_void, false, CL_DEVICE_EXTENSIONS);
            // pci bus info if extension present
            let pci = if extensions.contains("cl_khr_pci_bus_info") {
                let mut info = ClPciBusInfo::default();
                let mut sz2 = 0usize;
                let rc = unsafe {
                    (api.get_device_info)(
                        dev,
                        CL_DEVICE_PCI_BUS_INFO_KHR,
                        std::mem::size_of::<ClPciBusInfo>(),
                        &mut info as *mut _ as *mut c_void,
                        &mut sz2,
                    )
                };
                if rc == CL_SUCCESS && sz2 >= std::mem::size_of::<ClPciBusInfo>() {
                    Some(info)
                } else {
                    None
                }
            } else {
                None
            };

            out.push(DeviceCandidate {
                platform: plat,
                device: dev,
                platform_name: plat_name.clone(),
                device_name: dev_name,
                device_version: dev_version,
                extensions,
                unified: true,
                pci,
            });
        }
    }

    if out.is_empty() {
        return Err(BackendError::unavailable(
            "no unified-memory OpenCL GPU device found".to_string(),
        ));
    }
    Ok(out)
}

pub struct OpenClBackend {
    api: Arc<ClApi>,
    platform: cl_platform_id,
    device: cl_device_id,
    context: cl_context,
    queue: cl_command_queue,
    program: cl_program,
    kernel: cl_kernel,
    buf_in: cl_mem,
    buf_out: cl_mem,

    pub info_str: String,
    platform_name: String,
    device_name: String,
    device_version: String,
    config: Config,
    local_size: u32,
    kernel_name: String,
    poll_us: u32,

    stop: Option<Arc<AtomicBool>>,
    poisoned: bool,
    pending: Option<Box<HostTransfer>>,
}

struct HostTransfer {
    input: [u32; 24],
    zero: u32,
    output: [u32; 16],
}

impl OpenClBackend {
    pub fn new(requested: Option<&str>) -> Result<Self, BackendError> {
        // config from env
        let mut cfg = Config::default();
        config_from_env(&mut cfg)?;

        let api = Arc::new(ClApi::load()?);
        let src = kernel::build_kernel_src(&cfg)?;
        let src_cstr = CString::new(src)
            .map_err(|_| BackendError::driver_failure("kernel source contains NUL"))?;

        // platform filter
        let platform_filter = std::env::var("GBTC_OPENCL_PLATFORM")
            .ok()
            .filter(|s| !s.is_empty());

        // device requested: argument overrides env
        let device_req = requested
            .map(|s| s.to_string())
            .or_else(|| std::env::var("GBTC_DEVICE").ok())
            .unwrap_or_else(|| "/dev/dri/renderD128".to_string());
        let device_req = if device_req.trim().is_empty() {
            "/dev/dri/renderD128".to_string()
        } else {
            device_req
        };

        let candidates = enumerate_candidates(&api, platform_filter.as_deref())?;

        // prefer non-fallback platforms if no explicit platform filter
        let preferred: Vec<&DeviceCandidate> = if platform_filter.is_none() {
            let nonfallback: Vec<&DeviceCandidate> = candidates
                .iter()
                .filter(|c| !is_fallback_platform(&c.platform_name))
                .collect();
            if !nonfallback.is_empty() {
                nonfallback
            } else {
                candidates.iter().collect()
            }
        } else {
            candidates.iter().collect()
        };

        // filter by requested device
        let pci_target = resolve_pci_target(&device_req);
        let matched: Vec<&DeviceCandidate> = if let Some(t) = pci_target {
            preferred
                .iter()
                .copied()
                .filter(|c| {
                    c.pci
                        .map(|p| (p.pci_domain, p.pci_bus, p.pci_device, p.pci_function) == t)
                        .unwrap_or(false)
                })
                .collect()
        } else {
            // maybe requested is platform/device name substring
            preferred
                .iter()
                .copied()
                .filter(|c| {
                    c.device_name.contains(&device_req) || c.platform_name.contains(&device_req)
                })
                .collect()
        };

        if matched.is_empty() {
            return Err(BackendError::unavailable(format!(
                "requested device {:?} not found among integrated OpenCL GPUs",
                device_req
            )));
        }
        if matched.len() > 1 {
            return Err(BackendError::unavailable(format!(
                "ambiguous requested device {:?}: {} integrated matches",
                device_req,
                matched.len()
            )));
        }
        let cand = matched[0];

        if cfg.simd != 0 && !cand.extensions.contains("cl_intel_required_subgroup_size") {
            return Err(BackendError::unavailable(
                "explicit Intel SIMD width unsupported by selected OpenCL device",
            ));
        }
        // create context
        let mut err = CL_SUCCESS;
        let ctx = unsafe {
            (api.create_context)(
                std::ptr::null(),
                1,
                &cand.device,
                None,
                std::ptr::null_mut(),
                &mut err,
            )
        };
        if ctx.is_null() || err != CL_SUCCESS {
            return Err(BackendError::driver_failure(format!(
                "clCreateContext failed ({err})"
            )));
        }

        // queue
        let mut queue = std::ptr::null_mut();
        let mut qmode = "default";
        if let Some(cqw) = api.create_command_queue_with_properties {
            // check extension throttle
            if cand.extensions.contains("cl_khr_throttle_hints") {
                let props: [isize; 3] = [
                    CL_QUEUE_THROTTLE_KHR as isize,
                    CL_QUEUE_THROTTLE_LOW_KHR as isize,
                    0,
                ];
                err = CL_SUCCESS;
                let q = unsafe { cqw(ctx, cand.device, props.as_ptr(), &mut err) };
                if !q.is_null() && err == CL_SUCCESS {
                    queue = q;
                    qmode = "throttle-low";
                }
            }
        }
        if queue.is_null() {
            err = CL_SUCCESS;
            queue = unsafe { (api.create_command_queue)(ctx, cand.device, 0, &mut err) };
            if queue.is_null() || err != CL_SUCCESS {
                unsafe { (api.release_context)(ctx) };
                return Err(BackendError::driver_failure(format!(
                    "command queue creation failed ({err})"
                )));
            }
            qmode = "default";
        }
        let _ = qmode;

        // build source
        let src_ptr = src_cstr.as_ptr();
        let mut prog_err = CL_SUCCESS;
        let program = unsafe {
            (api.create_program_with_source)(ctx, 1, &src_ptr, std::ptr::null(), &mut prog_err)
        };
        if program.is_null() || prog_err != CL_SUCCESS {
            unsafe {
                (api.release_command_queue)(queue);
                (api.release_context)(ctx);
            }
            return Err(BackendError::driver_failure(format!(
                "clCreateProgramWithSource failed ({prog_err})"
            )));
        }

        let build_rc = unsafe {
            (api.build_program)(
                program,
                1,
                &cand.device,
                c"-cl-std=CL1.2".as_ptr(),
                None,
                std::ptr::null_mut(),
            )
        };
        if build_rc != CL_SUCCESS {
            // sanitized log
            let mut logbuf = vec![0u8; 4096];
            let mut n = 0usize;
            unsafe {
                (api.get_program_build_info)(
                    program,
                    cand.device,
                    CL_PROGRAM_BUILD_LOG,
                    logbuf.len(),
                    logbuf.as_mut_ptr() as *mut c_void,
                    &mut n,
                )
            };
            let msg = String::from_utf8_lossy(&logbuf[..n.min(logbuf.len())]);
            unsafe {
                (api.release_program)(program);
                (api.release_command_queue)(queue);
                (api.release_context)(ctx)
            };
            return Err(BackendError::driver_failure(format!(
                "OpenCL build failed ({build_rc}): {}",
                msg.trim()
            )));
        }

        let mut kerr = CL_SUCCESS;
        let kernel = unsafe { (api.create_kernel)(program, c"gbtc_mine".as_ptr(), &mut kerr) };
        if kernel.is_null() || kerr != CL_SUCCESS {
            unsafe {
                (api.release_program)(program);
                (api.release_command_queue)(queue);
                (api.release_context)(ctx)
            };
            return Err(BackendError::driver_failure(format!(
                "clCreateKernel failed ({kerr})"
            )));
        }

        // max work group size
        let mut max_wg: cl_size_t = 0;
        let rc = unsafe {
            (api.get_kernel_work_group_info)(
                kernel,
                cand.device,
                CL_KERNEL_WORK_GROUP_SIZE,
                std::mem::size_of::<cl_size_t>(),
                &mut max_wg as *mut _ as *mut c_void,
                std::ptr::null_mut(),
            )
        };
        if rc != CL_SUCCESS || max_wg == 0 {
            unsafe {
                (api.release_kernel)(kernel);
                (api.release_program)(program);
                (api.release_command_queue)(queue);
                (api.release_context)(ctx)
            };
            return Err(BackendError::driver_failure(
                "clGetKernelWorkGroupInfo failed".to_string(),
            ));
        }

        // resolve local size
        let mut local_size = cfg.local_size;
        if cfg.local_size_auto {
            let mut v = 64u32;
            while v > max_wg as u32 && v > 8 {
                v /= 2;
            }
            if v > max_wg as u32 {
                unsafe {
                    (api.release_kernel)(kernel);
                    (api.release_program)(program);
                    (api.release_command_queue)(queue);
                    (api.release_context)(ctx)
                };
                return Err(BackendError::unavailable(format!(
                    "device max work-group size {} too small",
                    max_wg
                )));
            }
            local_size = v;
            cfg.local_size = v;
        } else if cfg.local_size > max_wg as u32 {
            unsafe {
                (api.release_kernel)(kernel);
                (api.release_program)(program);
                (api.release_command_queue)(queue);
                (api.release_context)(ctx)
            };
            return Err(BackendError::unavailable(format!(
                "local_size {} exceeds device max {}",
                cfg.local_size, max_wg
            )));
        }

        // buffers
        let in_words = if cfg.kernel == KernelVariant::Pre3 {
            24usize
        } else {
            16usize
        };
        let mut berr = CL_SUCCESS;
        let buf_in = unsafe {
            (api.create_buffer)(
                ctx,
                CL_MEM_READ_ONLY,
                in_words * std::mem::size_of::<cl_uint>(),
                std::ptr::null_mut(),
                &mut berr,
            )
        };
        if buf_in.is_null() || berr != CL_SUCCESS {
            unsafe {
                (api.release_kernel)(kernel);
                (api.release_program)(program);
                (api.release_command_queue)(queue);
                (api.release_context)(ctx)
            };
            return Err(BackendError::driver_failure(format!(
                "input buffer failed ({berr})"
            )));
        }
        let buf_out = unsafe {
            (api.create_buffer)(
                ctx,
                CL_MEM_READ_WRITE,
                16 * std::mem::size_of::<cl_uint>(),
                std::ptr::null_mut(),
                &mut berr,
            )
        };
        if buf_out.is_null() || berr != CL_SUCCESS {
            unsafe {
                (api.release_mem_object)(buf_in);
                (api.release_kernel)(kernel);
                (api.release_program)(program);
                (api.release_command_queue)(queue);
                (api.release_context)(ctx)
            };
            return Err(BackendError::driver_failure(format!(
                "output buffer failed ({berr})"
            )));
        }

        let info_str = format!(
            "OpenCL {} / {} / {}",
            cand.platform_name, cand.device_name, cand.device_version
        );
        let kernel_name = cfg.kernel.name().to_string();

        Ok(Self {
            api,
            platform: cand.platform,
            device: cand.device,
            context: ctx,
            queue,
            program,
            kernel,
            buf_in,
            buf_out,
            info_str,
            platform_name: cand.platform_name.clone(),
            device_name: cand.device_name.clone(),
            device_version: cand.device_version.clone(),
            config: cfg,
            local_size,
            kernel_name,
            poll_us: cfg.poll_us,
            stop: None,
            poisoned: false,
            pending: None,
        })
    }

    pub fn backend_info(&self) -> BackendInfo {
        BackendInfo::new(
            "opencl",
            "opencl",
            self.device_name.clone(),
            self.device_version.clone(),
        )
    }

    pub fn set_stop(&mut self, stop: Arc<AtomicBool>) {
        self.stop = Some(stop);
    }

    pub fn local_size(&self) -> u32 {
        self.local_size
    }

    pub fn kernel_name(&self) -> &str {
        &self.kernel_name
    }

    pub fn probe(requested: Option<&str>) -> Result<BackendInfo, BackendError> {
        let b = Self::new(requested)?;
        Ok(b.backend_info())
    }

    fn input_words(&self) -> usize {
        if self.config.kernel == KernelVariant::Pre3 {
            24
        } else {
            16
        }
    }

    fn run_dispatch(
        &mut self,
        work: &WorkBatch,
        count_override: u32,
    ) -> Result<(u32, Vec<u32>), BackendError> {
        if self.poisoned {
            return Err(BackendError::timeout("opencl context poisoned".to_string()));
        }
        if let Some(stop) = &self.stop {
            if stop.load(Ordering::Relaxed) {
                return Err(BackendError::invalid_input("cancelled".to_string()));
            }
        }

        // Build input
        let mut transfer = Box::new(HostTransfer {
            input: [0; 24],
            zero: 0,
            output: [0; 16],
        });
        let inbuf = &mut transfer.input;
        if self.config.kernel == KernelVariant::Pre3 {
            let mut s3 = [0u32; 8];
            kernel::sha_prefix3(&mut s3, &work.midstate, &work.tail3);
            inbuf[0..8].copy_from_slice(&s3);
            inbuf[8..16].copy_from_slice(&work.midstate);
            inbuf[16..19].copy_from_slice(&work.tail3);
            inbuf[19] = work.target[0];
            inbuf[20] = work.nonce_base;
            inbuf[21] = count_override;
        } else {
            inbuf[0..8].copy_from_slice(&work.midstate);
            inbuf[8..10].copy_from_slice(&work.tail3[0..2]);
            // tail3 has 3 words: indices 8,9,10
            inbuf[10] = work.tail3[2];
            inbuf[11] = work.target[0];
            inbuf[12] = work.nonce_base;
            inbuf[13] = count_override;
        }

        // set kernel args
        let buf_in = self.buf_in;
        let buf_out = self.buf_out;
        let kernel = self.kernel;
        let queue = self.queue;
        let api = &self.api;

        let mut err = unsafe {
            (api.set_kernel_arg)(
                kernel,
                0,
                std::mem::size_of::<cl_mem>(),
                &buf_in as *const _ as *const c_void,
            )
        };
        if err != CL_SUCCESS {
            return Err(BackendError::driver_failure(format!(
                "clSetKernelArg 0 failed ({err})"
            )));
        }
        err = unsafe {
            (api.set_kernel_arg)(
                kernel,
                1,
                std::mem::size_of::<cl_mem>(),
                &buf_out as *const _ as *const c_void,
            )
        };
        if err != CL_SUCCESS {
            return Err(BackendError::driver_failure(format!(
                "clSetKernelArg 1 failed ({err})"
            )));
        }

        // nonblocking chained events
        let flush = api.flush.unwrap();
        let get_event_info = api.get_event_info.unwrap();
        let release_event = api.release_event.unwrap();

        let mut ev_w0: cl_event = std::ptr::null_mut();
        let mut ev_w1: cl_event = std::ptr::null_mut();
        let mut ev_k: cl_event = std::ptr::null_mut();
        let mut ev_r: cl_event = std::ptr::null_mut();
        self.poisoned = true;

        let res = (|| -> Result<(u32, Vec<u32>), BackendError> {
            err = unsafe {
                (api.enqueue_write_buffer)(
                    queue,
                    buf_in,
                    0,
                    0,
                    self.input_words() * 4,
                    inbuf.as_ptr() as *const c_void,
                    0,
                    std::ptr::null(),
                    &mut ev_w0,
                )
            };
            if err != CL_SUCCESS || ev_w0.is_null() {
                return Err(BackendError::driver_failure(format!(
                    "write-in failed ({err})"
                )));
            }
            err = unsafe {
                (api.enqueue_write_buffer)(
                    queue,
                    buf_out,
                    0,
                    0,
                    4,
                    &transfer.zero as *const _ as *const c_void,
                    1,
                    &ev_w0,
                    &mut ev_w1,
                )
            };
            if err != CL_SUCCESS || ev_w1.is_null() {
                return Err(BackendError::driver_failure(format!(
                    "write-zero failed ({err})"
                )));
            }
            let global = if self.config.kernel == KernelVariant::Dual {
                (count_override as usize).div_ceil(2)
            } else {
                count_override as usize
            };
            // pad global to multiple of local_size
            let local = self.local_size as usize;
            let global_padded = if global == 0 {
                local
            } else {
                global.div_ceil(local) * local
            };
            let g = global_padded;
            let l = local;
            err = unsafe {
                (api.enqueue_nd_range_kernel)(
                    queue,
                    kernel,
                    1,
                    std::ptr::null(),
                    &g,
                    &l,
                    1,
                    &ev_w1,
                    &mut ev_k,
                )
            };
            if err != CL_SUCCESS || ev_k.is_null() {
                return Err(BackendError::driver_failure(format!(
                    "ndrange failed ({err})"
                )));
            }
            // Read is queued asynchronously too: a blocking read can spin inside NEO.
            err = unsafe {
                (api.enqueue_read_buffer)(
                    queue,
                    buf_out,
                    0,
                    0,
                    64,
                    transfer.output.as_mut_ptr() as *mut c_void,
                    1,
                    &ev_k,
                    &mut ev_r,
                )
            };
            if err != CL_SUCCESS || ev_r.is_null() {
                return Err(BackendError::driver_failure(format!(
                    "read enqueue failed ({err})"
                )));
            }
            err = unsafe { flush(queue) };
            if err != CL_SUCCESS {
                return Err(BackendError::driver_failure(format!(
                    "flush failed ({err})"
                )));
            }
            let start = Instant::now();
            loop {
                if self
                    .stop
                    .as_ref()
                    .is_some_and(|s| s.load(Ordering::Relaxed))
                {
                    return Err(BackendError::invalid_input("cancelled"));
                }
                let mut status: cl_int = 0;
                let rc = unsafe {
                    get_event_info(
                        ev_r,
                        CL_EVENT_COMMAND_EXECUTION_STATUS,
                        std::mem::size_of::<cl_int>(),
                        &mut status as *mut _ as *mut c_void,
                        std::ptr::null_mut(),
                    )
                };
                if rc != CL_SUCCESS || status < 0 {
                    return Err(BackendError::driver_failure(format!(
                        "event status failed ({rc}, {status})"
                    )));
                }
                if status == CL_COMPLETE {
                    break;
                }
                if start.elapsed() >= Duration::from_secs(30) {
                    return Err(BackendError::timeout("OpenCL event wait timed out"));
                }
                std::thread::sleep(Duration::from_micros(self.poll_us as u64));
            }
            let out = &transfer.output;
            let counter = out[0];
            let n = (counter as usize).min(15);
            let mut cands = Vec::with_capacity(n);
            for i in 0..n {
                cands.push(out[1 + i]);
            }
            Ok((counter, cands))
        })();

        // Commands retain event dependencies. Host storage survives every error until queue drain.
        if !ev_r.is_null() {
            unsafe { release_event(ev_r) };
        }
        // release events
        if !ev_w0.is_null() {
            unsafe { release_event(ev_w0) };
        }
        if !ev_w1.is_null() {
            unsafe { release_event(ev_w1) };
        }
        if !ev_k.is_null() {
            unsafe { release_event(ev_k) };
        }

        if res.is_err() {
            self.pending = Some(transfer);
        } else {
            self.poisoned = false;
        }
        res
    }

    fn run_range(&mut self, work: &WorkBatch) -> Result<(Vec<u32>, u32), BackendError> {
        let (counter, cands) = self.run_dispatch(work, work.nonce_count)?;
        if counter <= 15 {
            return Ok((cands, 0));
        }
        if work.nonce_count <= 1 {
            return Ok((cands, 0));
        }
        // overflow: subdivide
        let mid = work.nonce_count / 2;
        if mid == 0 {
            return Ok((cands, 0));
        }
        let mut left = work.clone();
        left.nonce_count = mid;
        let mut right = work.clone();
        right.nonce_base = work.nonce_base.wrapping_add(mid);
        right.nonce_count = work.nonce_count - mid;

        let (mut lc, r1) = self.run_range(&left)?;
        let (rc, r2) = self.run_range(&right)?;
        lc.extend(rc);
        // dedup
        let set: std::collections::BTreeSet<u32> = lc.into_iter().collect();
        let out: Vec<u32> = set.into_iter().collect();
        Ok((out, r1 + r2 + 1))
    }
}

impl Drop for OpenClBackend {
    fn drop(&mut self) {
        // Keep the ICD and transfer memory alive while draining even a cancelled queue.
        let api = self.api.clone();
        let handles = [
            self.queue as usize,
            self.buf_out as usize,
            self.buf_in as usize,
            self.kernel as usize,
            self.program as usize,
            self.context as usize,
        ];
        let pending = self.pending.take();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            unsafe {
                (api.finish)(handles[0] as cl_command_queue);
                (api.release_mem_object)(handles[1] as cl_mem);
                (api.release_mem_object)(handles[2] as cl_mem);
                (api.release_kernel)(handles[3] as cl_kernel);
                (api.release_program)(handles[4] as cl_program);
                (api.release_command_queue)(handles[0] as cl_command_queue);
                (api.release_context)(handles[5] as cl_context);
            }
            drop(pending);
            let _ = tx.send(());
        });
        if rx.recv_timeout(Duration::from_secs(5)).is_err() {
            eprintln!("OpenCL teardown exceeded 5 second deadline");
            std::process::exit(1);
        }
    }
}

impl ComputeBackend for OpenClBackend {
    fn run_batch(&mut self, work: &WorkBatch) -> Result<BatchResult, BackendError> {
        work.validate()?;
        let mut all = Vec::new();
        let mut rescans = 0u32;
        // chunk by 1<<16
        let chunk_size = 1u32 << 26;
        let mut base = work.nonce_base;
        let mut remaining = work.nonce_count;
        while remaining > 0 {
            if let Some(stop) = &self.stop {
                if stop.load(Ordering::Relaxed) {
                    return Err(BackendError::invalid_input("cancelled".to_string()));
                }
            }
            let n = remaining.min(chunk_size);
            let sub = WorkBatch {
                midstate: work.midstate,
                tail3: work.tail3,
                target: work.target,
                nonce_base: base,
                nonce_count: n,
            };
            let (cands, r) = self.run_range(&sub)?;
            all.extend(cands);
            rescans = rescans.saturating_add(r);
            base = base.wrapping_add(n);
            remaining -= n;
        }
        all.sort();
        all.dedup();
        Ok(BatchResult {
            candidates: all,
            unique_hashes: work.nonce_count as u64,
            overflow_rescans: rescans,
        })
    }
}

fn config_from_env(cfg: &mut Config) -> Result<(), BackendError> {
    // local size
    if let Ok(v) = std::env::var("GBTC_OPENCL_LOCAL_SIZE") {
        if !v.is_empty() {
            if v == "auto" {
                cfg.local_size_auto = true;
            } else {
                let parsed: u32 = v.parse().map_err(|_| {
                    BackendError::invalid_input(
                        "GBTC_OPENCL_LOCAL_SIZE must be auto, 8, 16, 32, 64, 128, 256, or 512",
                    )
                })?;
                if !kernel::local_size_allowed(parsed) {
                    return Err(BackendError::invalid_input(
                        "GBTC_OPENCL_LOCAL_SIZE must be auto, 8, 16, 32, 64, 128, 256, or 512",
                    ));
                }
                cfg.local_size = parsed;
                cfg.local_size_auto = false;
            }
        }
    }
    // poll
    if let Ok(v) = std::env::var("GBTC_OPENCL_POLL_US") {
        if !v.is_empty() {
            let parsed: u32 = v.parse().map_err(|_| {
                BackendError::invalid_input(
                    "GBTC_OPENCL_POLL_US must be an integer from 100 to 50000 microseconds",
                )
            })?;
            if !(100..=50000).contains(&parsed) {
                return Err(BackendError::invalid_input(
                    "GBTC_OPENCL_POLL_US must be an integer from 100 to 50000 microseconds",
                ));
            }
            cfg.poll_us = parsed;
        }
    }
    // simd
    if let Ok(v) = std::env::var("GBTC_OPENCL_SIMD") {
        if !v.is_empty() {
            if v == "auto" {
                cfg.simd = 0;
            } else {
                let parsed: u32 = v.parse().map_err(|_| {
                    BackendError::invalid_input("GBTC_OPENCL_SIMD must be auto, 8, 16, or 32")
                })?;
                if !kernel::simd_allowed(parsed) {
                    return Err(BackendError::invalid_input(
                        "GBTC_OPENCL_SIMD must be auto, 8, 16, or 32",
                    ));
                }
                cfg.simd = parsed;
            }
        }
    }
    // kernel
    if let Ok(v) = std::env::var("GBTC_OPENCL_KERNEL") {
        if !v.is_empty() {
            cfg.kernel = KernelVariant::parse(&v)?;
        }
    }
    Ok(())
}

// expose config_from_env? keep private

pub fn probe(requested: Option<&str>) -> Result<BackendInfo, BackendError> {
    let b = OpenClBackend::new(requested)?;
    Ok(b.backend_info())
}
