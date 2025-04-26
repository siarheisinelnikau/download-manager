use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, DeviceId, StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::WindowId,
};

/// eframe 0.33 can leave Poll set after consuming its last redraw deadline.
/// Preserve WaitUntil deadlines and reset only stale Poll after event dispatch.
struct ReactiveHost<T>(T);
impl<T: ApplicationHandler<eframe::UserEvent>> ApplicationHandler<eframe::UserEvent>
    for ReactiveHost<T>
{
    fn resumed(&mut self, e: &ActiveEventLoop) {
        self.0.resumed(e);
    }
    fn suspended(&mut self, e: &ActiveEventLoop) {
        self.0.suspended(e);
    }
    fn new_events(&mut self, e: &ActiveEventLoop, cause: StartCause) {
        self.0.new_events(e, cause);
    }
    fn user_event(&mut self, e: &ActiveEventLoop, event: eframe::UserEvent) {
        self.0.user_event(e, event);
    }
    fn window_event(&mut self, e: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        self.0.window_event(e, id, event);
    }
    fn device_event(&mut self, e: &ActiveEventLoop, id: DeviceId, event: DeviceEvent) {
        self.0.device_event(e, id, event);
    }
    fn about_to_wait(&mut self, e: &ActiveEventLoop) {
        self.0.about_to_wait(e);
        if e.control_flow() == ControlFlow::Poll {
            e.set_control_flow(ControlFlow::Wait);
        }
    }
    fn exiting(&mut self, e: &ActiveEventLoop) {
        self.0.exiting(e);
    }
    fn memory_warning(&mut self, e: &ActiveEventLoop) {
        self.0.memory_warning(e);
    }
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::<eframe::UserEvent>::with_user_event().build()?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1080.0, 740.0])
            .with_min_inner_size([760.0, 540.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    let app = eframe::create_native(
        "DownloadManager",
        options,
        Box::new(|cc| Ok(Box::new(crate::ui::App::new(cc)))),
        &event_loop,
    );
    event_loop.run_app(&mut ReactiveHost(app))?;
    Ok(())
}
