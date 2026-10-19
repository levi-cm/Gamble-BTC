//! Vulkan backend.
use ash::vk;
use gbtc_core::BackendError;
use std::collections::HashSet;
use std::ffi::CStr;
use std::os::raw::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub mod device;
pub use device::{DeviceInfo, Limits};

use gbtc_core::backend::{BackendTelemetry, BatchResult, BatchTicket, ComputeBackend, WorkBatch};

const SHADER_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/sha256d.spv"));
const CANDIDATE_CAPACITY: usize = 1024;
const PUSH_CONSTANT_SIZE: usize = 22 * 4;
const SLOT_COUNT: usize = 2;

pub fn decode_spirv(bytes: &[u8]) -> Result<Vec<u32>, BackendError> {
    if bytes.is_empty() {
        return Err(BackendError::invalid_input("empty SPIR-V".to_string()));
    }
    if !bytes.len().is_multiple_of(4) {
        return Err(BackendError::invalid_input(format!(
            "SPIR-V byte length {} is not a multiple of 4",
            bytes.len()
        )));
    }
    let mut words = Vec::with_capacity(bytes.len() / 4);
    for chunk in bytes.chunks(4) {
        words.push(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    if words.first().copied() != Some(0x07230203) {
        return Err(BackendError::invalid_input("bad SPIR-V magic".to_string()));
    }
    Ok(words)
}

#[derive(Clone, Copy)]
struct PrefixCache {
    midstate: [u32; 8],
    tail3: [u32; 3],
    prefix: [u32; 8],
}

struct Buffer {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    mapped: Option<*mut c_void>,
    size: u64,
    coherent: bool,
}

struct Slot {
    output_buffer: Buffer,
    command_buffer: vk::CommandBuffer,
    fence: vk::Fence,
    descriptor_pool: vk::DescriptorPool,
    descriptor_set: vk::DescriptorSet,
    query_pool: Option<vk::QueryPool>,
    work: Option<WorkBatch>,
    ticket: Option<BatchTicket>,
    result: Option<BatchResult>,
    pending: bool,
}

pub struct VulkanContext {
    entry: ash::Entry,
    instance: ash::Instance,
    physical_device: vk::PhysicalDevice,
    device: ash::Device,
    queue_family_index: u32,
    queue: vk::Queue,
    command_pool: vk::CommandPool,
    info: DeviceInfo,
    limits: Limits,
    poll_us: u64,
    wait_us: u64,
    wait_mode: WaitMode,
    local_size: u32,
    kernel_name: String,
    stop: Option<Arc<AtomicBool>>,
    poisoned: bool,
    debug_utils: Option<ash::ext::debug_utils::Instance>,
    debug_messenger: Option<vk::DebugUtilsMessengerEXT>,
    #[allow(clippy::box_collection)]
    _debug_messages: Option<Box<std::sync::Mutex<Vec<String>>>>,

    descriptor_set_layout: vk::DescriptorSetLayout,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    shader_module: vk::ShaderModule,

    slots: [Slot; SLOT_COUNT],

    timestamps_supported: bool,
    timestamp_period: f32,
    timestamp_valid_bits: u32,

    prefix_cache: Option<PrefixCache>,

    telemetry: BackendTelemetry,
    next_ticket: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WaitMode {
    Fence,
    Poll,
}

impl VulkanContext {
    pub fn kernel_name(&self) -> &str {
        &self.kernel_name
    }

    pub fn new(requested: Option<&str>) -> Result<Self, BackendError> {
        Self::create(requested, None)
    }

    fn create(requested: Option<&str>, failure_stage: Option<&str>) -> Result<Self, BackendError> {
        let poll_us = std::env::var("GBTC_VULKAN_POLL_US")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "1000".into())
            .parse::<u64>()
            .ok()
            .filter(|v| (100..=50000).contains(v))
            .ok_or_else(|| {
                BackendError::invalid_input("Vulkan poll interval must be 100..50000 us")
            })?;
        let wait_mode = match std::env::var("GBTC_VULKAN_WAIT_MODE")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "fence".into())
            .as_str()
        {
            "fence" => WaitMode::Fence,
            "poll" => WaitMode::Poll,
            _ => {
                return Err(BackendError::invalid_input(
                    "GBTC_VULKAN_WAIT_MODE must be fence or poll".to_string(),
                ))
            }
        };
        let wait_us = std::env::var("GBTC_VULKAN_WAIT_US")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "10000".into())
            .parse::<u64>()
            .ok()
            .filter(|v| (1000..=100000).contains(v))
            .ok_or_else(|| {
                BackendError::invalid_input("Vulkan wait interval must be 1000..100000 us")
            })?;
        let local_size = std::env::var("GBTC_VULKAN_LOCAL_SIZE")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "64".into())
            .parse::<u32>()
            .ok()
            .filter(|v| [8, 16, 32, 64, 128, 256].contains(v))
            .ok_or_else(|| BackendError::invalid_input("unsupported Vulkan local size"))?;
        let kernel_name = std::env::var("GBTC_VULKAN_KERNEL")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "pre3".into());
        let shader = match kernel_name.as_str() {
            "looped" => SHADER_SPV,
            "unrolled" => include_bytes!(concat!(env!("OUT_DIR"), "/unrolled.spv")),
            "pre3" => include_bytes!(concat!(env!("OUT_DIR"), "/pre3.spv")),
            "pruned" => include_bytes!(concat!(env!("OUT_DIR"), "/pruned.spv")),
            "boolean" => include_bytes!(concat!(env!("OUT_DIR"), "/boolean.spv")),
            _ => {
                return Err(BackendError::invalid_input(
                    "GBTC_VULKAN_KERNEL must be looped, unrolled, pre3, pruned, or boolean",
                ))
            }
        };
        let entry = unsafe { ash::Entry::load() }
            .map_err(|e| BackendError::unavailable(format!("Vulkan loader: {e}")))?;
        let validation = std::env::var("GBTC_VULKAN_VALIDATION").ok().as_deref() == Some("1");
        let layer = c"VK_LAYER_KHRONOS_validation";
        let layers = if validation {
            let available = unsafe { entry.enumerate_instance_layer_properties() }
                .map_err(|e| BackendError::driver_failure(format!("enumerate layers: {e}")))?;
            if !available
                .iter()
                .any(|p| unsafe { CStr::from_ptr(p.layer_name.as_ptr()) } == layer)
            {
                return Err(BackendError::unavailable(
                    "GBTC_VULKAN_VALIDATION=1 but VK_LAYER_KHRONOS_validation is unavailable",
                ));
            }
            vec![layer.as_ptr()]
        } else {
            Vec::new()
        };
        let extensions = if validation {
            vec![ash::ext::debug_utils::NAME.as_ptr()]
        } else {
            Vec::new()
        };
        let app = vk::ApplicationInfo::default()
            .application_name(c"gbtc")
            .api_version(vk::API_VERSION_1_1);
        let create = vk::InstanceCreateInfo::default()
            .application_info(&app)
            .enabled_layer_names(&layers)
            .enabled_extension_names(&extensions);
        let instance = unsafe { entry.create_instance(&create, None) }
            .map_err(|e| BackendError::driver_failure(format!("create instance: {e}")))?;
        let mut init = InitGuard::new(&instance);
        let messages = if validation {
            Some(Box::new(std::sync::Mutex::new(Vec::<String>::new())))
        } else {
            None
        };
        if let Some(data) = messages.as_ref() {
            let utils = ash::ext::debug_utils::Instance::new(&entry, &instance);
            let ci = vk::DebugUtilsMessengerCreateInfoEXT::default()
                .message_severity(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR)
                .message_type(
                    vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                        | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                        | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
                )
                .pfn_user_callback(Some(debug_callback))
                .user_data(data.as_ref() as *const std::sync::Mutex<Vec<String>> as *mut c_void);
            let messenger = unsafe { utils.create_debug_utils_messenger(&ci, None) }
                .map_err(|e| BackendError::driver_failure(format!("debug messenger: {e}")))?;
            init.debug_utils = Some(utils);
            init.debug_messenger = Some(messenger);
        }
        init.messages = messages;
        let entries = device::enumerate(&entry, &instance)?;
        let req = requested
            .map(str::to_owned)
            .or_else(|| std::env::var("GBTC_DEVICE").ok());
        let index = device::select_index(&entries, req.as_deref())?;
        let (physical_device, info, limits) = entries[index].clone();
        device::validate_limits(&limits)?;
        if info.api_version < vk::API_VERSION_1_1
            || local_size > limits.max_compute_work_group_invocations
            || local_size > limits.max_compute_work_group_size[0]
        {
            return Err(BackendError::unavailable(
                "Vulkan API/local-size limits unsupported",
            ));
        }
        let queue_family_index = device::find_compute_queue_family(&instance, physical_device)
            .ok_or_else(|| BackendError::unavailable("no compute queue"))?;
        let priorities = [1.0f32];
        let queues = [vk::DeviceQueueCreateInfo::default()
            .queue_family_index(queue_family_index)
            .queue_priorities(&priorities)];
        let ci = vk::DeviceCreateInfo::default().queue_create_infos(&queues);
        let logical = unsafe { instance.create_device(physical_device, &ci, None) }
            .map_err(|e| BackendError::driver_failure(format!("create device: {e}")))?;
        init.device = Some(logical.clone());
        let queue = unsafe { logical.get_device_queue(queue_family_index, 0) };
        init.command_pool = unsafe {
            logical.create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .queue_family_index(queue_family_index)
                    .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                None,
            )
        }
        .map_err(|e| BackendError::driver_failure(format!("command pool: {e}")))?;
        // timestamp support
        let qf_props =
            unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
        let ts_valid_bits = if (queue_family_index as usize) < qf_props.len() {
            qf_props[queue_family_index as usize].timestamp_valid_bits
        } else {
            0
        };
        let timestamps_supported = ts_valid_bits > 0;
        let timestamp_period = if timestamps_supported {
            let props = unsafe { instance.get_physical_device_properties(physical_device) };
            props.limits.timestamp_period
        } else {
            0.0
        };

        init.check_failure(failure_stage, "buffer")?;
        let binding = [vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::COMPUTE)];
        init.descriptor_set_layout = unsafe {
            logical.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&binding),
                None,
            )
        }
        .map_err(|e| BackendError::driver_failure(format!("descriptor layout: {e}")))?;
        let layouts = [init.descriptor_set_layout];
        let ranges = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::COMPUTE)
            .offset(0)
            .size(PUSH_CONSTANT_SIZE as u32)];
        init.pipeline_layout = unsafe {
            logical.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&layouts)
                    .push_constant_ranges(&ranges),
                None,
            )
        }
        .map_err(|e| BackendError::driver_failure(format!("pipeline layout: {e}")))?;
        init.check_failure(failure_stage, "layout")?;
        let spirv = decode_spirv(shader)?;
        init.shader_module = unsafe {
            logical.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&spirv), None)
        }
        .map_err(|e| BackendError::driver_failure(format!("shader: {e}")))?;
        init.check_failure(failure_stage, "shader")?;
        let values = local_size.to_ne_bytes();
        let map = [vk::SpecializationMapEntry {
            constant_id: 0,
            offset: 0,
            size: 4,
        }];
        let special = vk::SpecializationInfo::default()
            .map_entries(&map)
            .data(&values);
        let stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(init.shader_module)
            .name(c"main")
            .specialization_info(&special);
        let pipeline_ci = [vk::ComputePipelineCreateInfo::default()
            .stage(stage)
            .layout(init.pipeline_layout)];
        init.pipeline = match unsafe {
            logical.create_compute_pipelines(vk::PipelineCache::null(), &pipeline_ci, None)
        } {
            Ok(p) => p[0],
            Err((partial, e)) => {
                for p in partial {
                    if p != vk::Pipeline::null() {
                        unsafe {
                            logical.destroy_pipeline(p, None);
                        }
                    }
                }
                return Err(BackendError::driver_failure(format!(
                    "compute pipeline: {e}"
                )));
            }
        };
        init.check_failure(failure_stage, "pipeline")?;
        init.check_failure(failure_stage, "descriptors")?;
        init.check_failure(failure_stage, "fence")?;

        // Create slots.
        let command_pool = init.command_pool;
        let descriptor_set_layout = init.descriptor_set_layout;
        let pipeline_layout = init.pipeline_layout;
        let mut slots: [Option<Slot>; SLOT_COUNT] = [None, None];
        for i in 0..SLOT_COUNT {
            match create_slot(
                &logical,
                &instance,
                physical_device,
                init.command_pool,
                descriptor_set_layout,
                timestamps_supported,
            ) {
                Ok(s) => slots[i] = Some(s),
                Err(e) => {
                    // cleanup created slots
                    for s in slots.iter_mut().flatten() {
                        unsafe {
                            if let Some(qp) = s.query_pool.take() {
                                logical.destroy_query_pool(qp, None);
                            }
                            logical.destroy_fence(s.fence, None);
                            logical.free_command_buffers(command_pool, &[s.command_buffer]);
                            logical.destroy_descriptor_pool(s.descriptor_pool, None);
                        }
                        cleanup_buffer(&logical, &s.output_buffer);
                    }
                    return Err(e);
                }
            }
        }
        let slots = [slots[0].take().unwrap(), slots[1].take().unwrap()];

        let command_pool = init.command_pool;
        let pipeline = init.pipeline;
        let shader_module = init.shader_module;
        let debug_utils = init.debug_utils.take();
        let debug_messenger = init.debug_messenger.take();
        let debug_messages = init.messages.take();
        init.armed = false;
        drop(init);

        Ok(Self {
            entry,
            instance,
            physical_device,
            device: logical,
            queue_family_index,
            queue,
            command_pool,
            info,
            limits,
            poll_us,
            wait_us,
            wait_mode,
            local_size,
            kernel_name,
            stop: None,
            poisoned: false,
            debug_utils,
            debug_messenger,
            _debug_messages: debug_messages,
            descriptor_set_layout,
            pipeline_layout,
            pipeline,
            shader_module,
            slots,
            timestamps_supported,
            timestamp_period,
            timestamp_valid_bits: ts_valid_bits,
            prefix_cache: None,
            telemetry: if timestamps_supported {
                BackendTelemetry {
                    gpu_ns: Some(0),
                    ..Default::default()
                }
            } else {
                BackendTelemetry::default()
            },
            next_ticket: 1,
        })
    }

    pub fn backend_info(&self) -> gbtc_core::BackendInfo {
        let driver = format!("0x{:x}", self.info.driver_version);
        let device = format!(
            "{} (vendor 0x{:x}, device 0x{:x})",
            self.info.name, self.info.vendor_id, self.info.device_id
        );
        let api_major = vk::api_version_major(self.info.api_version);
        let api_minor = vk::api_version_minor(self.info.api_version);
        let api = format!("Vulkan {}.{}", api_major, api_minor);
        gbtc_core::BackendInfo::new("vulkan", api, device, driver)
    }

    pub fn info(&self) -> &DeviceInfo {
        &self.info
    }
    pub fn set_stop(&mut self, stop: Arc<AtomicBool>) {
        self.stop = Some(stop);
    }
    pub fn clear_stop(&mut self) {
        self.stop = None;
    }
    pub fn local_size(&self) -> u32 {
        self.local_size
    }
    pub fn poll_us(&self) -> u64 {
        self.poll_us
    }
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }
    pub fn limits(&self) -> &Limits {
        &self.limits
    }
    pub fn device(&self) -> &ash::Device {
        &self.device
    }
    pub fn queue(&self) -> vk::Queue {
        self.queue
    }
    pub fn command_pool(&self) -> vk::CommandPool {
        self.command_pool
    }
    pub fn queue_family_index(&self) -> u32 {
        self.queue_family_index
    }
    pub fn physical_device(&self) -> vk::PhysicalDevice {
        self.physical_device
    }
    pub fn instance(&self) -> &ash::Instance {
        &self.instance
    }
    pub fn entry(&self) -> &ash::Entry {
        &self.entry
    }

    fn free_slot_index(&self) -> Option<usize> {
        self.slots.iter().position(|s| !s.pending)
    }

    fn max_chunk_invocations(&self) -> u32 {
        let groups = self.limits.max_compute_work_group_count[0].max(1);
        groups.saturating_mul(self.local_size).max(self.local_size)
    }

    fn wait_slot_fence(&mut self, slot_idx: usize) -> Result<(), BackendError> {
        let fence = self.slots[slot_idx].fence;
        let stop = self.stop.clone();
        let deadline = Duration::from_secs(30);
        let start = Instant::now();
        loop {
            if let Some(s) = &stop {
                if s.load(Ordering::Relaxed) {
                    self.poisoned = true;
                    return Err(BackendError::invalid_input("cancelled".to_string()));
                }
            }
            match self.wait_mode {
                WaitMode::Fence => {
                    self.telemetry.fence_waits = self.telemetry.fence_waits.saturating_add(1);
                    let t0 = Instant::now();
                    let res = unsafe {
                        self.device
                            .wait_for_fences(&[fence], true, self.wait_us * 1000)
                    };
                    self.telemetry.wait_wall_ns = self
                        .telemetry
                        .wait_wall_ns
                        .saturating_add(t0.elapsed().as_nanos() as u64);
                    match res {
                        Ok(()) => return Ok(()),
                        Err(vk::Result::TIMEOUT) => {
                            if start.elapsed() >= deadline {
                                self.poisoned = true;
                                return Err(BackendError::timeout(
                                    "fence wait timed out".to_string(),
                                ));
                            }
                        }
                        Err(vk::Result::ERROR_DEVICE_LOST) => {
                            self.poisoned = true;
                            return Err(BackendError::device_lost(
                                "device lost waiting for fence".to_string(),
                            ));
                        }
                        Err(e) => {
                            self.poisoned = true;
                            return Err(BackendError::driver_failure(format!(
                                "fence wait failed: {e}"
                            )));
                        }
                    }
                }
                WaitMode::Poll => {
                    self.telemetry.poll_checks = self.telemetry.poll_checks.saturating_add(1);
                    match unsafe { self.device.get_fence_status(fence) } {
                        Ok(true) => return Ok(()),
                        Ok(false) => {
                            if start.elapsed() >= deadline {
                                self.poisoned = true;
                                return Err(BackendError::timeout(
                                    "fence wait timed out".to_string(),
                                ));
                            }
                            std::thread::sleep(Duration::from_micros(self.poll_us));
                        }
                        Err(vk::Result::ERROR_DEVICE_LOST) => {
                            self.poisoned = true;
                            return Err(BackendError::device_lost(
                                "device lost polling fence".to_string(),
                            ));
                        }
                        Err(e) => {
                            self.poisoned = true;
                            return Err(BackendError::driver_failure(format!("fence status: {e}")));
                        }
                    }
                }
            }
        }
    }

    fn reset_slot_fence(&mut self, slot_idx: usize) -> Result<(), BackendError> {
        let fence = self.slots[slot_idx].fence;
        unsafe {
            self.device.reset_fences(&[fence]).map_err(|e| {
                if e == vk::Result::ERROR_DEVICE_LOST {
                    self.poisoned = true;
                    BackendError::device_lost(format!("reset fence: {e}"))
                } else {
                    self.poisoned = true;
                    BackendError::driver_failure(format!("reset fence: {e}"))
                }
            })?
        };
        Ok(())
    }

    fn push_constants_bytes(&mut self, work: &WorkBatch) -> [u8; PUSH_CONSTANT_SIZE] {
        let prefix = if let Some(c) = &self.prefix_cache {
            if c.midstate == work.midstate && c.tail3 == work.tail3 {
                c.prefix
            } else {
                let p = gbtc_core::sha256::prefix3(&work.midstate, &work.tail3);
                self.prefix_cache = Some(PrefixCache {
                    midstate: work.midstate,
                    tail3: work.tail3,
                    prefix: p,
                });
                p
            }
        } else {
            let p = gbtc_core::sha256::prefix3(&work.midstate, &work.tail3);
            self.prefix_cache = Some(PrefixCache {
                midstate: work.midstate,
                tail3: work.tail3,
                prefix: p,
            });
            p
        };

        let mut pc = [0u32; 22];
        pc[0..8].copy_from_slice(&work.midstate);
        pc[8..11].copy_from_slice(&work.tail3);
        pc[11] = work.nonce_base;
        pc[12] = work.target[0];
        pc[13] = work.nonce_count;
        pc[14..22].copy_from_slice(&prefix);

        let mut bytes = [0u8; PUSH_CONSTANT_SIZE];
        for (i, w) in pc.iter().enumerate() {
            bytes[i * 4..(i + 1) * 4].copy_from_slice(&w.to_le_bytes());
        }
        bytes
    }

    fn record_dispatch_slot(
        &mut self,
        slot_idx: usize,
        work: &WorkBatch,
        timestamp: bool,
    ) -> Result<(), BackendError> {
        let cb = self.slots[slot_idx].command_buffer;
        let output_buffer = &self.slots[slot_idx].output_buffer;
        let output_buf = output_buffer.buffer;
        let output_size = output_buffer.size;
        let descriptor_set = self.slots[slot_idx].descriptor_set;
        let query_pool = self.slots[slot_idx].query_pool;
        let local_size = self.local_size;
        let pipeline = self.pipeline;
        let pipeline_layout = self.pipeline_layout;
        let pc_bytes = self.push_constants_bytes(work);

        unsafe {
            self.device
                .reset_command_buffer(cb, vk::CommandBufferResetFlags::empty())
                .map_err(|e| BackendError::driver_failure(format!("reset cb: {e}")))?
        };

        let begin = vk::CommandBufferBeginInfo {
            s_type: vk::StructureType::COMMAND_BUFFER_BEGIN_INFO,
            p_next: std::ptr::null(),
            flags: vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT,
            p_inheritance_info: std::ptr::null(),
            ..Default::default()
        };
        unsafe {
            self.device
                .begin_command_buffer(cb, &begin)
                .map_err(|e| BackendError::driver_failure(format!("begin cb: {e}")))?
        };

        if timestamp {
            if let Some(qp) = query_pool {
                unsafe {
                    self.device.cmd_reset_query_pool(cb, qp, 0, 2);
                }
            }
        }

        unsafe {
            self.device
                .cmd_fill_buffer(cb, output_buf, 0, output_size, 0);
        }

        let barrier1 = vk::BufferMemoryBarrier {
            s_type: vk::StructureType::BUFFER_MEMORY_BARRIER,
            p_next: std::ptr::null(),
            src_access_mask: vk::AccessFlags::TRANSFER_WRITE,
            dst_access_mask: vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE,
            src_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
            dst_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
            buffer: output_buf,
            offset: 0,
            size: vk::WHOLE_SIZE,
            ..Default::default()
        };
        unsafe {
            self.device.cmd_pipeline_barrier(
                cb,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[barrier1],
                &[],
            );
        }

        unsafe {
            self.device
                .cmd_bind_pipeline(cb, vk::PipelineBindPoint::COMPUTE, pipeline);
            self.device.cmd_bind_descriptor_sets(
                cb,
                vk::PipelineBindPoint::COMPUTE,
                pipeline_layout,
                0,
                &[descriptor_set],
                &[],
            );
        }

        unsafe {
            self.device.cmd_push_constants(
                cb,
                pipeline_layout,
                vk::ShaderStageFlags::COMPUTE,
                0,
                &pc_bytes,
            );
        }

        let groups = work.nonce_count.div_ceil(local_size);
        if groups > self.limits.max_compute_work_group_count[0] {
            return Err(BackendError::invalid_input(
                "dispatch exceeds workgroup count limit".to_string(),
            ));
        }

        if timestamp {
            if let Some(qp) = query_pool {
                unsafe {
                    self.device
                        .cmd_write_timestamp(cb, vk::PipelineStageFlags::TOP_OF_PIPE, qp, 0);
                }
            }
        }

        unsafe {
            self.device.cmd_dispatch(cb, groups, 1, 1);
        }

        let barrier2 = vk::BufferMemoryBarrier {
            s_type: vk::StructureType::BUFFER_MEMORY_BARRIER,
            p_next: std::ptr::null(),
            src_access_mask: vk::AccessFlags::SHADER_WRITE,
            dst_access_mask: vk::AccessFlags::HOST_READ,
            src_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
            dst_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
            buffer: output_buf,
            offset: 0,
            size: vk::WHOLE_SIZE,
            ..Default::default()
        };
        unsafe {
            self.device.cmd_pipeline_barrier(
                cb,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::HOST,
                vk::DependencyFlags::empty(),
                &[],
                &[barrier2],
                &[],
            );
        }

        if timestamp {
            if let Some(qp) = query_pool {
                unsafe {
                    self.device.cmd_write_timestamp(
                        cb,
                        vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                        qp,
                        1,
                    );
                }
            }
        }

        unsafe {
            self.device
                .end_command_buffer(cb)
                .map_err(|e| BackendError::driver_failure(format!("end cb: {e}")))?
        };

        let submit_info = vk::SubmitInfo {
            s_type: vk::StructureType::SUBMIT_INFO,
            p_next: std::ptr::null(),
            wait_semaphore_count: 0,
            p_wait_semaphores: std::ptr::null(),
            p_wait_dst_stage_mask: std::ptr::null(),
            command_buffer_count: 1,
            p_command_buffers: &cb,
            signal_semaphore_count: 0,
            p_signal_semaphores: std::ptr::null(),
            ..Default::default()
        };
        let fence = self.slots[slot_idx].fence;
        unsafe {
            self.device
                .queue_submit(self.queue, &[submit_info], fence)
                .map_err(|e| {
                    if e == vk::Result::ERROR_DEVICE_LOST {
                        self.poisoned = true;
                        BackendError::device_lost(format!("submit: {e}"))
                    } else {
                        self.poisoned = true;
                        BackendError::driver_failure(format!("submit: {e}"))
                    }
                })?
        };
        Ok(())
    }

    fn collect_slot_gpu_ns(&mut self, slot_idx: usize) {
        if !self.timestamps_supported {
            return;
        }
        let qp = match self.slots[slot_idx].query_pool {
            Some(q) => q,
            None => return,
        };
        let mut results = [0u64; 2];
        let r = unsafe {
            self.device
                .get_query_pool_results(qp, 0, &mut results, vk::QueryResultFlags::TYPE_64)
        };
        if r.is_ok() {
            let mask = u64::MAX >> (64 - self.timestamp_valid_bits);
            let delta = results[1].wrapping_sub(results[0]) & mask;
            let ns = (delta as f64 * self.timestamp_period as f64) as u64;
            let cur = self.telemetry.gpu_ns.get_or_insert(0);
            *cur = cur.saturating_add(ns);
        } else {
            // Completed fences should make queries ready; report unavailable on driver error.
            self.telemetry.gpu_ns = None;
            self.timestamps_supported = false;
        }
    }

    fn read_slot_output(&mut self, slot_idx: usize) -> Result<(u32, Vec<u32>), BackendError> {
        let slot = &self.slots[slot_idx];
        let output_buffer = &slot.output_buffer;
        if !output_buffer.coherent {
            let range = vk::MappedMemoryRange {
                s_type: vk::StructureType::MAPPED_MEMORY_RANGE,
                p_next: std::ptr::null(),
                memory: output_buffer.memory,
                offset: 0,
                size: vk::WHOLE_SIZE,
                ..Default::default()
            };
            unsafe {
                self.device
                    .invalidate_mapped_memory_ranges(&[range])
                    .map_err(|e| BackendError::driver_failure(format!("invalidate: {e}")))?
            };
        }
        let ptr = output_buffer.mapped.unwrap() as *const u32;
        let counter = unsafe { ptr.read_volatile() };
        let n = (counter as usize).min(CANDIDATE_CAPACITY);
        let mut cands = Vec::with_capacity(n);
        for i in 0..n {
            cands.push(unsafe { ptr.add(1 + i).read_volatile() });
        }
        Ok((counter, cands))
    }

    fn dispatch_and_read_slot(
        &mut self,
        slot_idx: usize,
        work: &WorkBatch,
    ) -> Result<(u32, Vec<u32>), BackendError> {
        if self.poisoned {
            return Err(BackendError::timeout("vulkan context poisoned".to_string()));
        }
        if let Some(stop) = &self.stop {
            if stop.load(Ordering::Relaxed) {
                return Err(BackendError::invalid_input("cancelled".to_string()));
            }
        }
        work.validate()?;

        // ensure previous done
        self.wait_slot_fence(slot_idx)?;

        self.poisoned = true;
        let t_start = Instant::now();
        self.reset_slot_fence(slot_idx)?;
        self.record_dispatch_slot(slot_idx, work, self.timestamps_supported)?;
        self.telemetry.submissions = self.telemetry.submissions.saturating_add(1);
        self.telemetry.submit_wall_ns = self
            .telemetry
            .submit_wall_ns
            .saturating_add(t_start.elapsed().as_nanos() as u64);

        // await
        self.wait_slot_fence(slot_idx)?;
        self.collect_slot_gpu_ns(slot_idx);

        let (counter, cands) = self.read_slot_output(slot_idx)?;
        self.poisoned = false;
        Ok((counter, cands))
    }

    fn run_range_slot(
        &mut self,
        slot_idx: usize,
        work: &WorkBatch,
    ) -> Result<(Vec<u32>, u32), BackendError> {
        let (counter, cands) = self.dispatch_and_read_slot(slot_idx, work)?;
        if counter <= CANDIDATE_CAPACITY as u32 {
            return Ok((cands, 0));
        }
        if work.nonce_count <= 1 {
            return Ok((cands, 0));
        }
        let mid = work.nonce_count / 2;
        let mut left = work.clone();
        left.nonce_count = mid;
        let mut right = work.clone();
        right.nonce_base = work.nonce_base.wrapping_add(mid);
        right.nonce_count = work.nonce_count - mid;

        let (mut lc, r1) = self.run_range_slot(slot_idx, &left)?;
        let (rc, r2) = self.run_range_slot(slot_idx, &right)?;
        lc.extend(rc);

        let set: HashSet<u32> = lc.into_iter().collect();
        let mut out: Vec<u32> = set.into_iter().collect();
        out.sort();
        Ok((out, r1 + r2 + 1))
    }

    fn run_batch_slot(
        &mut self,
        slot_idx: usize,
        work: &WorkBatch,
    ) -> Result<BatchResult, BackendError> {
        work.validate()?;
        let mut all = Vec::new();
        let mut rescans = 0u32;
        let chunk = self.max_chunk_invocations();
        let mut base = work.nonce_base;
        let mut remaining = work.nonce_count;
        while remaining > 0 {
            if let Some(stop) = &self.stop {
                if stop.load(Ordering::Relaxed) {
                    return Err(BackendError::invalid_input("cancelled".to_string()));
                }
            }
            let n = remaining.min(chunk);
            let sub = WorkBatch {
                midstate: work.midstate,
                tail3: work.tail3,
                target: work.target,
                nonce_base: base,
                nonce_count: n,
            };
            let (cands, r) = self.run_range_slot(slot_idx, &sub)?;
            all.extend(cands);
            rescans = rescans.saturating_add(r);
            base = base.wrapping_add(n);
            remaining -= n;
        }
        all.sort();
        all.dedup();
        if let Some(messages) = &self._debug_messages {
            if !messages
                .lock()
                .map_err(|_| BackendError::driver_failure("validation storage poisoned"))?
                .is_empty()
            {
                return Err(BackendError::driver_failure(
                    "Vulkan validation reported errors",
                ));
            }
        }
        Ok(BatchResult {
            candidates: all,
            unique_hashes: work.nonce_count as u64,
            overflow_rescans: rescans,
        })
    }

    fn record_and_submit_slot(
        &mut self,
        slot_idx: usize,
        work: &WorkBatch,
    ) -> Result<(), BackendError> {
        if self.poisoned {
            return Err(BackendError::timeout("vulkan context poisoned".to_string()));
        }
        if let Some(stop) = &self.stop {
            if stop.load(Ordering::Relaxed) {
                return Err(BackendError::invalid_input("cancelled".to_string()));
            }
        }
        work.validate()?;
        let groups = work.nonce_count.div_ceil(self.local_size);
        if groups > self.limits.max_compute_work_group_count[0] {
            return Err(BackendError::invalid_input(
                "dispatch exceeds workgroup count limit".to_string(),
            ));
        }

        self.wait_slot_fence(slot_idx)?;
        self.poisoned = true;
        let t_start = Instant::now();
        self.reset_slot_fence(slot_idx)?;
        self.record_dispatch_slot(slot_idx, work, self.timestamps_supported)?;
        self.telemetry.submissions = self.telemetry.submissions.saturating_add(1);
        self.telemetry.submit_wall_ns = self
            .telemetry
            .submit_wall_ns
            .saturating_add(t_start.elapsed().as_nanos() as u64);
        self.poisoned = false;
        Ok(())
    }
}

impl ComputeBackend for VulkanContext {
    fn run_batch(&mut self, work: &WorkBatch) -> Result<BatchResult, BackendError> {
        work.validate()?;
        // synchronous adapter: use first free slot or slot 0
        let slot_idx = match self.free_slot_index() {
            Some(i) => i,
            None => {
                return Err(BackendError::unavailable("submission queue full"));
            }
        };
        let res = self.run_batch_slot(slot_idx, work)?;
        Ok(res)
    }

    fn queue_capacity(&self) -> usize {
        SLOT_COUNT
    }

    fn submit_batch(&mut self, work: &WorkBatch) -> Result<BatchTicket, BackendError> {
        work.validate()?;
        if self.poisoned {
            return Err(BackendError::timeout("vulkan context poisoned".to_string()));
        }
        if let Some(stop) = &self.stop {
            if stop.load(Ordering::Relaxed) {
                return Err(BackendError::invalid_input("cancelled".to_string()));
            }
        }
        let needs_sync = work.nonce_count > self.max_chunk_invocations()
            || work.nonce_count.div_ceil(self.local_size)
                > self.limits.max_compute_work_group_count[0];

        let slot_idx = match self.free_slot_index() {
            Some(i) => i,
            None => {
                return Err(BackendError::unavailable(
                    "submission queue full".to_string(),
                ))
            }
        };

        let ticket = BatchTicket(self.next_ticket);
        self.next_ticket = self.next_ticket.saturating_add(1);

        if needs_sync {
            let res = self.run_batch_slot(slot_idx, work)?;
            self.slots[slot_idx].work = Some(work.clone());
            self.slots[slot_idx].result = Some(res);
            self.slots[slot_idx].ticket = Some(ticket);
            self.slots[slot_idx].pending = true;
        } else {
            self.record_and_submit_slot(slot_idx, work)?;
            self.slots[slot_idx].work = Some(work.clone());
            self.slots[slot_idx].ticket = Some(ticket);
            self.slots[slot_idx].pending = true;
        }

        Ok(ticket)
    }

    fn retire_batch(&mut self, ticket: BatchTicket) -> Result<BatchResult, BackendError> {
        // find slot
        let slot_idx = self.slots.iter().position(|s| s.ticket == Some(ticket));
        let slot_idx = match slot_idx {
            Some(i) => i,
            None => {
                return Err(BackendError::invalid_input(
                    "unknown or already retired ticket".to_string(),
                ))
            }
        };

        // take ticket to prevent double retirement
        self.slots[slot_idx].ticket = None;

        // cached sync result
        if let Some(res) = self.slots[slot_idx].result.take() {
            let rescans = res.overflow_rescans as u64;
            self.slots[slot_idx].work = None;
            self.slots[slot_idx].pending = false;
            self.telemetry.completed_batches = self.telemetry.completed_batches.saturating_add(1);
            self.telemetry.overflow_rescans =
                self.telemetry.overflow_rescans.saturating_add(rescans);
            return Ok(res);
        }

        let work = match self.slots[slot_idx].work.take() {
            Some(w) => w,
            None => {
                self.slots[slot_idx].pending = false;
                return Err(BackendError::invalid_input(
                    "ticket has no work".to_string(),
                ));
            }
        };

        // wait for completion
        self.wait_slot_fence(slot_idx)?;
        self.collect_slot_gpu_ns(slot_idx);

        let (counter, cands) = self.read_slot_output(slot_idx)?;

        let (candidates, rescans) = if counter <= CANDIDATE_CAPACITY as u32 {
            (cands, 0u32)
        } else {
            // overflow: subdivide synchronously
            if work.nonce_count <= 1 {
                (cands, 0u32)
            } else {
                let mid = work.nonce_count / 2;
                let mut left = work.clone();
                left.nonce_count = mid;
                let mut right = work.clone();
                right.nonce_base = work.nonce_base.wrapping_add(mid);
                right.nonce_count = work.nonce_count - mid;
                let (mut lc, r1) = self.run_range_slot(slot_idx, &left)?;
                let (rc, r2) = self.run_range_slot(slot_idx, &right)?;
                lc.extend(rc);
                let set: HashSet<u32> = lc.into_iter().collect();
                let mut out: Vec<u32> = set.into_iter().collect();
                out.sort();
                (out, r1 + r2 + 1)
            }
        };

        let mut all = candidates;
        all.sort();
        all.dedup();

        if let Some(messages) = &self._debug_messages {
            if !messages
                .lock()
                .map_err(|_| BackendError::driver_failure("validation storage poisoned"))?
                .is_empty()
            {
                self.slots[slot_idx].pending = false;
                return Err(BackendError::driver_failure(
                    "Vulkan validation reported errors",
                ));
            }
        }

        self.slots[slot_idx].pending = false;
        self.telemetry.completed_batches = self.telemetry.completed_batches.saturating_add(1);
        self.telemetry.overflow_rescans = self
            .telemetry
            .overflow_rescans
            .saturating_add(rescans as u64);

        Ok(BatchResult {
            candidates: all,
            unique_hashes: work.nonce_count as u64,
            overflow_rescans: rescans,
        })
    }

    fn telemetry(&self) -> BackendTelemetry {
        self.telemetry.clone()
    }
}

impl Drop for VulkanContext {
    fn drop(&mut self) {
        let (tx, rx) = std::sync::mpsc::channel();
        let device = self.device.clone();
        std::thread::spawn(move || {
            let _ = unsafe { device.device_wait_idle() };
            let _ = tx.send(());
        });
        if rx.recv_timeout(Duration::from_secs(5)).is_err() {
            std::process::exit(1);
        }

        unsafe {
            for slot in self.slots.iter_mut() {
                if let Some(qp) = slot.query_pool.take() {
                    self.device.destroy_query_pool(qp, None);
                }
                self.device.destroy_fence(slot.fence, None);
                self.device
                    .free_command_buffers(self.command_pool, &[slot.command_buffer]);
                self.device
                    .destroy_descriptor_pool(slot.descriptor_pool, None);
                cleanup_buffer(&self.device, &slot.output_buffer);
            }
            self.device
                .destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            self.device.destroy_pipeline(self.pipeline, None);
            self.device.destroy_shader_module(self.shader_module, None);
            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.device.destroy_command_pool(self.command_pool, None);
            if let Some(m) = self.debug_messenger.take() {
                if let Some(du) = self.debug_utils.as_ref() {
                    du.destroy_debug_utils_messenger(m, None);
                }
            }
            self.device.destroy_device(None);
            self.instance.destroy_instance(None);
        }
    }
}

unsafe extern "system" fn debug_callback(
    _severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    _ty: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    user_data: *mut std::os::raw::c_void,
) -> vk::Bool32 {
    if !user_data.is_null() && !data.is_null() {
        let msg = if !(*data).p_message.is_null() {
            CStr::from_ptr((*data).p_message)
                .to_string_lossy()
                .into_owned()
        } else {
            String::new()
        };
        let messages = &*(user_data as *const std::sync::Mutex<Vec<String>>);
        eprintln!("Vulkan validation error: {msg}");
        if let Ok(mut entries) = messages.lock() {
            entries.push(msg);
        }
    }
    vk::FALSE
}

struct InitGuard<'a> {
    instance: &'a ash::Instance,
    device: Option<ash::Device>,
    debug_utils: Option<ash::ext::debug_utils::Instance>,
    debug_messenger: Option<vk::DebugUtilsMessengerEXT>,
    #[allow(clippy::box_collection)]
    messages: Option<Box<std::sync::Mutex<Vec<String>>>>,
    command_pool: vk::CommandPool,
    descriptor_set_layout: vk::DescriptorSetLayout,
    pipeline_layout: vk::PipelineLayout,
    shader_module: vk::ShaderModule,
    pipeline: vk::Pipeline,
    armed: bool,
}
impl<'a> InitGuard<'a> {
    fn new(instance: &'a ash::Instance) -> Self {
        Self {
            instance,
            device: None,
            debug_utils: None,
            debug_messenger: None,
            messages: None,
            command_pool: vk::CommandPool::null(),
            descriptor_set_layout: vk::DescriptorSetLayout::null(),
            pipeline_layout: vk::PipelineLayout::null(),
            shader_module: vk::ShaderModule::null(),
            pipeline: vk::Pipeline::null(),
            armed: true,
        }
    }
    fn check_failure(&self, requested: Option<&str>, stage: &str) -> Result<(), BackendError> {
        if requested == Some(stage) {
            Err(BackendError::driver_failure(format!(
                "injected initialization failure: {stage}"
            )))
        } else {
            Ok(())
        }
    }
}
impl Drop for InitGuard<'_> {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        unsafe {
            if let Some(d) = &self.device {
                d.destroy_pipeline(self.pipeline, None);
                d.destroy_shader_module(self.shader_module, None);
                d.destroy_pipeline_layout(self.pipeline_layout, None);
                d.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
                d.destroy_command_pool(self.command_pool, None);
                d.destroy_device(None);
            }
            if let (Some(utils), Some(messenger)) = (&self.debug_utils, self.debug_messenger) {
                utils.destroy_debug_utils_messenger(messenger, None);
            }
            self.instance.destroy_instance(None);
        }
    }
}

fn create_slot(
    device: &ash::Device,
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    command_pool: vk::CommandPool,
    descriptor_set_layout: vk::DescriptorSetLayout,
    timestamps_supported: bool,
) -> Result<Slot, BackendError> {
    let output_buffer = create_buffer(
        device,
        instance,
        physical_device,
        ((CANDIDATE_CAPACITY + 1) * 4) as u64,
        vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
        vk::MemoryPropertyFlags::HOST_VISIBLE,
    )?;

    let sizes = [vk::DescriptorPoolSize {
        ty: vk::DescriptorType::STORAGE_BUFFER,
        descriptor_count: 1,
    }];
    let descriptor_pool = match unsafe {
        device.create_descriptor_pool(
            &vk::DescriptorPoolCreateInfo::default()
                .max_sets(1)
                .pool_sizes(&sizes),
            None,
        )
    } {
        Ok(p) => p,
        Err(e) => {
            cleanup_buffer(device, &output_buffer);
            return Err(BackendError::driver_failure(format!(
                "descriptor pool: {e}"
            )));
        }
    };

    let layouts = [descriptor_set_layout];
    let descriptor_set = match unsafe {
        device.allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(descriptor_pool)
                .set_layouts(&layouts),
        )
    } {
        Ok(v) => v[0],
        Err(e) => {
            unsafe {
                device.destroy_descriptor_pool(descriptor_pool, None);
            }
            cleanup_buffer(device, &output_buffer);
            return Err(BackendError::driver_failure(format!(
                "allocate descriptor: {e}"
            )));
        }
    };

    let buffers = [vk::DescriptorBufferInfo {
        buffer: output_buffer.buffer,
        offset: 0,
        range: output_buffer.size,
    }];
    let writes = [vk::WriteDescriptorSet::default()
        .dst_set(descriptor_set)
        .dst_binding(0)
        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
        .buffer_info(&buffers)];
    unsafe {
        device.update_descriptor_sets(&writes, &[]);
    }

    let command_buffer = match unsafe {
        device.allocate_command_buffers(
            &vk::CommandBufferAllocateInfo::default()
                .command_pool(command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1),
        )
    } {
        Ok(v) => v[0],
        Err(e) => {
            unsafe {
                device.destroy_descriptor_pool(descriptor_pool, None);
            }
            cleanup_buffer(device, &output_buffer);
            return Err(BackendError::driver_failure(format!("command buffer: {e}")));
        }
    };

    let fence = match unsafe {
        device.create_fence(
            &vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED),
            None,
        )
    } {
        Ok(f) => f,
        Err(e) => {
            unsafe {
                device.free_command_buffers(command_pool, &[command_buffer]);
            }
            unsafe {
                device.destroy_descriptor_pool(descriptor_pool, None);
            }
            cleanup_buffer(device, &output_buffer);
            return Err(BackendError::driver_failure(format!("fence: {e}")));
        }
    };

    let query_pool = if timestamps_supported {
        match unsafe {
            device.create_query_pool(
                &vk::QueryPoolCreateInfo::default()
                    .query_type(vk::QueryType::TIMESTAMP)
                    .query_count(2),
                None,
            )
        } {
            Ok(q) => Some(q),
            Err(e) => {
                unsafe {
                    device.destroy_fence(fence, None);
                }
                unsafe {
                    device.free_command_buffers(command_pool, &[command_buffer]);
                }
                unsafe {
                    device.destroy_descriptor_pool(descriptor_pool, None);
                }
                cleanup_buffer(device, &output_buffer);
                return Err(BackendError::driver_failure(format!("query pool: {e}")));
            }
        }
    } else {
        None
    };

    Ok(Slot {
        output_buffer,
        command_buffer,
        fence,
        descriptor_pool,
        descriptor_set,
        query_pool,
        work: None,
        ticket: None,
        result: None,
        pending: false,
    })
}

fn cleanup_buffer(device: &ash::Device, buffer: &Buffer) {
    unsafe {
        if buffer.mapped.is_some() {
            device.unmap_memory(buffer.memory);
        }
        device.destroy_buffer(buffer.buffer, None);
        device.free_memory(buffer.memory, None);
    }
}

fn create_buffer(
    device: &ash::Device,
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    size: u64,
    usage: vk::BufferUsageFlags,
    required_flags: vk::MemoryPropertyFlags,
) -> Result<Buffer, BackendError> {
    let buffer_info = vk::BufferCreateInfo {
        s_type: vk::StructureType::BUFFER_CREATE_INFO,
        p_next: std::ptr::null(),
        flags: vk::BufferCreateFlags::empty(),
        size,
        usage,
        sharing_mode: vk::SharingMode::EXCLUSIVE,
        queue_family_index_count: 0,
        p_queue_family_indices: std::ptr::null(),
        ..Default::default()
    };
    let buffer = unsafe {
        device
            .create_buffer(&buffer_info, None)
            .map_err(|e| BackendError::driver_failure(format!("create buffer: {e}")))?
    };
    let mem_req = unsafe { device.get_buffer_memory_requirements(buffer) };
    let mem_props = unsafe { instance.get_physical_device_memory_properties(physical_device) };
    let mem_type_index =
        match find_memory_type(&mem_props, mem_req.memory_type_bits, required_flags) {
            Some(index) => index,
            None => {
                unsafe {
                    device.destroy_buffer(buffer, None);
                }
                return Err(BackendError::unavailable("no suitable memory type"));
            }
        };
    let coherent = mem_props.memory_types[mem_type_index as usize]
        .property_flags
        .contains(vk::MemoryPropertyFlags::HOST_COHERENT);
    let alloc_info = vk::MemoryAllocateInfo {
        s_type: vk::StructureType::MEMORY_ALLOCATE_INFO,
        p_next: std::ptr::null(),
        allocation_size: mem_req.size,
        memory_type_index: mem_type_index,
        ..Default::default()
    };
    let memory = match unsafe { device.allocate_memory(&alloc_info, None) } {
        Ok(memory) => memory,
        Err(e) => {
            unsafe {
                device.destroy_buffer(buffer, None);
            }
            return Err(BackendError::driver_failure(format!(
                "allocate memory: {e}"
            )));
        }
    };
    let mut owned = Buffer {
        buffer,
        memory,
        mapped: None,
        size,
        coherent,
    };
    if let Err(e) = unsafe { device.bind_buffer_memory(buffer, memory, 0) } {
        cleanup_buffer(device, &owned);
        return Err(BackendError::driver_failure(format!("bind memory: {e}")));
    }
    if required_flags.contains(vk::MemoryPropertyFlags::HOST_VISIBLE) {
        match unsafe { device.map_memory(memory, 0, vk::WHOLE_SIZE, vk::MemoryMapFlags::empty()) } {
            Ok(ptr) => owned.mapped = Some(ptr),
            Err(e) => {
                cleanup_buffer(device, &owned);
                return Err(BackendError::driver_failure(format!("map memory: {e}")));
            }
        }
    }
    Ok(owned)
}

fn find_memory_type(
    props: &vk::PhysicalDeviceMemoryProperties,
    type_filter: u32,
    required: vk::MemoryPropertyFlags,
) -> Option<u32> {
    (0..props.memory_type_count).find(|&i| {
        (type_filter & (1 << i)) != 0
            && props.memory_types[i as usize]
                .property_flags
                .contains(required)
    })
}

pub fn probe() -> Result<gbtc_core::BackendInfo, BackendError> {
    let ctx = VulkanContext::new(None)?;
    Ok(ctx.backend_info())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_callback_uses_stable_synchronized_storage() {
        let messages = std::sync::Mutex::new(Vec::<String>::new());
        let text = c"synthetic validation error";
        let data = vk::DebugUtilsMessengerCallbackDataEXT::default().message(text);
        unsafe {
            debug_callback(
                vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
                vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION,
                &data,
                &messages as *const _ as *mut c_void,
            );
        }
        assert_eq!(
            messages.lock().unwrap().as_slice(),
            ["synthetic validation error"]
        );
    }

    #[test]
    fn injected_initialization_failures_release_resources() {
        if probe().is_err() {
            assert_ne!(
                std::env::var("GBTC_REQUIRE_GPU_TESTS").ok().as_deref(),
                Some("1")
            );
            return;
        }
        for stage in [
            "buffer",
            "layout",
            "shader",
            "pipeline",
            "descriptors",
            "fence",
        ] {
            assert!(VulkanContext::create(None, Some(stage)).is_err());
        }
        assert!(VulkanContext::new(None).is_ok());
    }

    #[test]
    fn decode_embedded_shader() {
        let words = decode_spirv(SHADER_SPV).expect("decode");
        assert!(!words.is_empty());
        assert_eq!(words[0], 0x07230203);
    }

    #[test]
    fn push_constant_layout() {
        assert_eq!(PUSH_CONSTANT_SIZE, 88);
        assert_eq!(CANDIDATE_CAPACITY, 1024);
    }
}
