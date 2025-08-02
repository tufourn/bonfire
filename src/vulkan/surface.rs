use anyhow::Result;
use ash::vk;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

pub struct Surface {
    pub raw: vk::SurfaceKHR,
    pub loader: ash::khr::surface::Instance,
}

impl Surface {
    pub fn new(
        instance: &super::instance::Instance,
        window: &(impl HasDisplayHandle + HasWindowHandle),
    ) -> Result<Self> {
        let raw = unsafe {
            ash_window::create_surface(
                &instance.entry,
                &instance.raw,
                window.display_handle().unwrap().as_raw(),
                window.window_handle().unwrap().as_raw(),
                None,
            )?
        };

        let loader = ash::khr::surface::Instance::new(&instance.entry, &instance.raw);
        Ok(Self { raw, loader })
    }
}
