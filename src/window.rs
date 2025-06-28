use egui_software_backend::{BufferMutRef, ColorFieldOrder, EguiSoftwareRender};
use std::{
    fs::File,
    io::BufWriter,
    num::NonZeroU32,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    window::{Window, WindowId},
};

enum UserEvent {
    Tray(crate::tray::Command),
    BackendChanged,
    Repaint { when: Instant, pass: u64 },
}

#[derive(Clone, Copy, Default, serde::Serialize)]
struct Counters {
    native_changes: u64,
    layouts: u64,
    rasters: u64,
    presents: u64,
    native_redraws: u64,
}
impl Counters {
    fn since(self, old: Self) -> Self {
        Self {
            native_changes: self.native_changes - old.native_changes,
            layouts: self.layouts - old.layouts,
            rasters: self.rasters - old.rasters,
            presents: self.presents - old.presents,
            native_redraws: self.native_redraws - old.native_redraws,
        }
    }
}

#[derive(serde::Serialize)]
struct FrameTiming {
    ui_ms: f64,
    raster_ms: f64,
    present_ms: f64,
    total_ms: f64,
}

struct Capture {
    path: PathBuf,
    settings: bool,
    popup: bool,
    opened: bool,
    saved: bool,
    ready_frames: usize,
    deadline: Option<Instant>,
    frames: Vec<FrameTiming>,
}
impl Capture {
    fn new(path: PathBuf, view: &str) -> Self {
        Self {
            path,
            settings: view == "settings",
            popup: view == "popup",
            opened: false,
            saved: false,
            ready_frames: 0,
            deadline: Some(Instant::now() + Duration::from_secs(5)),
            frames: Vec::new(),
        }
    }
    fn save(
        &mut self,
        pixels: &[u32],
        width: u32,
        height: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let temporary = self.path.with_extension("png.partial");
        {
            let mut encoder =
                png::Encoder::new(BufWriter::new(File::create(&temporary)?), width, height);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header()?;
            let mut rgb = Vec::with_capacity(pixels.len() * 3);
            for pixel in pixels {
                rgb.extend_from_slice(&[(pixel >> 16) as u8, (pixel >> 8) as u8, *pixel as u8]);
            }
            writer.write_image_data(&rgb)?;
            writer.finish()?;
        }
        std::fs::OpenOptions::new()
            .write(true)
            .open(&temporary)?
            .sync_all()?;
        download_manager::storage::replace(&temporary, &self.path)?;
        self.saved = true;
        self.deadline = None;
        Ok(())
    }
    fn report(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut times: Vec<_> = self.frames.iter().map(|f| f.total_ms).collect();
        times.sort_by(f64::total_cmp);
        let at = |p: usize| {
            times
                .get(times.len().saturating_sub(1) * p / 100)
                .copied()
                .unwrap_or_default()
        };
        let report = serde_json::json!({"renderer":"CPU rasterizer / Win32 GDI", "scope":"Native client area; actual presented pixels", "captured":self.saved,
            "frame_count":self.frames.len(), "median_total_ms":at(50), "p95_total_ms":at(95), "frames":self.frames});
        std::fs::write(
            self.path.with_extension("metrics.json"),
            serde_json::to_vec_pretty(&report)?,
        )?;
        Ok(())
    }
}

const PHASES: [(&str, u64); 5] = [
    ("visible_settle", 2),
    ("visible_idle", 5),
    ("hidden", 3),
    ("restored_settle", 2),
    ("restored_idle", 5),
];
struct Diagnostic {
    path: PathBuf,
    settings: bool,
    popup: bool,
    tray: bool,
    activation: Option<std::process::Child>,
    opened: bool,
    phase: usize,
    started: Instant,
    next: Option<Instant>,
    baseline: Counters,
    cpu: Option<f64>,
    samples: Vec<serde_json::Value>,
    repaint_causes: std::collections::BTreeMap<String, u64>,
    native_events: std::collections::BTreeMap<String, u64>,
    idle_retries: u8,
}
impl Diagnostic {
    fn new(path: PathBuf, view: &str) -> Self {
        Self {
            path,
            settings: view == "settings",
            popup: view == "popup",
            tray: view == "tray",
            activation: None,
            opened: false,
            phase: 0,
            started: Instant::now(),
            next: None,
            baseline: Counters::default(),
            cpu: None,
            samples: Vec::new(),
            repaint_causes: Default::default(),
            native_events: Default::default(),
            idle_retries: 0,
        }
    }
    fn start(&mut self, counters: Counters) {
        self.started = Instant::now();
        self.next = Some(self.started + Duration::from_secs(PHASES[self.phase].1));
        self.baseline = counters;
        self.cpu = cpu_seconds();
    }
    fn advance(&mut self, counters: Counters, window: &Window) -> bool {
        let elapsed = self.started.elapsed().as_secs_f64();
        let cpu = cpu_seconds();
        let changed =
            matches!(self.phase, 1 | 4) && counters.since(self.baseline).native_changes > 0;
        let state = if changed {
            PHASES[self.phase].0.replace("_idle", "_native_activity")
        } else {
            PHASES[self.phase].0.to_owned()
        };
        self.samples.push(serde_json::json!({"state":state, "seconds":elapsed, "counts":counters.since(self.baseline),
            "cpu_one_core_percent": self.cpu.zip(cpu).map(|(before,after)|100.0*(after-before)/elapsed), "minimized":window.is_minimized(), "visible":window.is_visible()}));
        if changed && self.idle_retries == 0 {
            self.idle_retries = 1;
            self.start(counters);
            return false;
        }
        self.idle_retries = 0;
        self.phase += 1;
        if self.phase == PHASES.len() {
            self.next = None;
            return true;
        }
        if self.phase == 2 {
            window.set_visible(false);
        }
        if self.phase == 3 {
            window.set_visible(true);
        }
        self.start(counters);
        false
    }
    fn report(&self) -> Result<(), Box<dyn std::error::Error>> {
        let quiet_visible: Vec<_> = self
            .samples
            .iter()
            .filter(|s| {
                matches!(s["state"].as_str(), Some("visible_idle" | "restored_idle"))
                    && s["counts"]["native_changes"] == 0
            })
            .collect();
        let gate = ["visible_idle", "restored_idle"]
            .iter()
            .all(|state| quiet_visible.iter().any(|s| s["state"] == *state))
            && quiet_visible.iter().all(|s| {
                s["counts"]["layouts"] == 0 && s["counts"]["rasters"] == 0 && s["visible"] == true
            })
            && self.samples.iter().any(|s| {
                s["state"] == "hidden"
                    && s["visible"] == false
                    && s["counts"]["layouts"] == 0
                    && s["counts"]["rasters"] == 0
                    && s["counts"]["presents"] == 0
            });
        let report = serde_json::json!({"renderer":"egui CPU rasterizer -> softbuffer Win32 GDI BitBlt", "executable":std::env::current_exe()?, "pid":std::process::id(),
            "settings_page":self.settings,"popup":self.popup,"tray_lifecycle":self.tray,"complete":self.phase==PHASES.len(),"idle_hidden_render_gate":gate,"samples":self.samples,"repaint_causes":self.repaint_causes,"native_events":self.native_events,
            "limitations":"No keyboard/mouse automation or GPU telemetry. The disposable diagnostic window is not activated and disables its own cursor hit-testing to isolate idle samples from unrelated pointer movement. Normal input behavior is unchanged. Lifecycle transitions are initiated by the application; perceived stutter still needs user confirmation."});
        std::fs::write(&self.path, serde_json::to_vec_pretty(&report)?)?;
        Ok(())
    }
}

fn cpu_seconds() -> Option<f64> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::{
            Foundation::FILETIME,
            System::Threading::{GetCurrentProcess, GetProcessTimes},
        };
        let mut times = [FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        }; 4];
        // Four distinct, initialized FILETIME outputs remain valid for the synchronous call.
        let ok = unsafe {
            GetProcessTimes(
                GetCurrentProcess(),
                &mut times[0],
                &mut times[1],
                &mut times[2],
                &mut times[3],
            )
        };
        if ok == 0 {
            return None;
        }
        let value = |t: FILETIME| {
            ((t.dwHighDateTime as u64) << 32 | t.dwLowDateTime as u64) as f64 / 10_000_000.0
        };
        Some(value(times[2]) + value(times[3]))
    }
    #[cfg(not(windows))]
    {
        None
    }
}

struct CpuHost {
    tray: Option<crate::tray::Tray>,
    tray_sender: Arc<dyn Fn(crate::tray::Command) + Send + Sync>,
    instance: Option<crate::instance::Instance>,
    popup: bool,
    popup_focused: bool,
    last_popup_dismissed: Option<Instant>,
    full_geometry: Option<(
        Option<winit::dpi::PhysicalPosition<i32>>,
        winit::dpi::PhysicalSize<u32>,
        bool,
    )>,
    popup_anchor: Option<(f64, f64)>,
    quitting: bool,
    wake: Arc<dyn Fn() + Send + Sync>,
    surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,
    window: Option<Arc<Window>>,
    input: Option<egui_winit::State>,
    ctx: egui::Context,
    app: Option<crate::ui::App>,
    raster: EguiSoftwareRender,
    deadline: Option<Instant>,
    dirty: bool,
    occluded: bool,
    retained: Vec<u32>,
    retained_size: (u32, u32),
    counters: Counters,
    error: Option<String>,
    capture: Option<Capture>,
    diagnostic: Option<Diagnostic>,
    size: (u32, u32),
}
impl CpuHost {
    fn new(proxy: EventLoopProxy<UserEvent>) -> Self {
        let tray_proxy = proxy.clone();
        let tray_sender: Arc<dyn Fn(crate::tray::Command) + Send + Sync> =
            Arc::new(move |command| {
                let _ = tray_proxy.send_event(UserEvent::Tray(command));
            });
        let backend_proxy = proxy.clone();
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            let _ = backend_proxy.send_event(UserEvent::BackendChanged);
        });
        let ctx = egui::Context::default();
        ctx.set_request_repaint_callback(move |request| {
            if request.viewport_id == egui::ViewportId::ROOT
                && let Some(when) = Instant::now().checked_add(request.delay)
            {
                let _ = proxy.send_event(UserEvent::Repaint {
                    when,
                    pass: request.current_cumulative_pass_nr,
                });
            }
        });
        Self {
            tray: None,
            tray_sender,
            instance: None,
            popup: false,
            popup_focused: false,
            last_popup_dismissed: None,
            full_geometry: None,
            popup_anchor: None,
            quitting: false,
            wake,
            surface: None,
            window: None,
            input: None,
            ctx,
            app: None,
            raster: EguiSoftwareRender::new(ColorFieldOrder::Bgra),
            deadline: None,
            dirty: true,
            occluded: false,
            retained: Vec::new(),
            retained_size: (0, 0),
            counters: Counters::default(),
            error: None,
            capture: None,
            diagnostic: None,
            size: (960, 680),
        }
    }
    fn hidden(&self) -> bool {
        self.occluded
            || self
                .window
                .as_ref()
                .is_some_and(|w| w.is_minimized() == Some(true) || w.is_visible() == Some(false))
    }
    fn schedule(&mut self, when: Instant) {
        self.dirty = true;
        if !self.hidden() {
            self.deadline = Some(self.deadline.map_or(when, |old| old.min(when)));
        }
    }
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: impl std::fmt::Display) {
        self.error = Some(error.to_string());
        event_loop.exit();
    }
    fn hide_to_tray(&mut self) {
        if self.popup
            && self
                .window
                .as_ref()
                .is_some_and(|w| w.is_visible() == Some(true))
        {
            self.last_popup_dismissed = Some(Instant::now());
        }
        if let Some(window) = &self.window {
            window.set_visible(false);
        }
        self.popup_focused = false;
        self.deadline = None;
    }
    fn restore_full(&mut self) {
        let Some(window) = &self.window else {
            return;
        };
        if self.popup {
            self.popup = false;
            window.set_decorations(true);
            window.set_resizable(true);
            window.set_window_level(winit::window::WindowLevel::Normal);
            window.set_min_inner_size(Some(LogicalSize::new(760, 540)));
            #[cfg(windows)]
            {
                use winit::platform::windows::WindowExtWindows;
                window.set_skip_taskbar(false);
            }
            if let Some((position, size, maximized)) = self.full_geometry.take() {
                if let Some(position) = position {
                    window.set_outer_position(position);
                }
                let _ = window.request_inner_size(size);
                window.set_maximized(maximized);
            }
            self.ctx.memory_mut(|m| m.reset_areas());
        }
        window.set_visible(true);
        if window.is_minimized() == Some(true) {
            window.set_minimized(false);
        }
        if self.diagnostic.is_none() {
            window.focus_window();
        }
        self.occluded = false;
        self.schedule(Instant::now());
    }
    fn place_popup(&self) {
        let (Some(window), Some(anchor)) = (&self.window, self.popup_anchor) else {
            return;
        };
        let work = crate::tray::work_area(anchor).unwrap_or((0., 0., 1920., 1080.));
        let scale = window.scale_factor();
        let wanted = self.app.as_ref().unwrap().popup_size();
        let width = (wanted.0 as f64)
            .min((work.2 - work.0 - 16.) / scale)
            .max(240.);
        let height = (wanted.1 as f64)
            .min((work.3 - work.1 - 16.) / scale)
            .max(220.);
        let size = LogicalSize::new(width, height).to_physical::<u32>(scale);
        if window.inner_size() != size {
            let _ = window.request_inner_size(size);
        }
        let (x, y) =
            crate::tray::popup_origin(anchor, (size.width as f64, size.height as f64), work);
        let position = winit::dpi::PhysicalPosition::new(x, y);
        if window.outer_position().ok() != Some(position) {
            window.set_outer_position(position);
        }
    }
    fn dispatch_tray(&mut self, event_loop: &ActiveEventLoop, command: crate::tray::Command) {
        use crate::tray::Command as T;
        match command {
            T::Popup { x, y } => {
                // Windows can dismiss the popup on focus loss before delivering the icon's mouse-up.
                if self.popup
                    && self.hidden()
                    && self
                        .last_popup_dismissed
                        .take()
                        .is_some_and(|at| at.elapsed() < Duration::from_millis(200))
                {
                    return;
                }
                if self.popup
                    && self
                        .window
                        .as_ref()
                        .is_some_and(|w| w.is_visible() == Some(true))
                {
                    self.hide_to_tray();
                    return;
                }
                let Some(window) = &self.window else {
                    return;
                };
                if !self.popup {
                    let mut size = window.inner_size();
                    if size.width < 760 || size.height < 540 {
                        size = winit::dpi::PhysicalSize::new(960, 680);
                    }
                    self.full_geometry =
                        Some((window.outer_position().ok(), size, window.is_maximized()));
                }
                self.popup = true;
                self.popup_focused = false;
                self.popup_anchor = Some((x, y));
                window.set_maximized(false);
                window.set_minimized(false);
                window.set_min_inner_size(None::<LogicalSize<u32>>);
                window.set_decorations(false);
                window.set_resizable(false);
                window.set_window_level(winit::window::WindowLevel::AlwaysOnTop);
                #[cfg(windows)]
                {
                    use winit::platform::windows::WindowExtWindows;
                    window.set_skip_taskbar(true);
                }
                let (width, height) = self.app.as_ref().unwrap().popup_size();
                let work = crate::tray::work_area((x, y)).unwrap_or((0., 0., 1920., 1080.));
                let scale = window.scale_factor();
                let width = (width as f64)
                    .min((work.2 - work.0 - 16.) / scale)
                    .max(240.);
                let height = (height as f64)
                    .min((work.3 - work.1 - 16.) / scale)
                    .max(220.);
                let _ = window.request_inner_size(LogicalSize::new(width, height));
                window.set_visible(true);
                self.place_popup();
                if self.diagnostic.is_none() {
                    window.focus_window();
                }
                self.occluded = false;
                self.ctx.memory_mut(|m| m.reset_areas());
                self.schedule(Instant::now());
            }
            T::Queue => {
                self.app.as_mut().unwrap().show_queue();
                self.restore_full();
            }
            T::Settings => {
                self.app.as_mut().unwrap().open_settings();
                self.restore_full();
            }
            T::NewDownload => {
                self.app.as_mut().unwrap().show_new_download();
                self.restore_full();
            }
            T::PauseAll => self
                .app
                .as_mut()
                .unwrap()
                .command(download_manager::manager::Command::PauseAll),
            T::ResumeAll => self
                .app
                .as_mut()
                .unwrap()
                .command(download_manager::manager::Command::ResumeAll),
            T::Quit => {
                self.quitting = true;
                if self.app.as_mut().unwrap().request_close() {
                    event_loop.exit();
                }
            }
        }
    }
    fn paint(&mut self, event_loop: &ActiveEventLoop) -> Result<(), Box<dyn std::error::Error>> {
        if self.hidden() {
            self.deadline = None;
            return Ok(());
        }
        let start = Instant::now();
        let window = self.window.as_ref().unwrap();
        let size = window.inner_size();
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return Ok(());
        };
        if !self.dirty && self.retained_size == (size.width, size.height) {
            let surface = self.surface.as_mut().unwrap();
            surface.resize(width, height)?;
            let mut buffer = surface.buffer_mut()?;
            buffer.copy_from_slice(&self.retained);
            buffer.present()?;
            self.counters.presents += 1;
            return Ok(());
        }
        self.dirty = false;
        self.counters.layouts += 1;
        let open_capture = self
            .capture
            .as_ref()
            .is_some_and(|c| c.settings && !c.opened);
        let open_diagnostic = self
            .diagnostic
            .as_ref()
            .is_some_and(|d| d.settings && !d.opened);
        if (open_capture || open_diagnostic) && self.app.as_ref().unwrap().ready() {
            self.app.as_mut().unwrap().open_settings();
            if let Some(c) = &mut self.capture {
                c.opened = true;
            }
            if let Some(d) = &mut self.diagnostic {
                d.opened = true;
            }
        }
        let input = self.input.as_mut().unwrap();
        egui_winit::update_viewport_info(
            input
                .egui_input_mut()
                .viewports
                .entry(egui::ViewportId::ROOT)
                .or_default(),
            &self.ctx,
            window,
            false,
        );
        let raw = input.take_egui_input(window);
        let mut info = raw
            .viewports
            .get(&egui::ViewportId::ROOT)
            .cloned()
            .unwrap_or_default();
        let mut popup_action = None;
        let output = self.ctx.run(raw, |ctx| {
            if self.popup {
                popup_action = self.app.as_mut().unwrap().popup(ctx);
            } else {
                self.app.as_mut().unwrap().update(ctx);
            }
        });
        let ui_end = Instant::now();
        if let Some(d) = &mut self.diagnostic {
            for cause in self.ctx.repaint_causes() {
                *d.repaint_causes.entry(cause.to_string()).or_default() += 1;
            }
        }
        input.handle_platform_output(window, output.platform_output);
        let mut repaint = Duration::MAX;
        if let Some(viewport) = output.viewport_output.get(&egui::ViewportId::ROOT) {
            if viewport
                .commands
                .iter()
                .any(|c| matches!(c, egui::ViewportCommand::Close))
            {
                event_loop.exit();
            }
            let commands = viewport
                .commands
                .iter()
                .filter(|c| {
                    !matches!(
                        c,
                        egui::ViewportCommand::Close | egui::ViewportCommand::CancelClose
                    )
                })
                .cloned();
            egui_winit::process_viewport_commands(
                &self.ctx,
                &mut info,
                commands,
                window,
                &mut Vec::new(),
            );
            repaint = viewport.repaint_delay;
        }
        let primitives = self.ctx.tessellate(output.shapes, output.pixels_per_point);
        let surface = self.surface.as_mut().unwrap();
        surface.resize(width, height)?;
        let mut buffer = surface.buffer_mut()?;
        buffer.fill(0x00f4f6fa);
        let pixels: &mut [[u8; 4]] = bytemuck::cast_slice_mut(&mut buffer);
        self.raster.render(
            &mut BufferMutRef::new(pixels, size.width as usize, size.height as usize),
            &primitives,
            &output.textures_delta,
            output.pixels_per_point,
        );
        for pixel in buffer.iter_mut() {
            *pixel &= 0x00ff_ffff;
        }
        self.counters.rasters += 1;
        let raster_end = Instant::now();
        self.retained.clear();
        self.retained.extend_from_slice(&buffer);
        self.retained_size = (size.width, size.height);
        buffer.present()?;
        self.counters.presents += 1;
        let present_end = Instant::now();
        let mut close_capture = false;
        if let Some(capture) = &mut self.capture {
            if capture.frames.len() < 32 {
                capture.frames.push(FrameTiming {
                    ui_ms: (ui_end - start).as_secs_f64() * 1000.,
                    raster_ms: (raster_end - ui_end).as_secs_f64() * 1000.,
                    present_ms: (present_end - raster_end).as_secs_f64() * 1000.,
                    total_ms: (present_end - start).as_secs_f64() * 1000.,
                });
            }
            if self.app.as_ref().unwrap().ready() {
                capture.ready_frames += 1;
            }
            if capture.ready_frames >= 2 && !capture.saved {
                capture.save(&self.retained, size.width, size.height)?;
                close_capture = true;
            } else if capture.ready_frames > 0 {
                repaint = Duration::ZERO;
            }
        }
        if close_capture {
            if self.app.as_mut().unwrap().request_close() {
                event_loop.exit();
            }
        } else if let Some(when) = Instant::now().checked_add(repaint) {
            self.schedule(when);
        }
        if let Some(action) = popup_action {
            self.dispatch_tray(event_loop, action);
        }
        Ok(())
    }
}
impl ApplicationHandler<UserEvent> for CpuHost {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let window = Arc::new(
                event_loop.create_window(
                    Window::default_attributes()
                        .with_active(self.diagnostic.is_none())
                        .with_title("DownloadManager")
                        .with_window_icon(Some(winit::window::Icon::from_rgba(
                            crate::icons::rgba(64),
                            64,
                            64,
                        )?))
                        .with_decorations(!self.popup)
                        .with_resizable(!self.popup)
                        .with_inner_size(LogicalSize::new(self.size.0, self.size.1))
                        .with_min_inner_size(if self.popup {
                            LogicalSize::new(360, 220)
                        } else {
                            LogicalSize::new(760, 540)
                        }),
                )?,
            );
            if self.diagnostic.is_some() {
                window.set_cursor_hittest(false)?;
            }
            let context = softbuffer::Context::new(window.clone())?;
            self.surface = Some(softbuffer::Surface::new(&context, window.clone())?);
            self.input = Some(egui_winit::State::new(
                self.ctx.clone(),
                egui::ViewportId::ROOT,
                &*window,
                Some(window.scale_factor() as f32),
                window.theme(),
                Some(8192),
            ));
            self.app = Some(crate::ui::App::new(&self.ctx, self.wake.clone()));
            if self.capture.is_none() && self.diagnostic.as_ref().is_none_or(|d| d.tray) {
                match crate::tray::Tray::new(self.tray_sender.clone()) {
                    Ok(tray) => self.tray = Some(tray),
                    Err(error) => {
                        if let Some(d) = &mut self.diagnostic {
                            d.samples.push(serde_json::json!({"state":"tray_unavailable","error":error.to_string()}));
                        }
                        self.app
                            .as_mut()
                            .unwrap()
                            .tray_unavailable(&error.to_string());
                    }
                }
            }
            window.request_redraw();
            self.window = Some(window);
            if let Some(d) = &mut self.diagnostic {
                d.start(self.counters);
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.fail(event_loop, error);
        }
    }
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        if self.app.as_ref().is_some_and(|app| app.finished()) {
            event_loop.exit();
            return;
        }
        match event {
            UserEvent::Tray(command) => self.dispatch_tray(event_loop, command),
            UserEvent::BackendChanged => {
                let snapshot = self.app.as_ref().unwrap().snapshot();
                if let Some(tray) = &mut self.tray {
                    tray.update(&snapshot);
                }
                if self.quitting && !snapshot.stopping && snapshot.notice.is_some() {
                    self.quitting = false;
                    self.restore_full();
                }
                self.schedule(Instant::now());
            }
            UserEvent::Repaint { when, pass } if pass >= self.ctx.cumulative_pass_nr() => {
                self.schedule(when)
            }
            _ => {}
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        if id != window.id() {
            return;
        }
        let response = self.input.as_mut().unwrap().on_window_event(window, &event);
        if matches!(
            &event,
            WindowEvent::Focused(_)
                | WindowEvent::Resized(_)
                | WindowEvent::CursorMoved { .. }
                | WindowEvent::KeyboardInput { .. }
                | WindowEvent::MouseInput { .. }
                | WindowEvent::MouseWheel { .. }
        ) {
            self.counters.native_changes += 1;
        }
        if let Some(d) = &mut self.diagnostic {
            let name = match &event {
                WindowEvent::RedrawRequested => "Redraw",
                WindowEvent::CursorMoved { .. } => "CursorMoved",
                WindowEvent::Focused(_) => "Focused",
                WindowEvent::Resized(_) => "Resized",
                WindowEvent::Ime(_) => "Ime",
                WindowEvent::Occluded(_) => "Occluded",
                WindowEvent::CloseRequested => "Close",
                _ => "Other",
            };
            *d.native_events.entry(name.into()).or_default() += 1;
        }
        match event {
            WindowEvent::CloseRequested => {
                if self.tray.is_some()
                    && !self.quitting
                    && self
                        .app
                        .as_ref()
                        .unwrap()
                        .snapshot()
                        .state
                        .settings
                        .close_to_tray
                {
                    self.hide_to_tray();
                } else {
                    self.dispatch_tray(event_loop, crate::tray::Command::Quit);
                }
            }
            WindowEvent::Focused(true) if self.popup => {
                self.popup_focused = true;
                self.schedule(Instant::now());
            }
            WindowEvent::Focused(false)
                if self.popup && self.popup_focused && self.diagnostic.is_none() =>
            {
                self.hide_to_tray()
            }
            WindowEvent::RedrawRequested => {
                self.counters.native_redraws += 1;
                self.deadline = None;
                if let Err(error) = self.paint(event_loop) {
                    self.fail(event_loop, error);
                }
            }
            WindowEvent::Occluded(hidden) => {
                self.occluded = hidden;
                if hidden {
                    self.deadline = None;
                } else {
                    self.schedule(Instant::now());
                }
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                if self.popup {
                    self.place_popup();
                }
                self.schedule(Instant::now())
            }
            _ if response.repaint => self.schedule(Instant::now()),
            _ => {}
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if self
            .diagnostic
            .as_ref()
            .is_some_and(|d| d.phase > 0 && self.counters.layouts - d.baseline.layouts > 64)
        {
            self.fail(
                event_loop,
                "Diagnostic stopped: unexpected sustained UI work; inspect repaint causes",
            );
            return;
        }
        if self
            .capture
            .as_ref()
            .and_then(|c| c.deadline)
            .is_some_and(|t| t <= now)
        {
            self.fail(event_loop, "Timed out waiting for the requested UI capture");
            return;
        }
        if self
            .diagnostic
            .as_ref()
            .and_then(|d| d.next)
            .is_some_and(|t| t <= now)
        {
            let old_phase = self.diagnostic.as_ref().unwrap().phase;
            let done = self
                .diagnostic
                .as_mut()
                .unwrap()
                .advance(self.counters, self.window.as_ref().unwrap());
            if self.diagnostic.as_ref().is_some_and(|d| d.tray) {
                let phase = self.diagnostic.as_ref().unwrap().phase;
                if phase != old_phase {
                    match phase {
                        1 => {
                            let anchor = self.tray.as_ref().and_then(|t| t.anchor());
                            self.diagnostic.as_mut().unwrap().samples.push(
                                serde_json::json!({"state":"tray_registration","shell_icon_rectangle":anchor.is_some()}),
                            );
                            let (x, y) = anchor.unwrap_or((1000., 700.));
                            self.dispatch_tray(event_loop, crate::tray::Command::Popup { x, y });
                        }
                        2 => self.hide_to_tray(),
                        3 => {
                            let child =
                                std::process::Command::new(std::env::current_exe().unwrap())
                                    .arg("--activate-only")
                                    .spawn();
                            self.diagnostic.as_mut().unwrap().activation = child.ok();
                        }
                        _ => {}
                    }
                }
            }
            if done {
                if self.diagnostic.as_ref().is_some_and(|d| d.tray) {
                    self.hide_to_tray();
                    self.dispatch_tray(event_loop, crate::tray::Command::Quit);
                } else if self.app.as_mut().unwrap().request_close() {
                    event_loop.exit();
                }
            }
        }
        if self.deadline.is_some_and(|t| t <= now) {
            self.deadline = None;
            if !self.hidden() {
                self.window.as_ref().unwrap().request_redraw();
            }
        }
        let next = [
            self.deadline,
            self.capture.as_ref().and_then(|c| c.deadline),
            self.diagnostic.as_ref().and_then(|d| d.next),
        ]
        .into_iter()
        .flatten()
        .min();
        event_loop.set_control_flow(next.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }
}
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    run_mode(None, None, None)
}
pub fn run_capture(
    view: &str,
    path: PathBuf,
    size: Option<(u32, u32)>,
) -> Result<(), Box<dyn std::error::Error>> {
    run_mode(Some(Capture::new(path, view)), None, size)
}
pub fn run_diagnostic(view: &str, path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    run_mode(None, Some(Diagnostic::new(path, view)), None)
}
fn run_mode(
    capture: Option<Capture>,
    diagnostic: Option<Diagnostic>,
    size: Option<(u32, u32)>,
) -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut host = CpuHost::new(event_loop.create_proxy());
    host.capture = capture;
    host.diagnostic = diagnostic;
    host.popup = host.capture.as_ref().is_some_and(|c| c.popup)
        || host.diagnostic.as_ref().is_some_and(|d| d.popup);
    if host.popup {
        host.size = (400, 480);
    }
    if host.capture.is_none() && host.diagnostic.as_ref().is_none_or(|d| d.tray) {
        let sender = host.tray_sender.clone();
        match crate::instance::acquire(
            &download_manager::storage::data_directory()?,
            Arc::new(move || sender(crate::tray::Command::Queue)),
            false,
        )? {
            crate::instance::Acquisition::Activated => return Ok(()),
            crate::instance::Acquisition::Primary(instance) => host.instance = Some(instance),
        }
    }
    if let Some(size) = size {
        host.size = size;
    }
    event_loop.run_app(&mut host)?;
    let tray_created = host.tray.is_some();
    let tray_window = host.tray.as_ref().and_then(|t| t.native_window());
    drop(host.tray.take());
    #[cfg(windows)]
    let tray_window_destroyed = tray_window.is_some_and(
        |handle| unsafe { windows_sys::Win32::UI::WindowsAndMessaging::IsWindow(handle as _) } == 0,
    );
    #[cfg(not(windows))]
    let tray_window_destroyed = tray_window.is_none();
    if let Some(d) = &mut host.diagnostic
        && d.tray
    {
        let activation = d
            .activation
            .as_mut()
            .and_then(|child| child.try_wait().ok().flatten())
            .and_then(|status| status.code());
        d.samples.push(serde_json::json!({"state":"tray_cleanup","tray_created":tray_created,"tray_dropped":host.tray.is_none(),"tray_native_window_destroyed":tray_window_destroyed,"duplicate_activation_exit":activation,"same_main_window":host.window.is_some(),"quit_requested_while_hidden":host.window.as_ref().is_some_and(|w|w.is_visible()==Some(false))}));
    }
    if let Some(c) = &host.capture {
        c.report()?;
    }
    if let Some(d) = &host.diagnostic {
        d.report()?;
    }
    if let Some(error) = host.error {
        return Err(error.into());
    }
    if host.diagnostic.as_ref().is_some_and(|d| d.tray) && !tray_created {
        return Err(
            "Native tray diagnostic unavailable: Windows rejected tray registration; see report"
                .into(),
        );
    }
    if host.capture.as_ref().is_some_and(|c| !c.saved) {
        return Err("No live application frame was captured".into());
    }
    if host
        .diagnostic
        .as_ref()
        .is_some_and(|d| d.phase != PHASES.len())
    {
        return Err("Runtime diagnostic ended before all phases completed".into());
    }
    Ok(())
}
