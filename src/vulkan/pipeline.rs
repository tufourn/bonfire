use std::{collections::btree_map::Entry, ffi::CString, sync::Arc};

use super::device;
use super::shader_compiler;
use anyhow::{Context, Result};
use ash::vk;
use bytes::Bytes;
use log::info;

pub const MAX_DESCRIPTOR_SETS: usize = 4;

pub enum ShaderStage {
    Vertex,
    Fragment,
}

pub struct ShaderDesc {
    name: String,
    spirv: Bytes,
    stage: ShaderStage,
    entry_point: CString,
}

impl ShaderDesc {
    pub fn new(compiled_shader: shader_compiler::CompiledShader, stage: ShaderStage) -> Self {
        Self {
            name: compiled_shader.name,
            spirv: compiled_shader.spirv,
            stage,
            // TODO: specify entry point function name
            entry_point: c"main".into(),
        }
    }
}

pub struct RasterPipelineDesc {
    pub shaders: Vec<ShaderDesc>,
    pub color_attachments: Vec<vk::Format>,
}

pub struct RasterPipeline {
    device: Arc<device::Device>,
    pub pipeline: vk::Pipeline,
    layout: vk::PipelineLayout,
    set_layouts: Vec<vk::DescriptorSetLayout>,
}

pub fn create_raster_pipeline(
    device: Arc<device::Device>,
    pipeline_desc: RasterPipelineDesc,
) -> Result<RasterPipeline> {
    let shaders = pipeline_desc.shaders;
    let reflection = shaders
        .iter()
        .map(|shader| {
            rspirv_reflect::Reflection::new_from_spirv(&shader.spirv)
                .with_context(|| format!("Failed to reflect {}", shader.name))
        })
        .collect::<Result<Vec<_>>>()?;

    let descriptor_sets = reflection
        .iter()
        .map(|reflection| {
            reflection
                .get_descriptor_sets()
                .context("Failed to get descriptor set")
        })
        .collect::<Result<Vec<_>>>()?;

    let mut descriptor_sets = descriptor_sets.into_iter();
    let mut merged_sets = descriptor_sets.next().unwrap_or_default();
    for set in descriptor_sets {
        for (set_index, set_bindings) in set.into_iter() {
            match merged_sets.entry(set_index) {
                Entry::Occupied(mut existing_set) => {
                    let occupied_entry = existing_set.get_mut();
                    for (binding_index, binding) in set_bindings {
                        match occupied_entry.entry(binding_index) {
                            Entry::Occupied(occupied_binding) => {
                                let occupied_binding = occupied_binding.get();
                                assert_eq!(occupied_binding.ty, binding.ty);
                                assert_eq!(occupied_binding.name, binding.name);
                            }
                            Entry::Vacant(vacant_binding) => {
                                vacant_binding.insert(binding);
                            }
                        }
                    }
                }
                Entry::Vacant(vacant_set) => {
                    vacant_set.insert(set_bindings);
                }
            }
        }
    }

    let set_count = merged_sets
        .iter()
        .map(|(set_index, _)| set_index + 1)
        .max()
        .unwrap_or(0);

    let mut set_layouts: Vec<vk::DescriptorSetLayout> = Vec::with_capacity(set_count as usize);

    for set_index in 0..set_count {
        if let Some(set_bindings) = merged_sets.get(&set_index) {
            let mut bindings: Vec<vk::DescriptorSetLayoutBinding> =
                Vec::with_capacity(set_bindings.len());
            let binding_flags: Vec<vk::DescriptorBindingFlags> =
                vec![vk::DescriptorBindingFlags::PARTIALLY_BOUND; set_bindings.len()];
            let layout_create_flags = vk::DescriptorSetLayoutCreateFlags::empty();

            use rspirv_reflect::DescriptorType as BindType;
            for (binding_index, binding) in set_bindings {
                match binding.ty {
                    BindType::UNIFORM_BUFFER => {
                        info!("Found uniform buffer: set({set_index}), binding({binding_index})");
                        bindings.push(
                            vk::DescriptorSetLayoutBinding::default()
                                .binding(*binding_index)
                                .descriptor_count(1)
                                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                                .stage_flags(vk::ShaderStageFlags::ALL),
                        );
                    }
                    _ => todo!(),
                }
            }

            let mut binding_flags_create_info =
                vk::DescriptorSetLayoutBindingFlagsCreateInfo::default()
                    .binding_flags(&binding_flags);

            let set_layout_create_info = vk::DescriptorSetLayoutCreateInfo::default()
                .flags(layout_create_flags)
                .bindings(&bindings)
                .push_next(&mut binding_flags_create_info);

            let set_layout = unsafe {
                device
                    .raw
                    .create_descriptor_set_layout(&set_layout_create_info, None)?
            };

            set_layouts.push(set_layout);
        }
    }

    let push_constant_ranges = reflection
        .iter()
        .map(|shader| {
            shader
                .get_push_constant_range()
                .context("Failed to get push constant range")
        })
        .filter_map(Result::ok)
        .flatten()
        .map(|pc| {
            vk::PushConstantRange::default()
                .stage_flags(vk::ShaderStageFlags::ALL)
                .size(pc.size)
                .offset(pc.offset)
        })
        .collect::<Vec<_>>();

    let pipeline_layout_create_info = vk::PipelineLayoutCreateInfo::default()
        .set_layouts(&set_layouts)
        .push_constant_ranges(&push_constant_ranges);

    let pipeline_layout = unsafe {
        device
            .raw
            .create_pipeline_layout(&pipeline_layout_create_info, None)?
    };

    let shader_stages = shaders
        .iter()
        .map(|shader| {
            let stage = match shader.stage {
                ShaderStage::Vertex => vk::ShaderStageFlags::VERTEX,
                ShaderStage::Fragment => vk::ShaderStageFlags::FRAGMENT,
            };

            let module_create_info = vk::ShaderModuleCreateInfo {
                code_size: shader.spirv.len(),
                p_code: shader.spirv.as_ptr() as *const u32,
                ..Default::default()
            };

            let module = unsafe {
                device
                    .raw
                    .create_shader_module(&module_create_info, None)
                    .with_context(|| format!("Failed to create shader module {}", shader.name))?
            };

            Ok(vk::PipelineShaderStageCreateInfo::default()
                .stage(stage)
                .module(module)
                .name(&shader.entry_point))
        })
        .collect::<Result<Vec<_>>>()?;

    // TODO: vertex input & pvp
    let vertex_input_state = vk::PipelineVertexInputStateCreateInfo::default();

    let input_assembly_state = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);

    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .scissor_count(1)
        .viewport_count(1);

    // TODO: hardcoded for testing, allow changes later
    let rasterization_state = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0);

    let multisample_state = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);

    let color_blend_attachment_state = vk::PipelineColorBlendAttachmentState {
        blend_enable: vk::TRUE,
        src_color_blend_factor: vk::BlendFactor::SRC_ALPHA,
        dst_color_blend_factor: vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
        color_blend_op: vk::BlendOp::ADD,
        src_alpha_blend_factor: vk::BlendFactor::ONE,
        dst_alpha_blend_factor: vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
        alpha_blend_op: vk::BlendOp::ADD,
        color_write_mask: vk::ColorComponentFlags::RGBA,
    };

    let color_blend_state = vk::PipelineColorBlendStateCreateInfo::default()
        .attachments(std::slice::from_ref(&color_blend_attachment_state));

    let dynamic_state = vk::PipelineDynamicStateCreateInfo::default()
        .dynamic_states(&[vk::DynamicState::SCISSOR, vk::DynamicState::VIEWPORT]);

    let mut dynamic_rendering = vk::PipelineRenderingCreateInfo::default()
        .color_attachment_formats(&pipeline_desc.color_attachments);

    let pipeline_create_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&shader_stages)
        .vertex_input_state(&vertex_input_state)
        .input_assembly_state(&input_assembly_state)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterization_state)
        .multisample_state(&multisample_state)
        .color_blend_state(&color_blend_state)
        .dynamic_state(&dynamic_state)
        .layout(pipeline_layout)
        .push_next(&mut dynamic_rendering);

    let pipeline = unsafe {
        device
            .raw
            .create_graphics_pipelines(
                vk::PipelineCache::null(),
                &std::slice::from_ref(&pipeline_create_info),
                None,
            )
            .map_err(|_| anyhow::anyhow!("Failed to create graphics pipeline"))?[0]
    };

    // shader modules can be destroyed after pipeline has been created
    shader_stages.iter().for_each(|shader_stage| {
        unsafe { device.raw.destroy_shader_module(shader_stage.module, None) };
    });

    Ok(RasterPipeline {
        device: device,
        pipeline,
        layout: pipeline_layout,
        set_layouts,
    })
}

impl Drop for RasterPipeline {
    fn drop(&mut self) {
        unsafe {
            self.set_layouts.iter().for_each(|layout| {
                self.device.raw.destroy_descriptor_set_layout(*layout, None);
            });
            self.device.raw.destroy_pipeline_layout(self.layout, None);
            self.device.raw.destroy_pipeline(self.pipeline, None);
        }
    }
}
