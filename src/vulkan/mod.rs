use anyhow::Result;
use ash::vk;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::sync::Arc;

pub mod command_ring_buffer;
pub mod device;
pub mod instance;
pub mod physical_device;
pub mod surface;
pub mod swapchain;

pub struct RenderBackendConfig {
    pub validation_layers: bool,
    pub vsync: bool,
}

pub struct RenderBackend {
    pub swapchain: swapchain::Swapchain,
    pub surface: Arc<surface::Surface>,
    pub device: Arc<device::Device>,
}

impl RenderBackend {
    pub fn new(
        window: &(impl HasDisplayHandle + HasWindowHandle),
        config: &RenderBackendConfig,
    ) -> Result<Self> {
        let required_window_extensions =
            ash_window::enumerate_required_extensions(window.display_handle().unwrap().as_raw())
                .unwrap();
        let instance = Arc::new(
            instance::InstanceBuilder::default()
                .required_extensions(required_window_extensions)
                .enable_validation_layers(config.validation_layers)
                .build()?,
        );

        let physical_device_selector =
            physical_device::PhysicalDeviceSelector::with_instance(&instance);
        let physical_device = Arc::new(physical_device_selector.select()?);

        let device_builder = device::DeviceBuilder::new(instance, physical_device);
        let device = Arc::new(device_builder.build()?);

        let surface = Arc::new(surface::Surface::new(&device, window)?);

        let surface_format = vk::SurfaceFormatKHR {
            format: vk::Format::B8G8R8A8_SRGB,
            color_space: vk::ColorSpaceKHR::SRGB_NONLINEAR,
        };
        let supported_surface_formats =
            swapchain::Swapchain::enumerate_surface_formats(&device, &surface)?;
        if !supported_surface_formats.contains(&surface_format) {
            anyhow::bail!("Surface format not available");
        }

        let swapchain_desc = swapchain::SwapchainDesc {
            old_swapchain: None,
            format: surface_format,
            vsync: config.vsync,
        };
        let swapchain = swapchain::Swapchain::new(&device, &surface, swapchain_desc)?;

        Ok(Self {
            device,
            surface,
            swapchain,
        })
    }
}

impl Drop for RenderBackend {
    fn drop(&mut self) {
        let _ = unsafe { self.device.raw.device_wait_idle() };
        // struct fields are dropped in order
        // swapchain, then surface, then device
    }
}
