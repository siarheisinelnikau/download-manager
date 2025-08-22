use crate::folder_picker::{Request as FolderRequest, Target as FolderTarget};
use crate::theme::{self, palette};
use download_manager::{
    manager::{Command, Manager, Snapshot},
    model::{
        Download, Settings, Status, Theme, VERSION, suggested_filename, validate_filename,
        validate_url,
    },
    storage,
    updater::UpdateStatus,
};
use egui::{Color32, RichText, Stroke, Vec2};
use std::{path::PathBuf, sync::Arc};

pub struct App {
    icon: egui::TextureHandle,
    applied_theme: Theme,
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
    folder_request: Option<FolderRequest>,
    folder_pending: Option<(u64, FolderTarget)>,
    folder_generation: u64,
}

impl App {
    pub fn new(ctx: &egui::Context, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let icon = ctx.load_texture(
            "application-icon",
            egui::ColorImage::from_rgba_unmultiplied([64, 64], &crate::icons::rgba(64)),
            egui::TextureOptions::LINEAR,
        );
        let mut style = (*ctx.style()).clone();
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
        theme::apply(ctx, Theme::Dark);
        let (manager, startup_error) = match storage::data_directory() {
            Ok(directory) => (Some(Manager::start(directory, wake)), None),
            Err(e) => (None, Some(format!("{e:#}"))),
        };
        Self {
            icon,
            applied_theme: Theme::Dark,
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
            folder_request: None,
            folder_pending: None,
            folder_generation: 0,
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

    pub fn snapshot(&self) -> Snapshot {
        self.manager
            .as_ref()
            .map(Manager::snapshot)
            .unwrap_or_default()
    }
    pub fn show_queue(&mut self) {
        self.settings_open = false;
    }
    pub fn show_new_download(&mut self) {
        let snapshot = self.snapshot();
        self.settings_open = false;
        self.open_add(&snapshot);
    }
    pub fn command(&mut self, command: Command) {
        self.send(command);
    }
    pub fn tray_unavailable(&mut self, error: &str) {
        self.local_error = Some(format!(
            "System tray unavailable. Closing the window will exit. {error}"
        ));
    }

    pub fn popup_size(&self) -> (u32, u32) {
        let snapshot = self.snapshot();
        let count = snapshot
            .state
            .downloads
            .iter()
            .filter(|d| d.status != Status::Completed)
            .count()
            .min(4);
        (400, (192 + count * 120).clamp(240, 600) as u32)
    }

    pub fn popup(&mut self, ctx: &egui::Context) -> Option<crate::tray::Command> {
        let snapshot = self.snapshot();
        self.sync_theme(ctx, &snapshot);
        let colors = palette(ctx);
        let mut action = None;
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            return Some(crate::tray::Command::Popup { x: 0., y: 0. });
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(colors.surface)
                    .stroke(Stroke::new(1., colors.border))
                    .inner_margin(16.),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::Image::new(&self.icon).fit_to_exact_size(Vec2::splat(24.)));
                    ui.label(RichText::new("Downloads").size(18.).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let close = ui.small_button("×").on_hover_text("Close · Esc");
                        close.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Close popup")
                        });
                        if close.clicked() {
                            action = Some(crate::tray::Command::Popup { x: 0., y: 0. });
                        }
                        if ui
                            .small_button("+ New")
                            .on_hover_text("New download")
                            .clicked()
                        {
                            action = Some(crate::tray::Command::NewDownload);
                        }
                    });
                });
                let active = snapshot
                    .state
                    .downloads
                    .iter()
                    .filter(|d| d.status == Status::Downloading)
                    .count();
                let waiting = snapshot
                    .state
                    .downloads
                    .iter()
                    .filter(|d| d.status == Status::Queued)
                    .count();
                secondary(
                    ui,
                    &if active + waiting > 0 {
                        format!("{active} active · {waiting} waiting")
                    } else {
                        "Your downloads, at a glance".into()
                    },
                );
                ui.add_space(2.);
                ui.separator();
                let mut jobs: Vec<_> = snapshot
                    .state
                    .downloads
                    .iter()
                    .filter(|d| d.status != Status::Completed)
                    .collect();
                jobs.sort_by_key(|d| match d.status {
                    Status::Downloading => 0,
                    Status::Queued => 1,
                    Status::Pausing => 2,
                    Status::Paused => 3,
                    Status::Failed => 4,
                    _ => 5,
                });
                let available = (ui.available_height() - 54.).max(80.);
                egui::ScrollArea::vertical()
                    .id_salt("tray_downloads")
                    .max_height(available)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if jobs.is_empty() {
                            ui.add_space(18.);
                            section(ui, "No downloads waiting");
                            secondary(ui, "Add a link to start a new download.");
                        }
                        for job in jobs.iter().take(4) {
                            ui.add_space(6.);
                            ui.horizontal(|ui| {
                                let width = (ui.available_width() - 92.).max(80.);
                                ui.allocate_ui_with_layout(
                                    Vec2::new(width, 28.),
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(
                                                    job.destination
                                                        .file_name()
                                                        .unwrap_or_default()
                                                        .to_string_lossy(),
                                                )
                                                .strong(),
                                            )
                                            .truncate(),
                                        )
                                        .on_hover_text(job.destination.display().to_string());
                                    },
                                );
                                let button = match job.status {
                                    Status::Downloading | Status::Queued => {
                                        Some(("Pause", Command::Pause(job.id)))
                                    }
                                    Status::Paused | Status::Canceled => {
                                        Some(("Resume", Command::Resume(job.id)))
                                    }
                                    Status::Failed => Some(("Retry", Command::Resume(job.id))),
                                    _ => None,
                                };
                                if let Some((label, command)) = button
                                    && ui.small_button(label).clicked()
                                {
                                    self.send(command);
                                }
                            });
                            let (label, color) = match job.status {
                                Status::Downloading => (
                                    format!("{}/s", bytes(job.speed.max(0.) as u64)),
                                    colors.accent,
                                ),
                                Status::Failed => {
                                    ("Needs attention · partial file kept".into(), colors.error)
                                }
                                Status::Queued => ("Waiting".into(), colors.muted),
                                Status::Pausing => ("Pausing…".into(), colors.muted),
                                Status::Canceled => ("Canceled".into(), colors.muted),
                                _ => ("Paused".into(), colors.muted),
                            };
                            ui.label(RichText::new(label).size(13.).color(color));
                            let fraction = job
                                .total
                                .filter(|n| *n > 0)
                                .map(|n| job.bytes as f32 / n as f32)
                                .unwrap_or(0.)
                                .clamp(0., 1.);
                            ui.scope(|ui| {
                                ui.visuals_mut().extreme_bg_color = colors.background;
                                ui.add(
                                    egui::ProgressBar::new(fraction)
                                        .fill(colors.accent)
                                        .animate(false)
                                        .desired_height(5.)
                                        .desired_width(ui.available_width()),
                                );
                            });
                            ui.label(
                                RichText::new(format!(
                                    "{} / {}",
                                    bytes(job.bytes),
                                    job.total
                                        .map(bytes)
                                        .unwrap_or_else(|| "unknown size".into())
                                ))
                                .size(13.)
                                .color(colors.muted),
                            );
                            ui.add_space(4.);
                            ui.separator();
                        }
                    });
                ui.add_space(6.);
                ui.horizontal(|ui| {
                    if ui
                        .add_sized([ui.available_width(), 34.], primary(ui, "Open downloads"))
                        .clicked()
                    {
                        action = Some(crate::tray::Command::Queue);
                    }
                });
            });
        action
    }

    pub fn open_settings(&mut self) {
        self.folder_generation = self.folder_generation.wrapping_add(1);
        if let Some(manager) = &self.manager {
            self.settings = manager.snapshot().state.settings;
            self.settings_directory = self.settings.default_directory.display().to_string();
            self.settings_open = true;
            self.saved_settings = false;
        }
    }
    pub fn settings_visible(&self) -> bool {
        self.settings_open
    }

    fn choose_folder(&mut self, target: FolderTarget) {
        if self.folder_pending.is_some() {
            return;
        }
        let initial = match target {
            FolderTarget::NewDownload => &self.directory,
            FolderTarget::Defaults => &self.settings_directory,
        };
        self.folder_pending = Some((self.folder_generation, target));
        self.folder_request = Some(FolderRequest {
            generation: self.folder_generation,
            target,
            initial: PathBuf::from(initial),
        });
    }

    pub fn take_folder_request(&mut self) -> Option<FolderRequest> {
        self.folder_request.take()
    }

    pub fn folder_selected(
        &mut self,
        generation: u64,
        target: FolderTarget,
        result: Result<Option<PathBuf>, String>,
    ) {
        if self.folder_pending != Some((generation, target)) {
            return;
        }
        self.folder_pending = None;
        if generation != self.folder_generation
            || !match target {
                FolderTarget::NewDownload => self.add_open,
                FolderTarget::Defaults => self.settings_open,
            }
        {
            return;
        }
        let result = result.and_then(|chosen| {
            chosen
                .map(|path| {
                    path.into_os_string().into_string().map_err(|_| {
                        "Choose another folder: this folder name cannot be represented as text."
                            .to_owned()
                    })
                })
                .transpose()
        });
        match result {
            Ok(Some(path)) => match target {
                FolderTarget::NewDownload => {
                    self.directory = path;
                    self.form_error = None;
                }
                FolderTarget::Defaults => {
                    self.settings_directory = path;
                    self.local_error = None;
                }
            },
            Ok(None) => {}
            Err(error) => {
                let message =
                    format!("Couldn't choose a folder. You can enter its path manually. {error}");
                match target {
                    FolderTarget::NewDownload => self.form_error = Some(message),
                    FolderTarget::Defaults => self.local_error = Some(message),
                }
            }
        }
    }

    fn sync_theme(&mut self, ctx: &egui::Context, snapshot: &Snapshot) {
        let selected = if self.settings_open {
            self.settings.theme
        } else {
            snapshot.state.settings.theme
        };
        if selected != self.applied_theme {
            theme::apply(ctx, selected);
            self.applied_theme = selected;
            ctx.request_repaint();
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
        self.folder_generation = self.folder_generation.wrapping_add(1);
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
        let colors = palette(ctx);
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
            if folder_field(
                ui,
                &mut self.directory,
                self.folder_pending.is_some() || self.pending_add.is_some(),
            ) {
                self.choose_folder(FolderTarget::NewDownload);
            }
            secondary(
                ui,
                "Use an existing folder. Existing files are never replaced.",
            );
            if let Some(error) = &self.form_error {
                ui.colored_label(colors.error, error);
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let pending = self.pending_add.is_some();
                if ui
                    .add_enabled(
                        !pending,
                        primary(ui, if pending { "Adding…" } else { "Download" }),
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
        let colors = palette(ui.ctx());
        ui.spacing_mut().item_spacing.y = 8.;
        ui.spacing_mut().button_padding = egui::vec2(12., 6.);
        ui.spacing_mut().interact_size.y = 32.;
        ui.spacing_mut().scroll.floating = false;
        ui.horizontal(|ui| {
            if ui.button("‹ Downloads").clicked() {
                self.settings_open = false;
            }
            ui.label(RichText::new("Settings").size(22.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("Version {VERSION}"))
                        .size(13.)
                        .color(colors.muted),
                );
            });
        });
        ui.add_space(6.0);
        let scroll_height = (ui.available_height() - 52.0).max(120.0);
        egui::ScrollArea::vertical().id_salt("settings").max_height(scroll_height).auto_shrink([false, false]).show(ui, |ui| {
            section(ui, "Downloads");
            ui.label("Save folder");
            if folder_field(ui, &mut self.settings_directory, self.folder_pending.is_some()) {
                self.choose_folder(FolderTarget::Defaults);
            }
            egui::Grid::new("download_limits").num_columns(2).spacing([24.0, 8.0]).show(ui, |ui| {
                ui.label("Simultaneous downloads");
                egui::ComboBox::from_id_salt("concurrency").selected_text(self.settings.concurrency.to_string()).width(84.0).show_ui(ui, |ui| {
                    for count in 1..=4 { ui.selectable_value(&mut self.settings.concurrency, count, count.to_string()); }
                }); ui.end_row();
                ui.label("Bandwidth");
                ui.horizontal(|ui| {
                    let mut limited = self.settings.bandwidth_kib > 0;
                    if ui.checkbox(&mut limited, "Limit speed").changed() { self.settings.bandwidth_kib = if limited { 1024 } else { 0 }; }
                    if limited { ui.add(egui::DragValue::new(&mut self.settings.bandwidth_kib).range(1..=1_048_576).suffix(" KiB/s")); }
                }); ui.end_row();
            });
            ui.add_space(4.0); ui.separator();
            ui.horizontal(|ui| {
                ui.label("Theme");
                egui::ComboBox::from_id_salt("theme").selected_text(match self.settings.theme { Theme::Dark => "Dark", Theme::Light => "Light" }).width(120.).show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.settings.theme, Theme::Dark, "Dark");
                    ui.selectable_value(&mut self.settings.theme, Theme::Light, "Light");
                });
            });
            ui.checkbox(&mut self.settings.close_to_tray,"Keep downloading when the window closes").on_hover_text("Closing hides the window in the tray. Use Quit in the tray menu to save and exit.");
            ui.separator();
            let busy = matches!(snapshot.update, UpdateStatus::Checking | UpdateStatus::Downloading { .. } | UpdateStatus::Staged(_));
            ui.horizontal(|ui| {
                section(ui, "Updates");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add_enabled(!busy, egui::Button::new("Check for updates")).clicked() { self.send(Command::CheckUpdates); }
                });
            });
            ui.checkbox(&mut self.settings.startup_update_check, "Check automatically at startup");
            match &snapshot.update {
                UpdateStatus::Idle => {}
                UpdateStatus::Checking => { secondary(ui, "Checking for an update…"); }
                UpdateStatus::Current => { ui.colored_label(colors.success, "You have the latest published version."); }
                UpdateStatus::Error(error) => {
                    ui.colored_label(colors.error, "Couldn't check for updates. Try again later.");
                    egui::CollapsingHeader::new("Error details").show(ui, |ui| { ui.label(error); });
                }
                UpdateStatus::Available(release) => {
                    ui.colored_label(colors.success, format!("Version {} is available.", release.version));
                    secondary(ui, "Your downloads will pause while the app updates and restarts. Files and settings are kept.");
                    if ui.add(primary(ui, "Download and install")).clicked() { self.send(Command::InstallUpdate); }
                }
                UpdateStatus::Downloading { bytes: done, total } => {
                    ui.label(format!("Downloading update · {} of {}", bytes(*done), bytes(*total)));
                    ui.add(egui::ProgressBar::new(*done as f32 / (*total).max(1) as f32).fill(colors.accent).animate(false));
                }
                UpdateStatus::Staged(_) => { secondary(ui, "Update verified. Saving your queue before restarting…"); }
            }
        });
        ui.add_space(4.0);
        ui.separator();
        ui.add_space(4.0);
        let mut draft = self.settings.clone();
        draft.default_directory = PathBuf::from(&self.settings_directory);
        let changed = draft != snapshot.state.settings;
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add_enabled(changed, primary(ui, "Save changes"))
                .clicked()
            {
                self.saved_settings = self.send(Command::Settings(draft.clone()));
            }
            if self.saved_settings && !changed && snapshot.notice.is_none() {
                ui.colored_label(colors.success, "Changes saved");
            } else if changed {
                secondary(ui, "Unsaved changes");
            }
        });
    }

    fn download_row(&mut self, ui: &mut egui::Ui, job: &Download) {
        let colors = palette(ui.ctx());
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
                Status::Queued => ("Waiting", colors.muted),
                Status::Downloading => ("Downloading", colors.accent),
                Status::Pausing => ("Pausing…", colors.muted),
                Status::Paused => ("Paused", colors.muted),
                Status::Canceled => ("Canceled", colors.muted),
                Status::Failed => ("Needs attention", colors.error),
                Status::Completed => ("Complete", colors.success),
            };
            ui.label(RichText::new(label).color(color).strong());
        });
        let host = url::Url::parse(&job.url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_owned))
            .unwrap_or_else(|| job.url.clone());
        ui.label(RichText::new(host).color(colors.muted).size(13.0))
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
            ui.visuals_mut().extreme_bg_color = colors.track;
            ui.add(
                egui::ProgressBar::new(fraction)
                    .fill(if job.status == Status::Completed {
                        colors.success
                    } else {
                        colors.accent
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
            ui.colored_label(colors.error, "Download stopped. Your partial file is kept.");
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
        self.sync_theme(ctx, &snapshot);
        let colors = palette(ctx);
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
            .frame(
                egui::Frame::new()
                    .fill(colors.background)
                    .inner_margin(if self.settings_open { 18.0 } else { 24.0 }),
            )
            .show(ctx, |ui| {
                if self.settings_open {
                    if let Some(error) = self.local_error.as_ref().or(snapshot.notice.as_ref()) {
                        ui.colored_label(colors.error, error);
                    }
                    self.settings_page(ui, &snapshot);
                    return;
                }
                ui.horizontal(|ui| {
                    ui.add(egui::Image::new(&self.icon).fit_to_exact_size(Vec2::splat(32.)));
                    ui.heading(RichText::new("Downloads").strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(
                                snapshot.ready && !snapshot.stopping,
                                primary(ui, "+ New download"),
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
                    ui.colored_label(colors.error, "DownloadManager couldn't open your queue.");
                    ui.label(error);
                    return;
                }
                if !snapshot.ready {
                    secondary(ui, "Loading your saved downloads…");
                    return;
                }
                if let Some(error) = self.local_error.as_ref().or(snapshot.notice.as_ref()) {
                    ui.colored_label(colors.error, error);
                }
                if matches!(snapshot.update, UpdateStatus::Available(_)) {
                    ui.label(
                        RichText::new("An update is available in Settings.").color(colors.accent),
                    );
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
                            painter.circle_filled(center, 32.0, colors.badge);
                            painter.line_segment(
                                [
                                    center - egui::vec2(0.0, 15.0),
                                    center + egui::vec2(0.0, 7.0),
                                ],
                                Stroke::new(3.0, colors.accent),
                            );
                            painter.line_segment(
                                [
                                    center + egui::vec2(-8.0, 0.0),
                                    center + egui::vec2(0.0, 8.0),
                                ],
                                Stroke::new(3.0, colors.accent),
                            );
                            painter.line_segment(
                                [center + egui::vec2(8.0, 0.0), center + egui::vec2(0.0, 8.0)],
                                Stroke::new(3.0, colors.accent),
                            );
                            painter.line_segment(
                                [
                                    center + egui::vec2(-12.0, 18.0),
                                    center + egui::vec2(12.0, 18.0),
                                ],
                                Stroke::new(3.0, colors.accent),
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
                                    .fill(colors.surface)
                                    .inner_margin(18.0)
                                    .corner_radius(8.0)
                                    .stroke(Stroke::new(1.0, colors.border))
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
                    if ui.add(primary(ui, "Restart download")).clicked() { self.send(Command::Restart(id)); self.restart = None; }
                    if ui.button("Keep partial file").clicked() { self.restart = None; }
                });
            });
            if modal.should_close() {
                self.restart = None;
            }
        }
        self.sync_theme(ctx, &snapshot);
    }
}

fn primary<'a>(ui: &egui::Ui, text: &'a str) -> egui::Button<'a> {
    let colors = palette(ui.ctx());
    egui::Button::new(RichText::new(text).color(Color32::WHITE).strong())
        .fill(colors.primary)
        .stroke(Stroke::NONE)
        .corner_radius(6.0)
}
fn secondary(ui: &mut egui::Ui, text: &str) {
    let colors = palette(ui.ctx());
    ui.label(RichText::new(text).color(colors.muted));
}

fn folder_field(ui: &mut egui::Ui, value: &mut String, busy: bool) -> bool {
    ui.horizontal(|ui| {
        let input_width = (ui.available_width() - 104.0 - ui.spacing().item_spacing.x).max(80.0);
        ui.add_sized(
            [input_width, ui.spacing().interact_size.y],
            egui::TextEdit::singleline(value),
        );
        ui.add_enabled(!busy, egui::Button::new("Browse…"))
            .on_hover_text(if busy {
                "A folder selection is already in progress"
            } else {
                "Choose a folder"
            })
            .clicked()
    })
    .inner
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
