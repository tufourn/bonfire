use anyhow::Result;
use ash::vk;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::sync::Arc;

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

    // pub fn begin_frame(&self) -> Result<()> {
    //     if self.device.absolute_frame_index >= device::FRAMES_IN_FLIGHT {
    //         let wait_value =
    //             (self.device.absolute_frame_index - device::FRAMES_IN_FLIGHT + 1) as u64;
    //         let wait_info = vk::SemaphoreWaitInfo::default()
    //             .semaphores(std::slice::from_ref(
    //                 &self.device.graphics_timeline_semaphore,
    //             ))
    //             .values(std::slice::from_ref(&wait_value));
    //
    //         unsafe { self.device.raw.wait_semaphores(&wait_info, std::u64::MAX)? };
    //     }
    //
    //     Ok(())
    // }
    //
    // //TODO: wrap command buffer
    // pub fn get_command_buffer(&self) -> vk::CommandBuffer {
    //     self.device.command_buffers[self.device.absolute_frame_index % device::FRAMES_IN_FLIGHT]
    // }
    //
    // pub fn submit(
    //     &self,
    //     command_buffer: vk::CommandBuffer,
    //     swapchain_image: &swapchain::SwapchainImage,
    // ) -> Result<()> {
    //     let mut wait_semaphores = vec![
    //         vk::SemaphoreSubmitInfo::default()
    //             .semaphore(swapchain_image.image_acquired_semaphore)
    //             .stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT),
    //     ];
    //     if self.device.absolute_frame_index >= device::FRAMES_IN_FLIGHT {
    //         wait_semaphores.push(
    //             vk::SemaphoreSubmitInfo::default()
    //                 .semaphore(self.device.graphics_timeline_semaphore)
    //                 .value((self.device.absolute_frame_index - device::FRAMES_IN_FLIGHT + 1) as u64)
    //                 .stage_mask(vk::PipelineStageFlags2::TOP_OF_PIPE),
    //         );
    //     }
    //
    //     let signal_semaphores = [
    //         vk::SemaphoreSubmitInfo::default()
    //             .semaphore(swapchain_image.render_finished_semaphore)
    //             .stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT),
    //         vk::SemaphoreSubmitInfo::default()
    //             .semaphore(self.device.graphics_timeline_semaphore)
    //             .value((self.device.absolute_frame_index + 1) as u64)
    //             .stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT),
    //     ];
    //
    //     let command_buffer_submit_info =
    //         vk::CommandBufferSubmitInfo::default().command_buffer(command_buffer);
    //
    //     let submit_info = vk::SubmitInfo2::default()
    //         .wait_semaphore_infos(&wait_semaphores)
    //         .signal_semaphore_infos(&signal_semaphores)
    //         .command_buffer_infos(std::slice::from_ref(&command_buffer_submit_info));
    //
    //     let _ = unsafe {
    //         self.device.raw.queue_submit2(
    //             self.device.graphics_queue.raw,
    //             std::slice::from_ref(&submit_info),
    //             vk::Fence::null(),
    //         )?
    //     };
    //
    //     Ok(())
    // }

    // pub fn finish_frame(&mut self) {
    //     self.device.absolute_frame_index += 1;
    // }
}

impl Drop for RenderBackend {
    fn drop(&mut self) {
        let _ = unsafe { self.device.raw.device_wait_idle() };
        // struct fields are dropped in order
        // swapchain, then surface, then device
    }
}
