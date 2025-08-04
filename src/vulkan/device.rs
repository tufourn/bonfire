use super::instance::Instance;
use super::physical_device::PhysicalDevice;
use anyhow::{Context, Result};
use std::ffi::CStr;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use ash::vk;

pub const FRAMES_IN_FLIGHT: usize = 2;

pub struct DeviceBuilder {
    instance: Arc<Instance>,
    physical_device: Arc<PhysicalDevice>,
}

pub struct Device {
    pub raw: ash::Device,
    pub instance: Arc<Instance>,
    pub physical_device: Arc<PhysicalDevice>,

    pub graphics_queue: Queue,
    pub compute_queue: Queue,
    pub transfer_queue: Queue,

    pub graphics_timeline_semaphore: vk::Semaphore,
    absolute_frame_index: AtomicUsize,

    pub command_pools: [vk::CommandPool; FRAMES_IN_FLIGHT],
    pub command_buffers: [vk::CommandBuffer; FRAMES_IN_FLIGHT],
}

pub struct Queue {
    pub raw: vk::Queue,
    pub family: u32,
}

impl DeviceBuilder {
    pub fn new(instance: Arc<Instance>, physical_device: Arc<PhysicalDevice>) -> Self {
        Self {
            instance,
            physical_device,
        }
    }

    pub fn build(self) -> Result<Device> {
        let queue_family_properties = unsafe {
            self.instance
                .raw
                .get_physical_device_queue_family_properties(self.physical_device.raw)
        };

        let mut graphics_queue_family_index = None;
        let mut compute_queue_family_index = None;
        let mut transfer_queue_family_index = None;

        for (index, properties) in queue_family_properties.iter().enumerate() {
            let supports_graphics = properties.queue_flags.contains(vk::QueueFlags::GRAPHICS);
            let supports_compute = properties.queue_flags.contains(vk::QueueFlags::COMPUTE);
            let supports_transfer = properties.queue_flags.contains(vk::QueueFlags::TRANSFER);

            if supports_graphics && graphics_queue_family_index.is_none() {
                graphics_queue_family_index = Some(index as u32);
            }

            if !supports_graphics && supports_compute && compute_queue_family_index.is_none() {
                compute_queue_family_index = Some(index as u32);
            }

            if !supports_graphics
                && !supports_compute
                && supports_transfer
                && transfer_queue_family_index.is_none()
            {
                transfer_queue_family_index = Some(index as u32);
            }
        }

        let graphics_queue_family_index =
            graphics_queue_family_index.context("Failed to find graphics queue")?;
        let compute_queue_family_index =
            compute_queue_family_index.unwrap_or(graphics_queue_family_index);
        let transfer_queue_family_index =
            transfer_queue_family_index.unwrap_or(graphics_queue_family_index);

        let mut unique_queue_indices = std::collections::HashSet::new();
        unique_queue_indices.insert(graphics_queue_family_index);
        unique_queue_indices.insert(compute_queue_family_index);
        unique_queue_indices.insert(transfer_queue_family_index);

        let priorities = [1.0];
        let queue_create_info: Vec<vk::DeviceQueueCreateInfo> = unique_queue_indices
            .into_iter()
            .map(|index| {
                vk::DeviceQueueCreateInfo::default()
                    .queue_family_index(index)
                    .queue_priorities(&priorities)
            })
            .collect();

        let supported_extensions: Vec<&CStr> = unsafe {
            self.instance
                .raw
                .enumerate_device_extension_properties(self.physical_device.raw)
                .context("Failed to enumerate device extensions")?
                .iter()
                .map(|extension| CStr::from_ptr(extension.extension_name.as_ptr()))
                .collect()
        };

        let required_extensions = vec![
            ash::khr::swapchain::NAME,
            ash::khr::timeline_semaphore::NAME,
            ash::ext::descriptor_indexing::NAME,
            ash::khr::synchronization2::NAME,
        ];

        for ext in &required_extensions {
            if !supported_extensions.contains(ext) {
                anyhow::bail!("Device extension not supported: {}", ext.to_string_lossy());
            }
        }

        let mut timeline_sem = vk::PhysicalDeviceTimelineSemaphoreFeatures::default();
        let mut desc_indexing = vk::PhysicalDeviceDescriptorIndexingFeatures::default();
        let mut sync2 = vk::PhysicalDeviceSynchronization2Features::default();

        let required_extensions: Vec<*const i8> =
            required_extensions.iter().map(|ext| ext.as_ptr()).collect();

        let mut features2 = vk::PhysicalDeviceFeatures2::default()
            .push_next(&mut timeline_sem)
            .push_next(&mut desc_indexing)
            .push_next(&mut sync2);

        unsafe {
            self.instance
                .raw
                .get_physical_device_features2(self.physical_device.raw, &mut features2);
        }

        let create_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queue_create_info)
            .enabled_extension_names(&required_extensions)
            .push_next(&mut features2);

        let raw_device = unsafe {
            self.instance
                .raw
                .create_device(self.physical_device.raw, &create_info, None)
                .context("Failed to create logical device")?
        };

        assert!(timeline_sem.timeline_semaphore == vk::TRUE);
        assert!(desc_indexing.shader_uniform_texel_buffer_array_dynamic_indexing == vk::TRUE);
        assert!(desc_indexing.shader_storage_texel_buffer_array_dynamic_indexing == vk::TRUE);
        assert!(desc_indexing.shader_sampled_image_array_non_uniform_indexing == vk::TRUE);
        assert!(desc_indexing.shader_storage_image_array_non_uniform_indexing == vk::TRUE);
        assert!(desc_indexing.shader_uniform_texel_buffer_array_non_uniform_indexing == vk::TRUE);
        assert!(desc_indexing.shader_storage_texel_buffer_array_non_uniform_indexing == vk::TRUE);
        assert!(desc_indexing.descriptor_binding_sampled_image_update_after_bind == vk::TRUE);
        assert!(desc_indexing.descriptor_binding_update_unused_while_pending == vk::TRUE);
        assert!(desc_indexing.descriptor_binding_partially_bound == vk::TRUE);
        assert!(desc_indexing.descriptor_binding_variable_descriptor_count == vk::TRUE);
        assert!(desc_indexing.runtime_descriptor_array == vk::TRUE);

        let graphics_queue = Queue {
            raw: unsafe { raw_device.get_device_queue(graphics_queue_family_index, 0) },
            family: graphics_queue_family_index,
        };
        let compute_queue = Queue {
            raw: unsafe { raw_device.get_device_queue(compute_queue_family_index, 0) },
            family: compute_queue_family_index,
        };
        let transfer_queue = Queue {
            raw: unsafe { raw_device.get_device_queue(transfer_queue_family_index, 0) },
            family: transfer_queue_family_index,
        };

        // only 1 pool and buffer per frame for now
        // TODO: multithreading, compute
        let mut command_pools = Vec::with_capacity(FRAMES_IN_FLIGHT);
        let mut command_buffers = Vec::with_capacity(FRAMES_IN_FLIGHT);
        for _ in 0..FRAMES_IN_FLIGHT {
            let command_pool_create_info = vk::CommandPoolCreateInfo::default()
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
                .queue_family_index(graphics_queue.family);
            let command_pool =
                unsafe { raw_device.create_command_pool(&command_pool_create_info, None)? };

            let command_buffer_alloc_info = vk::CommandBufferAllocateInfo::default()
                .command_pool(command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1);
            let command_buffer =
                unsafe { raw_device.allocate_command_buffers(&command_buffer_alloc_info)? };

            command_pools.push(command_pool);
            command_buffers.push(command_buffer[0]);
        }
        let command_pools: [vk::CommandPool; FRAMES_IN_FLIGHT] = command_pools
            .try_into()
            .map_err(|_| anyhow::anyhow!("Failed to create command pools"))?;
        let command_buffers: [vk::CommandBuffer; FRAMES_IN_FLIGHT] = command_buffers
            .try_into()
            .map_err(|_| anyhow::anyhow!("Failed to create command buffers"))?;

        let mut timeline_semaphore_type_create_info = vk::SemaphoreTypeCreateInfo::default()
            .semaphore_type(vk::SemaphoreType::TIMELINE)
            .initial_value(0);
        let timeline_semaphore_create_info =
            vk::SemaphoreCreateInfo::default().push_next(&mut timeline_semaphore_type_create_info);

        let graphics_timeline_semaphore =
            unsafe { raw_device.create_semaphore(&timeline_semaphore_create_info, None) }?;

        Ok(Device {
            raw: raw_device,
            physical_device: self.physical_device,
            instance: self.instance,

            graphics_queue,
            compute_queue,
            transfer_queue,

            graphics_timeline_semaphore,
            absolute_frame_index: AtomicUsize::new(0),

            command_pools,
            command_buffers,
        })
    }
}

impl Device {
    pub fn absolute_frame_index(&self) -> usize {
        self.absolute_frame_index
            .load(std::sync::atomic::Ordering::Acquire)
    }
    pub fn begin_frame(&self, absolute_frame_index: usize) -> Result<()> {
        // wait for the frame submitted FRAMES_IN_FLIGHT ago
        if absolute_frame_index >= FRAMES_IN_FLIGHT {
            let wait_value = (absolute_frame_index - FRAMES_IN_FLIGHT + 1) as u64;
            let wait_info = vk::SemaphoreWaitInfo::default()
                .semaphores(std::slice::from_ref(&self.graphics_timeline_semaphore))
                .values(std::slice::from_ref(&wait_value));

            unsafe { self.raw.wait_semaphores(&wait_info, std::u64::MAX)? };
        }

        Ok(())
    }

    pub fn finish_frame(&self) {
        self.absolute_frame_index
            .fetch_add(1, std::sync::atomic::Ordering::Release);
    }

    pub fn get_command_buffer(&self, absolute_frame_index: usize) -> vk::CommandBuffer {
        self.command_buffers[absolute_frame_index % FRAMES_IN_FLIGHT]
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe {
            let _ = self.raw.device_wait_idle();

            for command_pool in self.command_pools {
                self.raw.destroy_command_pool(command_pool, None);
            }
            self.raw
                .destroy_semaphore(self.graphics_timeline_semaphore, None);
            self.raw.destroy_device(None);

            if let Some(debug_messenger) = self.instance.debug_messenger
                && let Some(ref debug_utils_loader) = self.instance.debug_utils_loader
            {
                debug_utils_loader.destroy_debug_utils_messenger(debug_messenger, None);
            }
            self.instance.raw.destroy_instance(None);
        }
    }
}
