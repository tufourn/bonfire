use anyhow::Result;
use ash::vk;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

pub mod device;
pub mod instance;
pub mod physical_device;
pub mod surface;
pub mod swapchain;

pub struct RenderBackendConfig {
    pub swapchain_extent: [u32; 2],
    pub validation_layers: bool,
    pub vsync: bool,
}

pub struct RenderBackend {
    pub instance: instance::Instance,
    pub physical_device: physical_device::PhysicalDevice,
    pub device: device::Device,
    pub surface: surface::Surface,
    pub swapchain: swapchain::Swapchain,
}

impl RenderBackend {
    pub fn new(
        window: &(impl HasDisplayHandle + HasWindowHandle),
        config: &RenderBackendConfig,
    ) -> Result<Self> {
        let required_window_extensions =
            ash_window::enumerate_required_extensions(window.display_handle().unwrap().as_raw())
                .unwrap();
        let instance = instance::InstanceBuilder::default()
            .required_extensions(required_window_extensions)
            .enable_validation_layers(config.validation_layers)
            .build()?;

        let physical_device_selector =
            physical_device::PhysicalDeviceSelector::with_instance(&instance);
        let physical_device = physical_device_selector.select()?;

        let device_builder = device::DeviceBuilder::new(&instance, &physical_device);
        let device = device_builder.build()?;

        let surface = surface::Surface::new(&instance, window)?;

        let surface_format = vk::SurfaceFormatKHR {
            format: vk::Format::B8G8R8A8_SRGB,
            color_space: vk::ColorSpaceKHR::SRGB_NONLINEAR,
        };
        let supported_surface_formats =
            swapchain::Swapchain::enumerate_surface_formats(&physical_device, &surface)?;
        if !supported_surface_formats.contains(&surface_format) {
            anyhow::bail!("Surface format not available");
        }

        let swapchain_desc = swapchain::SwapchainDesc {
            old_swapchain: None,
            format: surface_format,
            extent: vk::Extent2D {
                width: config.swapchain_extent[0],
                height: config.swapchain_extent[1],
            },
            vsync: config.vsync,
        };
        let swapchain = swapchain::Swapchain::new(
            &instance,
            &physical_device,
            &device,
            &surface,
            swapchain_desc,
        )?;

        Ok(Self {
            instance,
            physical_device,
            device,
            surface,
            swapchain,
        })
    }

    pub fn resize_swapchain(&mut self, extent: &[u32; 2]) -> Result<()> {
        let extent = vk::Extent2D {
            width: extent[0],
            height: extent[1],
        };

        let mut desc = self.swapchain.desc.clone();
        if desc.extent.width == extent.width && desc.extent.height == extent.height {
            return Ok(());
        }

        unsafe { self.device.raw.device_wait_idle()? };

        desc.old_swapchain = Some(self.swapchain.raw);
        desc.extent = extent;

        let new_swapchain = swapchain::Swapchain::new(
            &self.instance,
            &self.physical_device,
            &self.device,
            &self.surface,
            desc,
        )?;

        unsafe {
            self.swapchain
                .loader
                .destroy_swapchain(self.swapchain.raw, None);
        }

        self.swapchain = new_swapchain;

        Ok(())
    }
}

impl Drop for RenderBackend {
    fn drop(&mut self) {
        unsafe {
            self.device.raw.device_wait_idle().unwrap();

            self.swapchain
                .loader
                .destroy_swapchain(self.swapchain.raw, None);

            self.surface.loader.destroy_surface(self.surface.raw, None);

            self.device.raw.destroy_device(None);
            if let Some(debug_messenger) = self.instance.debug_messenger
                && let Some(ref debug_utils_loader) = self.instance.debug_utils_loader
            {
                debug_utils_loader.destroy_debug_utils_messenger(debug_messenger, None);
            }
            self.instance.raw.destroy_instance(None);
        }
    }
}
