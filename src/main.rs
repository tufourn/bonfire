use anyhow::Result;
use bonfire::vulkan::{
    RenderBackend, RenderBackendConfig, command_ring_buffer::CommandRingBuffer,
    device::FRAMES_IN_FLIGHT,
};

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

        self.window = Some(window);
        self.renderer = Some(Renderer {
            render_backend,
            command_ring_buffer,
        });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                println!("The close button was pressed; stopping");
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                let renderer = self.renderer.as_mut().unwrap();
                let render_backend = &mut renderer.render_backend;
                let vk_device = &render_backend.device.raw;

                render_backend.device.begin_frame().expect("begin frame");

                let swapchain_image = render_backend
                    .swapchain
                    .acquire_next_image()
                    .expect("acquire next image");

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
                }

                // draw

                let image_barrier = vk::ImageMemoryBarrier2::default()
                    .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                    .src_access_mask(vk::AccessFlags2::MEMORY_WRITE)
                    .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                    .dst_access_mask(vk::AccessFlags2::MEMORY_WRITE | vk::AccessFlags2::MEMORY_READ)
                    .old_layout(vk::ImageLayout::UNDEFINED)
                    .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: vk::REMAINING_MIP_LEVELS,
                        base_array_layer: 0,
                        layer_count: vk::REMAINING_ARRAY_LAYERS,
                    })
                    .image(swapchain_image.image);
                let dependency_info = vk::DependencyInfo::default()
                    .image_memory_barriers(std::slice::from_ref(&image_barrier));

                unsafe { vk_device.cmd_pipeline_barrier2(command_buffer, &dependency_info) };

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
                println!("Resize requested: {}x{}", new_size.width, new_size.height);
                self.renderer
                    .as_mut()
                    .unwrap()
                    .render_backend
                    .swapchain
                    .resize()
                    .expect("Failed to resize swapchain");
            }
            _ => (),
        }
    }
}

fn main() -> Result<()> {
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

    let mut app = App::default();
    event_loop.run_app(&mut app)?;

    Ok(())
}
