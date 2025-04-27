use download_manager::{
    manager::{Command, Manager, Snapshot},
    model::{Download, Settings, Status, VERSION, suggested_filename},
    storage,
    updater::UpdateStatus,
};
use eframe::egui::{self, Color32, RichText};
use std::{path::PathBuf, sync::Arc};

pub struct App {
    manager: Option<Manager>,
    startup_error: Option<String>,
    local_error: Option<String>,
    completed: bool,
    add_open: bool,
    settings_open: bool,
    url: String,
    filename: String,
    directory: String,
    settings: Settings,
    settings_directory: String,
    restart: Option<u64>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::light());
        let mut style = (*cc.egui_ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(10.0, 9.0);
        style.spacing.button_padding = egui::vec2(12.0, 7.0);
        cc.egui_ctx.set_style(style);
        let ctx = cc.egui_ctx.clone();
        let (manager, startup_error) = match storage::data_directory() {
            Ok(directory) => (
                Some(Manager::start(
                    directory,
                    Arc::new(move || ctx.request_repaint()),
                )),
                None,
            ),
            Err(e) => (None, Some(format!("{e:#}"))),
        };
        Self {
            manager,
            startup_error,
            local_error: None,
            completed: false,
            add_open: false,
            settings_open: false,
            url: String::new(),
            filename: String::new(),
            directory: String::new(),
            settings: Settings::default(),
            settings_directory: String::new(),
            restart: None,
        }
    }

    fn send(&mut self, command: Command) -> bool {
        if let Some(manager) = &self.manager {
            match manager.send(command) {
                Ok(()) => {
                    self.local_error = None;
                    return true;
                }
                Err(error) => self.local_error = Some(format!("{error:#}")),
            }
        }
        false
    }

    fn add_window(&mut self, ctx: &egui::Context) {
        if !self.add_open {
            return;
        }
        let mut open = true;
        egui::Window::new("New download").open(&mut open).collapsible(false).resizable(true).default_width(580.0).show(ctx, |ui| {
            ui.label("Paste an HTTP or HTTPS URL");
            if ui.add(egui::TextEdit::singleline(&mut self.url).desired_width(f32::INFINITY).hint_text("https://example.com/archive.zip")).changed() {
                self.filename = suggested_filename(&self.url);
            }
            ui.label("Destination folder (existing absolute path)");
            ui.add(egui::TextEdit::singleline(&mut self.directory).desired_width(f32::INFINITY));
            ui.label("Save as");
            ui.add(egui::TextEdit::singleline(&mut self.filename).desired_width(f32::INFINITY));
            ui.weak("Existing files are never overwritten. Downloads are never opened or executed automatically.");
            if ui.button("Add to queue").clicked()
                && self.send(Command::Add { url: self.url.clone(), directory: PathBuf::from(&self.directory), filename: self.filename.clone() }) {
                self.add_open = false;
            }
        });
        self.add_open &= open;
    }

    fn settings_window(&mut self, ctx: &egui::Context, snapshot: &Snapshot) {
        if !self.settings_open {
            return;
        }
        let mut open = true;
        egui::Window::new("Settings & updates").open(&mut open).default_width(620.0).resizable(true).show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height(580.0).show(ui, |ui| {
                ui.heading("Downloads");
                ui.add(egui::Slider::new(&mut self.settings.concurrency, 1..=4).text("Simultaneous downloads"));
                ui.horizontal(|ui| {
                    ui.label("Total bandwidth limit (KiB/s)");
                    ui.add(egui::DragValue::new(&mut self.settings.bandwidth_kib).range(0..=1_048_576));
                });
                ui.weak("0 = unlimited. One connection per file; limit is shared across active downloads.");
                ui.label("Default destination folder");
                ui.add(egui::TextEdit::singleline(&mut self.settings_directory).desired_width(f32::INFINITY));
                ui.checkbox(&mut self.settings.startup_update_check, "Check for updates on startup");
                if ui.button("Save settings").clicked() {
                    self.settings.default_directory = PathBuf::from(&self.settings_directory);
                    self.send(Command::Settings(self.settings.clone()));
                }
                ui.separator();
                ui.heading("Updates");
                ui.label(format!("Installed version: {VERSION} · Stable channel · Windows x64 portable"));
                ui.weak("Canonical repository: siarheisinelnikau/download-manager");
                ui.weak("The repository/release may not be published yet. A failed check is not an up-to-date result.");
                let busy = matches!(snapshot.update, UpdateStatus::Checking | UpdateStatus::Downloading { .. } | UpdateStatus::Staged(_));
                if ui.add_enabled(!busy, egui::Button::new("Check for updates")).clicked() { self.send(Command::CheckUpdates); }
                match &snapshot.update {
                    UpdateStatus::Idle => { ui.label("No update check performed."); }
                    UpdateStatus::Checking => { ui.label("Checking for a stable release…"); }
                    UpdateStatus::Current => { ui.label("You have the latest published stable version."); }
                    UpdateStatus::Error(error) => { ui.colored_label(Color32::from_rgb(170, 50, 40), error); }
                    UpdateStatus::Available(release) => {
                        ui.label(format!("Version {} is available ({}).", release.version, bytes(release.size)));
                        ui.weak("Installation pauses downloads, saves the queue, closes this app, replaces the portable EXE and restarts. Settings and partial files are preserved.");
                        if ui.button("Download and install").clicked() { self.send(Command::InstallUpdate); }
                    }
                    UpdateStatus::Downloading { bytes: done, total } => {
                        ui.add(egui::ProgressBar::new(*done as f32 / (*total).max(1) as f32).text(format!("Downloading update: {} / {}", bytes(*done), bytes(*total))));
                    }
                    UpdateStatus::Staged(_) => { ui.label("Update verified. Pausing downloads and preparing restart…"); }
                }
                ui.separator();
                ui.weak("Resume requires a matching strong ETag and a valid byte range. Otherwise choose Restart from zero. Paused transfers remain paused after restart.");
                if let Ok(directory) = storage::data_directory() { ui.label(format!("App data: {}", directory.display())); }
                ui.weak("Update helper errors are recorded in updates/last-update-error.txt. Close this window to keep working.");
            });
        });
        self.settings_open = open;
    }

    fn download_row(&mut self, ui: &mut egui::Ui, job: &Download) {
        egui::Frame::group(ui.style())
            .fill(Color32::WHITE)
            .inner_margin(14.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let name = job
                        .destination
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy();
                    ui.label(RichText::new(name).strong().size(17.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let color = match job.status {
                            Status::Failed => Color32::from_rgb(180, 55, 40),
                            Status::Completed => Color32::from_rgb(30, 120, 75),
                            _ => Color32::from_rgb(65, 85, 110),
                        };
                        ui.colored_label(color, format!("{:?}", job.status));
                    });
                });
                ui.label(RichText::new(&job.url).small().color(Color32::DARK_GRAY));
                ui.label(
                    RichText::new(job.destination.display().to_string())
                        .small()
                        .color(Color32::DARK_GRAY),
                );
                let fraction = job
                    .total
                    .filter(|n| *n > 0)
                    .map(|n| job.bytes as f32 / n as f32)
                    .unwrap_or(0.0)
                    .clamp(0.0, 1.0);
                let total = job
                    .total
                    .map(bytes)
                    .unwrap_or_else(|| "unknown size".into());
                let mut text = format!("{} / {}", bytes(job.bytes), total);
                if job.status == Status::Downloading {
                    text.push_str(&format!("  ·  {}/s", bytes(job.speed.max(0.0) as u64)));
                    if let Some(total) = job.total.filter(|_| job.speed > 0.0) {
                        text.push_str(&format!(
                            "  ·  ETA {}",
                            duration(total.saturating_sub(job.bytes) as f64 / job.speed)
                        ));
                    }
                }
                ui.add(
                    egui::ProgressBar::new(if job.status == Status::Completed {
                        1.0
                    } else {
                        fraction
                    })
                    .text(text)
                    .desired_width(f32::INFINITY),
                );
                if let Some(error) = &job.error {
                    ui.colored_label(Color32::from_rgb(170, 50, 40), error);
                }
                ui.horizontal_wrapped(|ui| {
                    if job.status == Status::Completed
                        && ui.small_button("Remove from history").clicked()
                    {
                        self.send(Command::RemoveCompleted(job.id));
                    }
                    match job.status {
                        Status::Downloading | Status::Queued => {
                            if ui.small_button("Pause").clicked() {
                                self.send(Command::Pause(job.id));
                            }
                        }
                        Status::Paused | Status::Failed | Status::Canceled => {
                            if ui
                                .small_button(if job.status == Status::Failed {
                                    "Retry / resume"
                                } else {
                                    "Resume"
                                })
                                .clicked()
                            {
                                self.send(Command::Resume(job.id));
                            }
                            if ui.small_button("Restart from zero…").clicked() {
                                self.restart = Some(job.id);
                            }
                        }
                        _ => {}
                    }
                    if matches!(
                        job.status,
                        Status::Downloading | Status::Queued | Status::Paused | Status::Failed
                    ) && ui.small_button("Cancel").clicked()
                    {
                        self.send(Command::Cancel(job.id));
                    }
                    if job.status == Status::Queued && ui.small_button("Move up").clicked() {
                        self.send(Command::MoveUp(job.id));
                    }
                    if matches!(job.status, Status::Canceled) {
                        ui.weak("Canceled; partial data kept for recovery.");
                    }
                });
            });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let snapshot = self
            .manager
            .as_ref()
            .map(Manager::snapshot)
            .unwrap_or_default();
        if snapshot.stopped && snapshot.fatal.is_none() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else if ctx.input(|i| i.viewport().close_requested())
            && self.manager.is_some()
            && snapshot.fatal.is_none()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if !snapshot.stopping {
                self.send(Command::Shutdown);
            }
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.heading(RichText::new("DownloadManager").size(28.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Settings & updates").clicked() {
                        self.settings = snapshot.state.settings.clone();
                        self.settings_directory =
                            self.settings.default_directory.display().to_string();
                        self.settings_open = true;
                    }
                });
            });
            ui.weak("A clear queue. Careful recovery. Your files stay under your control.");
            ui.add_space(10.0);
            if let Some(error) = self.startup_error.as_ref().or(snapshot.fatal.as_ref()) {
                ui.colored_label(Color32::DARK_RED, error);
                return;
            }
            if !snapshot.ready {
                ui.label("Loading saved queue…");
                return;
            }
            if let Some(error) = self.local_error.as_ref().or(snapshot.notice.as_ref()) {
                ui.colored_label(Color32::DARK_RED, error);
            }
            if matches!(snapshot.update, UpdateStatus::Available(_)) {
                ui.colored_label(
                    Color32::from_rgb(35, 95, 150),
                    "An update is available. Open Settings & updates to review and install it.",
                );
            }
            if snapshot.stopping {
                ui.label("Saving checkpoints and pausing active transfers before closing…");
            }
            ui.add_enabled_ui(!snapshot.stopping, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .button(RichText::new("+ Add download").strong())
                        .clicked()
                    {
                        self.directory = snapshot
                            .state
                            .settings
                            .default_directory
                            .display()
                            .to_string();
                        self.add_open = true;
                    }
                    if ui.button("Pause all").clicked() {
                        self.send(Command::PauseAll);
                    }
                    if ui.button("Resume paused").clicked() {
                        self.send(Command::ResumeAll);
                    }
                    let active = snapshot
                        .state
                        .downloads
                        .iter()
                        .filter(|j| j.status == Status::Downloading)
                        .count();
                    let speed: f64 = snapshot.state.downloads.iter().map(|j| j.speed).sum();
                    ui.label(format!(
                        "{active} active · {}/s",
                        bytes(speed.max(0.0) as u64)
                    ));
                });
                ui.separator();
                ui.horizontal(|ui| {
                    let completed = snapshot
                        .state
                        .downloads
                        .iter()
                        .filter(|j| j.status == Status::Completed)
                        .count();
                    ui.selectable_value(
                        &mut self.completed,
                        false,
                        format!("Queue ({})", snapshot.state.downloads.len() - completed),
                    );
                    ui.selectable_value(
                        &mut self.completed,
                        true,
                        format!("Completed ({completed})"),
                    );
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let jobs: Vec<_> = snapshot
                            .state
                            .downloads
                            .iter()
                            .filter(|j| (j.status == Status::Completed) == self.completed)
                            .collect();
                        if jobs.is_empty() {
                            ui.add_space(40.0);
                            ui.label(
                                RichText::new(if self.completed {
                                    "Completed downloads appear here."
                                } else {
                                    "Your queue is ready."
                                })
                                .size(22.0),
                            );
                            ui.label("Add a URL and choose a destination folder to get started.");
                        }
                        for job in jobs {
                            self.download_row(ui, job);
                            ui.add_space(4.0);
                        }
                    });
            });
        });
        self.add_window(ctx);
        self.settings_window(ctx, &snapshot);
        if let Some(id) = self.restart {
            egui::Window::new("Restart download?").collapsible(false).resizable(false).show(ctx, |ui| {
                ui.label("This removes only this task's managed partial file and starts the URL from zero.");
                ui.label("Existing final destination files remain protected.");
                ui.horizontal(|ui| {
                    if ui.button("Restart from zero").clicked() { self.send(Command::Restart(id)); self.restart = None; }
                    if ui.button("Keep partial file").clicked() { self.restart = None; }
                });
            });
        }
        // No continuous repaint: background events wake the UI at most every 250 ms per transfer.
    }
}

fn bytes(value: u64) -> String {
    if value < 1024 {
        format!("{value} B")
    } else if value < 1024 * 1024 {
        format!("{:.1} KiB", value as f64 / 1024.0)
    } else if value < 1024 * 1024 * 1024 {
        format!("{:.1} MiB", value as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GiB", value as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

fn duration(seconds: f64) -> String {
    let seconds = seconds.max(0.0) as u64;
    if seconds >= 3600 {
        format!("{}h {}m", seconds / 3600, seconds % 3600 / 60)
    } else {
        format!("{}m {}s", seconds / 60, seconds % 60)
    }
}
