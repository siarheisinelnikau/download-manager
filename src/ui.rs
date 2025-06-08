use download_manager::{
    manager::{Command, Manager, Snapshot},
    model::{
        Download, Settings, Status, VERSION, suggested_filename, validate_filename, validate_url,
    },
    storage,
    updater::UpdateStatus,
};
use egui::{Color32, RichText, Stroke, Vec2};
use std::{path::PathBuf, sync::Arc};

const BACKGROUND: Color32 = Color32::from_rgb(244, 246, 250);
const SURFACE: Color32 = Color32::WHITE;
const INK: Color32 = Color32::from_rgb(28, 39, 57);
const MUTED: Color32 = Color32::from_rgb(83, 99, 120);
const BORDER: Color32 = Color32::from_rgb(218, 225, 234);
const ACCENT: Color32 = Color32::from_rgb(33, 99, 209);
const ERROR: Color32 = Color32::from_rgb(166, 47, 51);
const SUCCESS: Color32 = Color32::from_rgb(29, 111, 77);

pub struct App {
    manager: Option<Manager>,
    startup_error: Option<String>,
    local_error: Option<String>,
    completed: bool,
    add_open: bool,
    settings_open: bool,
    url: String,
    filename: String,
    custom_filename: bool,
    directory: String,
    focus_url: bool,
    form_error: Option<String>,
    pending_add: Option<(u64, u64)>,
    settings: Settings,
    settings_directory: String,
    saved_settings: bool,
    restart: Option<u64>,
}

impl App {
    pub fn new(ctx: &egui::Context, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let mut style = (*ctx.style()).clone();
        style.visuals = egui::Visuals::light();
        style.visuals.override_text_color = Some(INK);
        style.visuals.panel_fill = BACKGROUND;
        style.visuals.window_fill = SURFACE;
        style.visuals.extreme_bg_color = SURFACE;
        style.visuals.faint_bg_color = BACKGROUND;
        style.visuals.selection.bg_fill = Color32::from_rgb(219, 233, 255);
        style.visuals.selection.stroke = Stroke::new(1.0, ACCENT);
        style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, MUTED);
        style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
        style.visuals.widgets.inactive.bg_fill = SURFACE;
        style.visuals.widgets.inactive.weak_bg_fill = SURFACE;
        style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
        style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, INK);
        style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(234, 241, 253);
        style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(234, 241, 253);
        style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
        style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, INK);
        style.visuals.widgets.active.bg_fill = Color32::from_rgb(219, 233, 255);
        style.visuals.widgets.active.fg_stroke = Stroke::new(1.0, INK);
        style.spacing.item_spacing = egui::vec2(12.0, 10.0);
        style.spacing.button_padding = egui::vec2(14.0, 9.0);
        style.spacing.interact_size.y = 34.0;
        style.animation_time = 0.0;
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(15.0));
        style
            .text_styles
            .insert(egui::TextStyle::Small, egui::FontId::proportional(13.0));
        style
            .text_styles
            .insert(egui::TextStyle::Heading, egui::FontId::proportional(28.0));
        ctx.set_style(style);
        let (manager, startup_error) = match storage::data_directory() {
            Ok(directory) => (Some(Manager::start(directory, wake)), None),
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
            custom_filename: false,
            directory: String::new(),
            focus_url: false,
            form_error: None,
            pending_add: None,
            settings: Settings::default(),
            settings_directory: String::new(),
            saved_settings: false,
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

    pub fn ready(&self) -> bool {
        self.manager.as_ref().is_some_and(|m| m.snapshot().ready)
    }

    pub fn open_settings(&mut self) {
        if let Some(manager) = &self.manager {
            self.settings = manager.snapshot().state.settings;
            self.settings_directory = self.settings.default_directory.display().to_string();
            self.settings_open = true;
            self.saved_settings = false;
        }
    }

    pub fn finished(&self) -> bool {
        self.manager.as_ref().is_some_and(|m| {
            let s = m.snapshot();
            s.stopped && s.fatal.is_none()
        })
    }

    pub fn request_close(&mut self) -> bool {
        let Some(manager) = &self.manager else {
            return true;
        };
        let snapshot = manager.snapshot();
        if snapshot.fatal.is_some() || snapshot.stopped {
            return true;
        }
        if !snapshot.stopping {
            self.send(Command::Shutdown);
        }
        false
    }

    fn open_add(&mut self, snapshot: &Snapshot) {
        self.directory = snapshot
            .state
            .settings
            .default_directory
            .display()
            .to_string();
        self.form_error = None;
        self.add_open = true;
        self.focus_url = true;
    }

    fn submit_add(&mut self, snapshot: &Snapshot) {
        let validation = validate_url(&self.url).and_then(|_| validate_filename(&self.filename));
        if let Err(error) = validation {
            self.form_error = Some(error.to_string());
            return;
        }
        if !PathBuf::from(&self.directory).is_absolute() {
            self.form_error = Some("Enter the full path to an existing folder.".into());
            return;
        }
        if self.send(Command::Add {
            url: self.url.clone(),
            directory: PathBuf::from(&self.directory),
            filename: self.filename.clone(),
        }) {
            self.form_error = None;
            self.pending_add = Some((
                snapshot.state.next_id,
                snapshot
                    .last_add_result
                    .as_ref()
                    .map_or(1, |(sequence, _)| sequence + 1),
            ));
        }
    }

    fn add_dialog(&mut self, ctx: &egui::Context, snapshot: &Snapshot) {
        if !self.add_open {
            return;
        }
        if let Some((id, sequence)) = self.pending_add {
            if snapshot.state.downloads.iter().any(|j| j.id == id) {
                self.pending_add = None;
                self.add_open = false;
                self.url.clear();
                self.filename.clear();
                self.custom_filename = false;
                return;
            }
            if let Some((received, Some(error))) = &snapshot.last_add_result
                && *received >= sequence
            {
                self.form_error = Some(error.clone());
                self.pending_add = None;
            }
        }
        let modal = egui::Modal::new(egui::Id::new("new_download")).show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 80.0).clamp(320.0, 520.0));
            ui.heading("New download");
            secondary(ui, "Paste a link and choose where to save the file.");
            ui.add_space(6.0);
            ui.label("Download link");
            let url = ui.add(
                egui::TextEdit::singleline(&mut self.url)
                    .desired_width(f32::INFINITY)
                    .hint_text("https://example.com/file.zip"),
            );
            if self.focus_url {
                url.request_focus();
                self.focus_url = false;
            }
            if url.changed() && !self.custom_filename {
                self.filename = suggested_filename(&self.url);
            }
            ui.label("File name");
            if ui
                .add(egui::TextEdit::singleline(&mut self.filename).desired_width(f32::INFINITY))
                .changed()
            {
                self.custom_filename = true;
            }
            ui.label("Save to folder");
            ui.add(egui::TextEdit::singleline(&mut self.directory).desired_width(f32::INFINITY));
            secondary(
                ui,
                "Use an existing folder. Existing files are never replaced.",
            );
            if let Some(error) = &self.form_error {
                ui.colored_label(ERROR, error);
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let pending = self.pending_add.is_some();
                if ui
                    .add_enabled(
                        !pending,
                        primary(if pending { "Adding…" } else { "Download" }),
                    )
                    .clicked()
                {
                    self.submit_add(snapshot);
                }
                if ui
                    .add_enabled(!pending, egui::Button::new("Cancel"))
                    .clicked()
                {
                    self.add_open = false;
                }
            });
        });
        if modal.should_close() && self.pending_add.is_none() {
            self.add_open = false;
        }
    }

    fn settings_page(&mut self, ui: &mut egui::Ui, snapshot: &Snapshot) {
        ui.horizontal(|ui| {
            if ui.button("Back to downloads").clicked() {
                self.settings_open = false;
            }
            ui.label(RichText::new("Settings").size(28.0).strong());
        });
        ui.add_space(10.0);
        let scroll_height = (ui.available_height() - 76.0).max(160.0);
        egui::ScrollArea::vertical().id_salt("settings").max_height(scroll_height).auto_shrink([false, false]).show(ui, |ui| {
            ui.set_max_width(720.0);
            section(ui, "Download defaults");
            ui.label("Save new downloads to");
            ui.add(egui::TextEdit::singleline(&mut self.settings_directory).desired_width(f32::INFINITY));
            secondary(ui, "You can choose another folder for each download.");
            ui.add_space(6.0);
            egui::Grid::new("download_limits").num_columns(2).spacing([24.0, 12.0]).show(ui, |ui| {
                ui.label("Simultaneous downloads");
                egui::ComboBox::from_id_salt("concurrency").selected_text(self.settings.concurrency.to_string()).width(100.0).show_ui(ui, |ui| {
                    for count in 1..=4 { ui.selectable_value(&mut self.settings.concurrency, count, count.to_string()); }
                }); ui.end_row();
                ui.label("Total speed limit");
                ui.horizontal(|ui| {
                    let mut limited = self.settings.bandwidth_kib > 0;
                    if ui.checkbox(&mut limited, "Limit speed").changed() { self.settings.bandwidth_kib = if limited { 1024 } else { 0 }; }
                    if limited { ui.add(egui::DragValue::new(&mut self.settings.bandwidth_kib).range(1..=1_048_576).suffix(" KiB/s")); }
                }); ui.end_row();
            });
            ui.add_space(14.0); ui.separator(); ui.add_space(8.0);
            ui.horizontal(|ui| { section(ui, "Updates"); ui.label(RichText::new(format!("Version {VERSION}")).color(MUTED)); });
            ui.checkbox(&mut self.settings.startup_update_check, "Check for updates when DownloadManager starts");
            let busy = matches!(snapshot.update, UpdateStatus::Checking | UpdateStatus::Downloading { .. } | UpdateStatus::Staged(_));
            if ui.add_enabled(!busy, egui::Button::new("Check for updates")).clicked() { self.send(Command::CheckUpdates); }
            match &snapshot.update {
                UpdateStatus::Idle => { secondary(ui, "No update check yet. You can check manually at any time."); }
                UpdateStatus::Checking => { secondary(ui, "Checking for an update…"); }
                UpdateStatus::Current => { ui.colored_label(SUCCESS, "You have the latest published version."); }
                UpdateStatus::Error(error) => {
                    ui.colored_label(ERROR, "Couldn't check for updates. Try again later.");
                    egui::CollapsingHeader::new("Error details").show(ui, |ui| { ui.label(error); });
                }
                UpdateStatus::Available(release) => {
                    ui.colored_label(SUCCESS, format!("Version {} is available.", release.version));
                    secondary(ui, "Your downloads will pause while the app updates and restarts. Files and settings are kept.");
                    if ui.add(primary("Download and install")).clicked() { self.send(Command::InstallUpdate); }
                }
                UpdateStatus::Downloading { bytes: done, total } => {
                    ui.label(format!("Downloading update · {} of {}", bytes(*done), bytes(*total)));
                    ui.add(egui::ProgressBar::new(*done as f32 / (*total).max(1) as f32).fill(ACCENT).animate(false));
                }
                UpdateStatus::Staged(_) => { secondary(ui, "Update verified. Saving your queue before restarting…"); }
            }
        });
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);
        let mut draft = self.settings.clone();
        draft.default_directory = PathBuf::from(&self.settings_directory);
        let changed = draft != snapshot.state.settings;
        ui.horizontal(|ui| {
            if ui.add_enabled(changed, primary("Save changes")).clicked() {
                self.saved_settings = self.send(Command::Settings(draft.clone()));
            }
            if self.saved_settings && !changed && snapshot.notice.is_none() {
                ui.colored_label(SUCCESS, "Changes saved");
            } else if changed {
                secondary(ui, "Changes haven't been saved yet.");
            }
        });
    }

    fn download_row(&mut self, ui: &mut egui::Ui, job: &Download) {
        ui.spacing_mut().item_spacing.y = 6.0;
        ui.add_space(8.0);
        let filename = job
            .destination
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2((ui.available_width() - 145.0).max(120.0), 24.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.add(
                        egui::Label::new(RichText::new(filename.as_ref()).size(17.0).strong())
                            .truncate(),
                    )
                    .on_hover_text(filename.as_ref());
                },
            );
            let (label, color) = match job.status {
                Status::Queued => ("Waiting", MUTED),
                Status::Downloading => ("Downloading", ACCENT),
                Status::Pausing => ("Pausing…", MUTED),
                Status::Paused => ("Paused", MUTED),
                Status::Canceled => ("Canceled", MUTED),
                Status::Failed => ("Needs attention", ERROR),
                Status::Completed => ("Complete", SUCCESS),
            };
            ui.label(RichText::new(label).color(color).strong());
        });
        let host = url::Url::parse(&job.url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_owned))
            .unwrap_or_else(|| job.url.clone());
        ui.label(RichText::new(host).color(MUTED).size(13.0))
            .on_hover_text(&job.url);
        let fraction = if job.status == Status::Completed {
            1.0
        } else {
            job.total
                .filter(|n| *n > 0)
                .map(|n| job.bytes as f32 / n as f32)
                .unwrap_or(0.0)
                .clamp(0.0, 1.0)
        };
        ui.scope(|ui| {
            ui.visuals_mut().extreme_bg_color = Color32::from_rgb(231, 236, 244);
            ui.add(
                egui::ProgressBar::new(fraction)
                    .fill(if job.status == Status::Completed {
                        SUCCESS
                    } else {
                        ACCENT
                    })
                    .animate(false)
                    .desired_height(6.0)
                    .desired_width(ui.available_width()),
            );
        });
        ui.horizontal_wrapped(|ui| {
            let total = job
                .total
                .map(bytes)
                .unwrap_or_else(|| "size unknown".into());
            secondary(ui, &format!("{} of {total}", bytes(job.bytes)));
            if job.status == Status::Downloading {
                secondary(ui, &format!("{}/s", bytes(job.speed.max(0.0) as u64)));
                if let Some(total) = job.total.filter(|_| job.speed > 0.0) {
                    secondary(
                        ui,
                        &format!(
                            "{} remaining",
                            duration(total.saturating_sub(job.bytes) as f64 / job.speed)
                        ),
                    );
                }
            }
        });
        if let Some(error) = &job.error {
            ui.colored_label(ERROR, "Download stopped. Your partial file is kept.");
            egui::CollapsingHeader::new("Details")
                .id_salt(job.id)
                .show(ui, |ui| {
                    ui.label(error);
                });
        }
        ui.horizontal(|ui| {
            let action = match job.status {
                Status::Downloading | Status::Queued => Some(("Pause", Command::Pause(job.id))),
                Status::Paused | Status::Canceled => Some(("Resume", Command::Resume(job.id))),
                Status::Failed => Some(("Retry download", Command::Resume(job.id))),
                _ => None,
            };
            if let Some((label, command)) = action
                && ui.button(label).clicked()
            {
                self.send(command);
            }
            ui.menu_button("More", |ui| {
                ui.label(RichText::new("Save location").strong());
                ui.label(job.destination.display().to_string());
                ui.separator();
                if job.status == Status::Queued && ui.button("Move up in queue").clicked() {
                    self.send(Command::MoveUp(job.id));
                    ui.close();
                }
                if matches!(
                    job.status,
                    Status::Paused | Status::Failed | Status::Canceled
                ) && ui.button("Restart from zero…").clicked()
                {
                    self.restart = Some(job.id);
                    ui.close();
                }
                if matches!(
                    job.status,
                    Status::Downloading | Status::Queued | Status::Paused | Status::Failed
                ) && ui.button("Cancel download").clicked()
                {
                    self.send(Command::Cancel(job.id));
                    ui.close();
                }
                if job.status == Status::Completed && ui.button("Remove from history").clicked() {
                    self.send(Command::RemoveCompleted(job.id));
                    ui.close();
                }
            });
        });
        ui.add_space(10.0);
    }

    pub fn update(&mut self, ctx: &egui::Context) {
        let snapshot = self
            .manager
            .as_ref()
            .map(Manager::snapshot)
            .unwrap_or_default();
        if snapshot.stopped && snapshot.fatal.is_none() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if snapshot.ready
            && !snapshot.stopping
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::N))
        {
            self.open_add(&snapshot);
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BACKGROUND).inner_margin(24.0))
            .show(ctx, |ui| {
                if self.settings_open {
                    if let Some(error) = self.local_error.as_ref().or(snapshot.notice.as_ref()) {
                        ui.colored_label(ERROR, error);
                    }
                    self.settings_page(ui, &snapshot);
                    return;
                }
                ui.horizontal(|ui| {
                    ui.heading(RichText::new("Downloads").strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(
                                snapshot.ready && !snapshot.stopping,
                                primary("+ New download"),
                            )
                            .clicked()
                        {
                            self.open_add(&snapshot);
                        }
                        if ui
                            .add_enabled(snapshot.ready, egui::Button::new("Settings"))
                            .clicked()
                        {
                            self.open_settings();
                        }
                    });
                });
                secondary(ui, "DownloadManager");
                ui.add_space(12.0);
                if let Some(error) = self.startup_error.as_ref().or(snapshot.fatal.as_ref()) {
                    ui.colored_label(ERROR, "DownloadManager couldn't open your queue.");
                    ui.label(error);
                    return;
                }
                if !snapshot.ready {
                    secondary(ui, "Loading your saved downloads…");
                    return;
                }
                if let Some(error) = self.local_error.as_ref().or(snapshot.notice.as_ref()) {
                    ui.colored_label(ERROR, error);
                }
                if matches!(snapshot.update, UpdateStatus::Available(_)) {
                    ui.label(RichText::new("An update is available in Settings.").color(ACCENT));
                }
                if snapshot.stopping {
                    secondary(ui, "Saving your downloads before closing…");
                }
                ui.add_enabled_ui(!snapshot.stopping, |ui| {
                    let completed = snapshot
                        .state
                        .downloads
                        .iter()
                        .filter(|j| j.status == Status::Completed)
                        .count();
                    ui.horizontal(|ui| {
                        ui.selectable_value(
                            &mut self.completed,
                            false,
                            format!("Queue  {}", snapshot.state.downloads.len() - completed),
                        );
                        ui.selectable_value(
                            &mut self.completed,
                            true,
                            format!("Completed  {completed}"),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let active =
                                snapshot.state.downloads.iter().any(|j| {
                                    matches!(j.status, Status::Downloading | Status::Queued)
                                });
                            let paused = snapshot
                                .state
                                .downloads
                                .iter()
                                .any(|j| j.status == Status::Paused);
                            if active && ui.button("Pause all").clicked() {
                                self.send(Command::PauseAll);
                            }
                            if paused && ui.button("Resume paused").clicked() {
                                self.send(Command::ResumeAll);
                            }
                        });
                    });
                    ui.add_space(4.0);
                    ui.separator();
                    let jobs: Vec<_> = snapshot
                        .state
                        .downloads
                        .iter()
                        .filter(|j| (j.status == Status::Completed) == self.completed)
                        .collect();
                    if jobs.is_empty() {
                        ui.add_space(64.0);
                        ui.vertical_centered(|ui| {
                            let (rect, _) =
                                ui.allocate_exact_size(Vec2::splat(64.0), egui::Sense::hover());
                            let painter = ui.painter();
                            let center = rect.center();
                            painter.circle_filled(center, 32.0, Color32::from_rgb(226, 236, 253));
                            painter.line_segment(
                                [
                                    center - egui::vec2(0.0, 15.0),
                                    center + egui::vec2(0.0, 7.0),
                                ],
                                Stroke::new(3.0, ACCENT),
                            );
                            painter.line_segment(
                                [
                                    center + egui::vec2(-8.0, 0.0),
                                    center + egui::vec2(0.0, 8.0),
                                ],
                                Stroke::new(3.0, ACCENT),
                            );
                            painter.line_segment(
                                [center + egui::vec2(8.0, 0.0), center + egui::vec2(0.0, 8.0)],
                                Stroke::new(3.0, ACCENT),
                            );
                            painter.line_segment(
                                [
                                    center + egui::vec2(-12.0, 18.0),
                                    center + egui::vec2(12.0, 18.0),
                                ],
                                Stroke::new(3.0, ACCENT),
                            );
                            ui.add_space(12.0);
                            ui.label(
                                RichText::new(if self.completed {
                                    "Completed files will appear here"
                                } else {
                                    "Ready for your first download"
                                })
                                .size(22.0)
                                .strong(),
                            );
                            secondary(
                                ui,
                                if self.completed {
                                    "Your files stay in the folder you chose."
                                } else {
                                    "Choose New download and paste a link to get started."
                                },
                            );
                            if !self.completed {
                                ui.add_space(4.0);
                                secondary(ui, "Shortcut: Ctrl+N");
                            }
                        });
                    } else {
                        egui::ScrollArea::vertical()
                            .id_salt("downloads")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                egui::Frame::new()
                                    .fill(SURFACE)
                                    .inner_margin(18.0)
                                    .corner_radius(8.0)
                                    .stroke(Stroke::new(1.0, BORDER))
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        for (index, job) in jobs.iter().enumerate() {
                                            if index > 0 {
                                                ui.separator();
                                            }
                                            self.download_row(ui, job);
                                        }
                                    });
                            });
                    }
                });
            });
        self.add_dialog(ctx, &snapshot);
        if let Some(id) = self.restart {
            let modal = egui::Modal::new(egui::Id::new("restart_download")).show(ctx, |ui| {
                ui.set_max_width(440.0); ui.heading("Restart from the beginning?");
                ui.label("This discards this download's partial file and downloads it again. Existing completed files are kept.");
                ui.horizontal(|ui| {
                    if ui.add(primary("Restart download")).clicked() { self.send(Command::Restart(id)); self.restart = None; }
                    if ui.button("Keep partial file").clicked() { self.restart = None; }
                });
            });
            if modal.should_close() {
                self.restart = None;
            }
        }
    }
}

fn primary(text: &str) -> egui::Button<'_> {
    egui::Button::new(RichText::new(text).color(Color32::WHITE).strong())
        .fill(ACCENT)
        .stroke(Stroke::NONE)
        .corner_radius(6.0)
}
fn secondary(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).color(MUTED));
}
fn section(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(18.0).strong());
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
