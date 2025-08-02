use super::instance::Instance;
use anyhow::{Context, Result};
use ash::vk;

pub struct PhysicalDeviceSelector<'a> {
    instance: &'a Instance,
}

impl<'a> PhysicalDeviceSelector<'a> {
    pub fn with_instance(instance: &'a Instance) -> Self {
        Self { instance }
    }

    pub fn select(&self) -> Result<PhysicalDevice> {
        let physical_devices = unsafe {
            self.instance
                .raw
                .enumerate_physical_devices()
                .context("Failed to enumerate physical devices")?
        };

        let raw = *physical_devices
            .iter()
            .max_by_key(|device| {
                let properties =
                    unsafe { self.instance.raw.get_physical_device_properties(**device) };
                match properties.device_type {
                    vk::PhysicalDeviceType::DISCRETE_GPU => 1000,
                    vk::PhysicalDeviceType::INTEGRATED_GPU => 10,
                    _ => 0,
                }
            })
            .ok_or(anyhow::anyhow!("failed to find physical device"))?;

        Ok(PhysicalDevice { raw })
    }
}

pub struct PhysicalDevice {
    pub raw: vk::PhysicalDevice,
}
