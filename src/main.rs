use anyhow::Result;
use bonfire::vulkan::{
    RenderBackend, RenderBackendConfig,
    command_ring_buffer::CommandRingBuffer,
    device::FRAMES_IN_FLIGHT,
    pipeline::{self, RasterPipeline, RasterPipelineDesc, ShaderDesc},
    shader_compiler::ShaderCompiler,
};
use log::{info, warn};
use vk_sync::{AccessType, ImageLayout};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

use ash::vk;

struct Renderer {
    render_backend: RenderBackend,
    command_ring_buffer: CommandRingBuffer,
    triangle_pipeline: RasterPipeline,
}

#[derive(Default)]
struct App {
    window: Option<Window>,
    renderer: Option<Renderer>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let window_attributes = Window::default_attributes();
        let window = event_loop
            .create_window(window_attributes)
            .expect("Failed to create window");

        let render_config = RenderBackendConfig {
            validation_layers: true,
            vsync: true,
        };

        let render_backend =
            RenderBackend::new(&window, &render_config).expect("Failed to create render backend");

        let command_ring_buffer = CommandRingBuffer::builder(render_backend.device.clone())
            .num_pools(FRAMES_IN_FLIGHT)
            .primary_buffers_per_pool(1)
            .build()
            .expect("Failed to build command buffer manager");

        let triangle_vert_shader = ShaderCompiler::compile_slang("triangle/triangle_vert.slang")
            .expect("Failed to compile vert shader");
        let triangle_vert = ShaderDesc::new(triangle_vert_shader, pipeline::ShaderStage::Vertex);
        let triangle_frag_shader = ShaderCompiler::compile_slang("triangle/triangle_frag.slang")
            .expect("Failed to compile frag shader");
        let triangle_frag = ShaderDesc::new(triangle_frag_shader, pipeline::ShaderStage::Fragment);

        let triangle_pipeline_desc = RasterPipelineDesc {
            shaders: vec![triangle_vert, triangle_frag],
            color_attachments: vec![vk::Format::B8G8R8A8_SRGB],
        };

        let triangle_pipeline =
            pipeline::create_raster_pipeline(render_backend.device.clone(), triangle_pipeline_desc)
                .unwrap();

        self.window = Some(window);
        self.renderer = Some(Renderer {
            render_backend,
            command_ring_buffer,
            triangle_pipeline,
        });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                info!("The close button was pressed; stopping");
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                let renderer = self.renderer.as_mut().unwrap();
                let render_backend = &mut renderer.render_backend;
                let vk_device = &render_backend.device.raw;
                let swapchain = &mut render_backend.swapchain;

                render_backend.device.begin_frame().expect("begin frame");

                let swapchain_image = swapchain.acquire_next_image().expect("acquire next image");

                renderer
                    .command_ring_buffer
                    .reset_pool(0)
                    .expect("failed to reset command pool");

                let command_buffer = renderer.command_ring_buffer.get_next_primary_buffer(0);

                unsafe {
                    vk_device
                        .reset_command_buffer(
                            command_buffer,
                            vk::CommandBufferResetFlags::default(),
                        )
                        .expect("reset command buffer");
                };

                let begin_info = vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
                unsafe {
                    vk_device
                        .begin_command_buffer(command_buffer, &begin_info)
                        .expect("begin command buffer");
                    let image_barrier = vk_sync::ImageBarrier {
                        previous_accesses: &[AccessType::Nothing],
                        next_accesses: &[AccessType::ColorAttachmentWrite],
                        previous_layout: ImageLayout::General,
                        next_layout: ImageLayout::Optimal,
                        discard_contents: true,
                        src_queue_family_index: render_backend.device.graphics_queue.family,
                        dst_queue_family_index: render_backend.device.graphics_queue.family,
                        image: swapchain_image.image,
                        range: vk::ImageSubresourceRange {
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            base_mip_level: 0,
                            level_count: vk::REMAINING_MIP_LEVELS,
                            base_array_layer: 0,
                            layer_count: vk::REMAINING_ARRAY_LAYERS,
                        },
                    };

                    vk_sync::cmd::pipeline_barrier(
                        &render_backend.device.raw,
                        command_buffer,
                        None,
                        &[],
                        &[image_barrier],
                    );
                }

                // draw
                unsafe {
                    vk_device.cmd_bind_pipeline(
                        command_buffer,
                        vk::PipelineBindPoint::GRAPHICS,
                        renderer.triangle_pipeline.pipeline,
                    );

                    let extent = swapchain.get_extent();
                    let height = extent.height;
                    let width = extent.width;
                    vk_device.cmd_set_viewport(
                        command_buffer,
                        0,
                        &[vk::Viewport {
                            x: 0.0,
                            y: height as _,
                            width: width as _,
                            height: -(height as f32),
                            min_depth: 0.0,
                            max_depth: 1.0,
                        }],
                    );
                    vk_device.cmd_set_scissor(
                        command_buffer,
                        0,
                        &[vk::Rect2D {
                            offset: vk::Offset2D { x: 0, y: 0 },
                            extent: vk::Extent2D {
                                width: width as _,
                                height: height as _,
                            },
                        }],
                    );

                    let color_attachment = vk::RenderingAttachmentInfo::default()
                        .image_view(swapchain_image.image_view)
                        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                        .load_op(vk::AttachmentLoadOp::CLEAR)
                        .store_op(vk::AttachmentStoreOp::STORE)
                        .clear_value(vk::ClearValue::default());

                    let rendering_info = vk::RenderingInfo::default()
                        .layer_count(1)
                        .render_area(
                            vk::Rect2D::default()
                                .offset(vk::Offset2D { x: 0, y: 0 })
                                .extent(extent),
                        )
                        .color_attachments(std::slice::from_ref(&color_attachment));

                    vk_device.cmd_begin_rendering(command_buffer, &rendering_info);

                    vk_device.cmd_draw(command_buffer, 3, 1, 0, 0);

                    vk_device.cmd_end_rendering(command_buffer);
                };

                let image_barrier = vk_sync::ImageBarrier {
                    previous_accesses: &[AccessType::ColorAttachmentWrite],
                    next_accesses: &[AccessType::Present],
                    previous_layout: ImageLayout::General,
                    next_layout: ImageLayout::Optimal,
                    discard_contents: true,
                    src_queue_family_index: render_backend.device.graphics_queue.family,
                    dst_queue_family_index: render_backend.device.graphics_queue.family,
                    image: swapchain_image.image,
                    range: vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: vk::REMAINING_MIP_LEVELS,
                        base_array_layer: 0,
                        layer_count: vk::REMAINING_ARRAY_LAYERS,
                    },
                };

                vk_sync::cmd::pipeline_barrier(
                    &render_backend.device.raw,
                    command_buffer,
                    None,
                    &[],
                    &[image_barrier],
                );

                unsafe {
                    vk_device
                        .end_command_buffer(command_buffer)
                        .expect("end command buffer");
                }

                let mut wait_semaphores = vec![
                    vk::SemaphoreSubmitInfo::default()
                        .semaphore(swapchain_image.sync.acquire_semaphore)
                        .stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT),
                ];
                if render_backend.device.absolute_frame_index() >= FRAMES_IN_FLIGHT {
                    wait_semaphores.push(
                        vk::SemaphoreSubmitInfo::default()
                            .semaphore(render_backend.device.graphics_timeline_semaphore)
                            .value(
                                (render_backend.device.absolute_frame_index() - FRAMES_IN_FLIGHT
                                    + 1) as u64,
                            )
                            .stage_mask(vk::PipelineStageFlags2::TOP_OF_PIPE),
                    );
                }

                let signal_semaphores = [
                    vk::SemaphoreSubmitInfo::default()
                        .semaphore(swapchain_image.sync.present_semaphore)
                        .stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT),
                    vk::SemaphoreSubmitInfo::default()
                        .semaphore(render_backend.device.graphics_timeline_semaphore)
                        .value((render_backend.device.absolute_frame_index() + 1) as u64)
                        .stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT),
                ];

                let command_buffer_submit_info =
                    vk::CommandBufferSubmitInfo::default().command_buffer(command_buffer);

                let submit_info = vk::SubmitInfo2::default()
                    .wait_semaphore_infos(&wait_semaphores)
                    .signal_semaphore_infos(&signal_semaphores)
                    .command_buffer_infos(std::slice::from_ref(&command_buffer_submit_info));

                unsafe {
                    render_backend
                        .device
                        .raw
                        .queue_submit2(
                            render_backend.device.graphics_queue.raw,
                            std::slice::from_ref(&submit_info),
                            vk::Fence::null(),
                        )
                        .expect("queue_submit2");
                };

                render_backend.swapchain.present_image(swapchain_image);

                render_backend.device.finish_frame();

                self.window.as_ref().unwrap().request_redraw();
            }
            WindowEvent::Resized(new_size) => {
                warn!("Resize requested: {}x{}", new_size.width, new_size.height);
                self.renderer
                    .as_mut()
                    .unwrap()
                    .render_backend
                    .swapchain
                    .rebuild()
                    .expect("Failed to rebuild swapchain");
            }
            _ => (),
        }
    }
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

    let mut app = App::default();
    event_loop.run_app(&mut app)?;

    Ok(())
}
