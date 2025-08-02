use anyhow::Result;
use bonfire::vulkan::{RenderBackend, RenderBackendConfig};

use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

#[derive(Default)]
struct App {
    window: Option<Window>,
    render_backend: Option<RenderBackend>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let window_attributes = Window::default_attributes();
        let window = event_loop
            .create_window(window_attributes)
            .expect("Failed to create window");

        let render_config = RenderBackendConfig {
            swapchain_extent: [800, 600],
            validation_layers: true,
            vsync: true,
        };
        self.render_backend = Some(
            RenderBackend::new(&window, &render_config).expect("Failed to create render backend"),
        );

        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                println!("The close button was pressed; stopping");
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                // Redraw the application.
                //
                // It's preferable for applications that do not render continuously to render in
                // this event rather than in AboutToWait, since rendering in here allows
                // the program to gracefully handle redraws requested by the OS.

                // Draw.

                // Queue a RedrawRequested event.
                //
                // You only need to call this if you've determined that you need to redraw in
                // applications which do not always need to. Applications that redraw continuously
                // can render here instead.
                self.window.as_ref().unwrap().request_redraw();
            }
            WindowEvent::Resized(new_size) => {
                println!("Resize requested: {}x{}", new_size.width, new_size.height);
                let new_size = [new_size.width, new_size.height];
                self.render_backend
                    .as_mut()
                    .unwrap()
                    .resize_swapchain(&new_size)
                    .unwrap();
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
