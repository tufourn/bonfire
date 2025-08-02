use super::instance::Instance;
use super::physical_device::PhysicalDevice;
use anyhow::{Context, Result};
use std::ffi::CStr;

use ash::vk;

pub struct Queue {
    raw: vk::Queue,
    family: u32,
}

pub struct DeviceBuilder<'a, 'b> {
    instance: &'a Instance,
    physical_device: &'b PhysicalDevice,
}

impl<'a, 'b> DeviceBuilder<'a, 'b> {
    pub fn new(instance: &'a Instance, physical_device: &'b PhysicalDevice) -> Self {
        Self {
            instance,
            physical_device,
        }
    }

    pub fn build(&self) -> Result<Device> {
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
        ];

        for ext in &required_extensions {
            if !supported_extensions.contains(ext) {
                anyhow::bail!("Device extension not supported: {}", ext.to_string_lossy());
            }
        }

        let mut timeline_sem = vk::PhysicalDeviceTimelineSemaphoreFeatures::default();
        let mut desc_indexing = vk::PhysicalDeviceDescriptorIndexingFeatures::default();

        let required_extensions: Vec<*const i8> =
            required_extensions.iter().map(|ext| ext.as_ptr()).collect();

        let mut features2 = vk::PhysicalDeviceFeatures2::default()
            .push_next(&mut timeline_sem)
            .push_next(&mut desc_indexing);

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

        Ok(Device {
            raw: raw_device,
            graphics_queue,
            compute_queue,
            transfer_queue,
        })
    }
}

pub struct Device {
    pub raw: ash::Device,
    pub graphics_queue: Queue,
    pub compute_queue: Queue,
    pub transfer_queue: Queue,
}
