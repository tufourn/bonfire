use anyhow::Result;
use ash::vk;
use ash::vk::SwapchainCreateInfoKHR;

use super::device;
use super::instance;
use super::physical_device;
use super::surface;

#[derive(Clone)]
pub struct SwapchainDesc {
    pub old_swapchain: Option<vk::SwapchainKHR>,
    pub format: vk::SurfaceFormatKHR,
    pub extent: vk::Extent2D,
    pub vsync: bool,
}

pub struct Swapchain {
    pub raw: vk::SwapchainKHR,
    pub loader: ash::khr::swapchain::Device,
    pub desc: SwapchainDesc,
}

impl Swapchain {
    pub fn enumerate_surface_formats(
        physical_device: &physical_device::PhysicalDevice,
        surface: &surface::Surface,
    ) -> Result<Vec<vk::SurfaceFormatKHR>> {
        unsafe {
            Ok(surface
                .loader
                .get_physical_device_surface_formats(physical_device.raw, surface.raw)?)
        }
    }

    pub fn new(
        instance: &instance::Instance,
        physical_device: &physical_device::PhysicalDevice,
        device: &device::Device,
        surface: &surface::Surface,
        mut desc: SwapchainDesc,
    ) -> Result<Self> {
        let surface_capabilities = unsafe {
            surface
                .loader
                .get_physical_device_surface_capabilities(physical_device.raw, surface.raw)?
        };

        desc.extent = vk::Extent2D {
            width: desc.extent.width.clamp(
                surface_capabilities.min_image_extent.width,
                surface_capabilities.max_image_extent.width,
            ),
            height: desc.extent.height.clamp(
                surface_capabilities.min_image_extent.height,
                surface_capabilities.max_image_extent.height,
            ),
        };
        if desc.extent.width == 0 && desc.extent.height == 0 {
            anyhow::bail!("Swapchain extent cannot be zero");
        }

        let mut image_count = surface_capabilities.min_image_count + 1;
        if surface_capabilities.max_image_count > 0
            && image_count > surface_capabilities.max_image_count
        {
            image_count = surface_capabilities.max_image_count;
        }

        let present_modes = unsafe {
            surface
                .loader
                .get_physical_device_surface_present_modes(physical_device.raw, surface.raw)?
        };

        let present_mode_preference = if desc.vsync {
            [vk::PresentModeKHR::MAILBOX, vk::PresentModeKHR::FIFO]
        } else {
            [
                vk::PresentModeKHR::FIFO_RELAXED,
                vk::PresentModeKHR::IMMEDIATE,
            ]
        };

        let chosen_present_mode = present_mode_preference
            .into_iter()
            .find(|mode| present_modes.contains(mode))
            .unwrap_or(vk::PresentModeKHR::FIFO);

        let mut create_info = SwapchainCreateInfoKHR::default()
            .surface(surface.raw)
            .min_image_count(image_count)
            .image_format(desc.format.format)
            .image_color_space(desc.format.color_space)
            .image_extent(desc.extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(surface_capabilities.current_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(chosen_present_mode)
            .clipped(true);

        if let Some(old_swapchain) = desc.old_swapchain {
            create_info = create_info.old_swapchain(old_swapchain);
        }

        let loader = ash::khr::swapchain::Device::new(&instance.raw, &device.raw);
        let raw = unsafe { loader.create_swapchain(&create_info, None)? };

        println!(
            "Created swapchain: {}x{}",
            desc.extent.width, desc.extent.height
        );

        Ok(Self { raw, loader, desc })
    }
}
