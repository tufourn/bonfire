use std::sync::Arc;

use anyhow::Result;
use ash::vk;
use ash::vk::SwapchainCreateInfoKHR;

use super::device;
use super::surface;

#[derive(Clone)]
pub struct SwapchainDesc {
    pub old_swapchain: Option<vk::SwapchainKHR>,
    pub format: vk::SurfaceFormatKHR,
    pub vsync: bool,
}

#[derive(Copy, Clone)]
pub struct SwapchainSync {
    pub acquire_semaphore: vk::Semaphore,
    pub present_semaphore: vk::Semaphore,
}

pub struct SwapchainImage {
    pub image: vk::Image,
    pub image_index: u32,
    pub sync: SwapchainSync,
}

pub struct Swapchain {
    pub loader: ash::khr::swapchain::Device,
    pub desc: SwapchainDesc,

    pub raw: vk::SwapchainKHR,

    images: Vec<vk::Image>,

    syncs: Vec<SwapchainSync>,
    sync_index: usize,

    device: Arc<device::Device>,
    surface: Arc<surface::Surface>,
}

impl Swapchain {
    pub fn enumerate_surface_formats(
        device: &Arc<device::Device>,
        surface: &Arc<surface::Surface>,
    ) -> Result<Vec<vk::SurfaceFormatKHR>> {
        unsafe {
            Ok(surface
                .loader
                .get_physical_device_surface_formats(device.physical_device.raw, surface.raw)?)
        }
    }

    pub fn new(
        device: &Arc<device::Device>,
        surface: &Arc<surface::Surface>,
        desc: SwapchainDesc,
    ) -> Result<Self> {
        let loader = ash::khr::swapchain::Device::new(&device.instance.raw, &device.raw);

        let raw = Self::create_raw_swapchain(&loader, device, surface, &desc)?;
        let images = unsafe { loader.get_swapchain_images(raw)? };

        let mut syncs = Vec::with_capacity(images.len());
        let semaphore_create_info = vk::SemaphoreCreateInfo::default();
        for _ in &images {
            let acquire_semaphore =
                unsafe { device.raw.create_semaphore(&semaphore_create_info, None)? };
            let present_semaphore =
                unsafe { device.raw.create_semaphore(&semaphore_create_info, None)? };
            syncs.push(SwapchainSync {
                acquire_semaphore,
                present_semaphore,
            });
        }

        Ok(Self {
            raw,
            loader,
            desc,
            device: device.clone(),
            surface: surface.clone(),
            syncs,
            sync_index: 0,
            images,
        })
    }

    pub fn resize(&mut self) -> Result<()> {
        unsafe { self.device.raw.device_wait_idle()? };

        self.desc.old_swapchain = Some(self.raw);
        let new_swapchain =
            Self::create_raw_swapchain(&self.loader, &self.device, &self.surface, &self.desc)?;
        let new_images = unsafe { self.loader.get_swapchain_images(new_swapchain)? };

        unsafe { self.loader.destroy_swapchain(self.raw, None) };

        self.raw = new_swapchain;
        self.images = new_images;

        Ok(())
    }

    fn create_raw_swapchain(
        loader: &ash::khr::swapchain::Device,
        device: &Arc<device::Device>,
        surface: &Arc<surface::Surface>,
        desc: &SwapchainDesc,
    ) -> Result<vk::SwapchainKHR> {
        let surface_capabilities = unsafe {
            surface
                .loader
                .get_physical_device_surface_capabilities(device.physical_device.raw, surface.raw)?
        };

        let mut extent = surface_capabilities.current_extent;
        extent = vk::Extent2D {
            width: extent.width.clamp(
                surface_capabilities.min_image_extent.width,
                surface_capabilities.max_image_extent.width,
            ),
            height: extent.height.clamp(
                surface_capabilities.min_image_extent.height,
                surface_capabilities.max_image_extent.height,
            ),
        };
        if extent.width == 0 && extent.height == 0 {
            anyhow::bail!("Swapchain extent cannot be zero");
        }

        let mut image_count = surface_capabilities.min_image_count + 1;
        if surface_capabilities.max_image_count > 0
            && image_count > surface_capabilities.max_image_count
        {
            image_count = surface_capabilities.max_image_count;
        }

        let present_modes = unsafe {
            surface.loader.get_physical_device_surface_present_modes(
                device.physical_device.raw,
                surface.raw,
            )?
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
            .image_extent(extent)
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

        let raw = unsafe { loader.create_swapchain(&create_info, None)? };

        println!("Created swapchain: {}x{}", extent.width, extent.height);

        Ok(raw)
    }

    pub fn acquire_next_image(&mut self) -> Result<SwapchainImage> {
        self.sync_index += 1;
        let sync = self.syncs[self.sync_index % self.images.len()];

        let image_index = unsafe {
            self.loader
                .acquire_next_image(
                    self.raw,
                    u64::MAX,
                    sync.acquire_semaphore,
                    vk::Fence::null(),
                )
                .map(|(index, _)| index)?
        };

        Ok(SwapchainImage {
            image: self.images[image_index as usize],
            image_index,
            sync,
        })
    }

    pub fn present_image(&self, swapchain_image: SwapchainImage) {
        let present_info = vk::PresentInfoKHR::default()
            .wait_semaphores(std::slice::from_ref(
                &swapchain_image.sync.present_semaphore,
            ))
            .swapchains(std::slice::from_ref(&self.raw))
            .image_indices(std::slice::from_ref(&swapchain_image.image_index));

        let res = unsafe {
            self.loader
                .queue_present(self.device.graphics_queue.raw, &present_info)
        };
        match res {
            Ok(_) => {}
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) | Err(vk::Result::SUBOPTIMAL_KHR) => {}
            Err(e) => {
                panic!("Failed to present image: {e:?}");
            }
        }
    }
}

impl Drop for Swapchain {
    fn drop(&mut self) {
        unsafe {
            for sync in &self.syncs {
                self.device
                    .raw
                    .destroy_semaphore(sync.acquire_semaphore, None);
                self.device
                    .raw
                    .destroy_semaphore(sync.present_semaphore, None);
            }
            self.loader.destroy_swapchain(self.raw, None);
        }
    }
}
