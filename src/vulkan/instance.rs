use log::{debug, error, info, trace, warn};
use std::ffi::CStr;

use anyhow::{Context, Result};
use ash::vk;

#[derive(Default)]
pub struct InstanceBuilder {
    pub required_extensions: &'static [*const i8],
    pub validation_layers: bool,
}

impl InstanceBuilder {
    pub fn build(&self) -> Result<Instance> {
        Instance::new(self)
    }

    pub fn required_extensions(mut self, extensions: &'static [*const i8]) -> Self {
        self.required_extensions = extensions;
        self
    }

    pub fn enable_validation_layers(mut self, should_enable: bool) -> Self {
        self.validation_layers = should_enable;
        self
    }
}

pub struct Instance {
    pub entry: ash::Entry,
    pub raw: ash::Instance,
    pub debug_utils_loader: Option<ash::ext::debug_utils::Instance>,
    pub debug_messenger: Option<vk::DebugUtilsMessengerEXT>,
}

impl Instance {
    fn new(builder: &InstanceBuilder) -> Result<Self> {
        let entry = unsafe { ash::Entry::load().context("Failed to load Entry")? };

        let app_info = vk::ApplicationInfo::default()
            .application_name(c"Bonfire")
            .application_version(0)
            .engine_name(c"No engine")
            .engine_version(0)
            .api_version(vk::make_api_version(0, 1, 3, 0));

        let layers = Self::layers(builder);

        let create_flags = if cfg!(any(target_os = "macos", target_os = "ios")) {
            vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR
        } else {
            vk::InstanceCreateFlags::default()
        };

        let mut enabled_extensions = Self::extensions(builder);
        enabled_extensions.extend_from_slice(builder.required_extensions);

        let create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_extension_names(&enabled_extensions)
            .enabled_layer_names(&layers)
            .flags(create_flags);

        let raw = unsafe {
            entry
                .create_instance(&create_info, None)
                .context("Failed to create instance")?
        };

        let (debug_utils_loader, debug_messenger) = if builder.validation_layers {
            let debug_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
                .message_severity(
                    vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                        | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                        | vk::DebugUtilsMessageSeverityFlagsEXT::INFO,
                )
                .message_type(
                    vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                        | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                        | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
                )
                .pfn_user_callback(Some(vulkan_debug_callback));
            let debug_utils_loader = ash::ext::debug_utils::Instance::new(&entry, &raw);
            let debug_messenger = unsafe {
                debug_utils_loader
                    .create_debug_utils_messenger(&debug_info, None)
                    .context("Failed to create debug messenger")?
            };

            (Some(debug_utils_loader), Some(debug_messenger))
        } else {
            (None, None)
        };

        Ok(Instance {
            entry,
            raw,
            debug_utils_loader,
            debug_messenger,
        })
    }

    fn extensions(builder: &InstanceBuilder) -> Vec<*const i8> {
        let mut extensions = vec![ash::khr::get_physical_device_properties2::NAME.as_ptr()];

        if builder.validation_layers {
            extensions.push(ash::ext::debug_utils::NAME.as_ptr());
        }

        extensions
    }

    fn layers(builder: &InstanceBuilder) -> Vec<*const i8> {
        let mut layers = Vec::new();

        if builder.validation_layers {
            layers.push(c"VK_LAYER_KHRONOS_validation".as_ptr());
        }

        layers
    }
}

extern "system" fn vulkan_debug_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    _message_type: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _user_data: *mut std::os::raw::c_void,
) -> vk::Bool32 {
    unsafe {
        let callback_data = *p_callback_data;
        let log_message = CStr::from_ptr(callback_data.p_message).to_str().unwrap();

        match message_severity {
            vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE => trace!("{log_message}"),
            vk::DebugUtilsMessageSeverityFlagsEXT::INFO => info!("{log_message}"),
            vk::DebugUtilsMessageSeverityFlagsEXT::WARNING => warn!("{log_message}"),
            vk::DebugUtilsMessageSeverityFlagsEXT::ERROR => error!("{log_message}"),
            _ => debug!("{log_message}"),
        }

        vk::FALSE
    }
}
