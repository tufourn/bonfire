use anyhow::Result;
use ash::vk;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::sync::Arc;

pub struct Surface {
    pub raw: vk::SurfaceKHR,
    pub loader: ash::khr::surface::Instance,
}

impl Surface {
    pub fn new(
        device: &Arc<super::device::Device>,
        window: &(impl HasDisplayHandle + HasWindowHandle),
    ) -> Result<Self> {
        let raw = unsafe {
            ash_window::create_surface(
                &device.instance.entry,
                &device.instance.raw,
                window.display_handle().unwrap().as_raw(),
                window.window_handle().unwrap().as_raw(),
                None,
            )?
        };

        let loader = ash::khr::surface::Instance::new(&device.instance.entry, &device.instance.raw);
        Ok(Self { raw, loader })
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            self.loader.destroy_surface(self.raw, None);
        }
    }
}
