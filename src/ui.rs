mod components;
use crate::folder_picker::{Request as FolderRequest, Target as FolderTarget};
use crate::theme::{self, palette, title};
use components::{Glyph, badge, file_icon, glyph, icon_button, progress, switch, tab};
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
    popup_bounds: Option<(egui::Rect, egui::Rect)>,
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
        theme::configure(ctx);
        theme::apply(ctx, Theme::Dark);
        let (manager, startup_error) = match storage::data_directory() {
            Ok(directory) => (Some(Manager::start(directory, wake)), None),
            Err(e) => (None, Some(format!("{e:#}"))),
        };
        Self {
            popup_bounds: None,
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
        (400, (156 + count * 108).clamp(256, 588) as u32)
    }
    pub fn popup_bounds(&self) -> Option<(egui::Rect, egui::Rect)> {
        self.popup_bounds
    }

    pub fn popup(&mut self, ctx: &egui::Context) -> Option<crate::tray::Command> {
        self.popup_bounds = None;
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
                let bounds = ui.max_rect();
                ui.horizontal(|ui| {
                    ui.add(egui::Image::new(&self.icon).fit_to_exact_size(Vec2::splat(24.)));
                    ui.label(title("Downloads", 18.));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icon_button(ui, Glyph::Close, "Close popup · Esc").clicked() {
                            action = Some(crate::tray::Command::Popup { x: 0., y: 0. });
                        }
                        if icon_button(ui, Glyph::Plus, "New download").clicked() {
                            action = Some(crate::tray::Command::NewDownload);
                        }
                    });
                });
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
                let active = jobs
                    .iter()
                    .filter(|d| d.status == Status::Downloading)
                    .count();
                let status = if jobs.is_empty() {
                    "Ready when you are".into()
                } else if active > 0 {
                    format!("{active} downloading · {} in queue", jobs.len())
                } else {
                    format!("{} in queue", jobs.len())
                };
                small(ui, &status);
                ui.add_space(4.);
                ui.separator();
                let height = (ui.text_style_height(&egui::TextStyle::Button)
                    + 2. * ui.spacing().button_padding.y)
                    .max(ui.spacing().interact_size.y)
                    .ceil();
                let footer_rect = egui::Rect::from_min_max(
                    egui::pos2(bounds.left(), bounds.bottom() - height),
                    bounds.max,
                );
                let body_rect = egui::Rect::from_min_max(
                    ui.cursor().min,
                    egui::pos2(bounds.right(), footer_rect.top() - 12.),
                );
                let mut body = ui.new_child(egui::UiBuilder::new().max_rect(body_rect));
                body.set_clip_rect(body_rect.intersect(ui.clip_rect()));
                egui::ScrollArea::vertical()
                    .id_salt("tray_downloads")
                    .max_height(body_rect.height().max(1.))
                    .auto_shrink([false, false])
                    .show(&mut body, |ui| {
                        if jobs.is_empty() {
                            ui.add_space(12.);
                            ui.horizontal(|ui| {
                                file_icon(ui, "download", 40.);
                                ui.vertical(|ui| {
                                    ui.label(title("Nothing in the queue", 15.));
                                    small(ui, "Use + to add your next download.");
                                });
                            });
                        }
                        for (index, job) in jobs.iter().take(4).enumerate() {
                            ui.add_space(8.);
                            let filename = job
                                .destination
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy();
                            ui.horizontal(|ui| {
                                file_icon(ui, &filename, 36.);
                                ui.allocate_ui_with_layout(
                                    egui::vec2((ui.available_width() - 84.).max(100.), 40.),
                                    egui::Layout::top_down(egui::Align::Min),
                                    |ui| {
                                        ui.add(
                                            egui::Label::new(title(filename.as_ref(), 14.))
                                                .truncate(),
                                        )
                                        .on_hover_text(job.destination.display().to_string());
                                        let (label, color) = status_label(job.status, colors);
                                        ui.label(
                                            RichText::new(if job.status == Status::Downloading {
                                                format!("{}/s", bytes(job.speed.max(0.) as u64))
                                            } else {
                                                label.into()
                                            })
                                            .size(12.)
                                            .color(color),
                                        );
                                    },
                                );
                                if let Some((label, command)) = job_action(job)
                                    && ui
                                        .add(
                                            egui::Button::new(title(label, 12.))
                                                .min_size(egui::vec2(72., 28.)),
                                        )
                                        .clicked()
                                {
                                    self.send(command);
                                }
                            });
                            progress(
                                ui,
                                fraction(job),
                                if job.status == Status::Failed {
                                    colors.error
                                } else {
                                    colors.accent
                                },
                            );
                            small(
                                ui,
                                &format!(
                                    "{} / {}",
                                    bytes(job.bytes),
                                    job.total
                                        .map(bytes)
                                        .unwrap_or_else(|| "Unknown size".into())
                                ),
                            );
                            if index + 1 < jobs.len().min(4) {
                                ui.add_space(4.);
                                ui.separator();
                            }
                        }
                    });
                let footer = ui.put(
                    footer_rect,
                    egui::Button::new(title("Open downloads", 14.))
                        .fill(colors.hover)
                        .stroke(Stroke::new(1., colors.border))
                        .corner_radius(7.),
                );
                self.popup_bounds = Some((bounds, footer.rect));
                if footer.clicked() {
                    action = Some(crate::tray::Command::Queue);
                }
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
        self.completed = false;
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
        let submit_from_keyboard = self.pending_add.is_none()
            && ctx.memory(|m| {
                m.focused().is_some_and(|id| {
                    ["download-url", "download-filename", "download-folder"]
                        .iter()
                        .any(|name| id == egui::Id::new(name))
                })
            })
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
        let frame = egui::Frame::new()
            .fill(colors.surface)
            .stroke(Stroke::new(1., colors.border))
            .corner_radius(12.)
            .inner_margin(24.);
        let modal = egui::Modal::new(egui::Id::new("new_download"))
            .frame(frame)
            .show(ctx, |ui| {
                ui.set_width((ctx.content_rect().width() - 112.).clamp(320., 488.));
                ui.horizontal(|ui| {
                    ui.label(title("New download", 22.));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled_ui(self.pending_add.is_none(), |ui| {
                                icon_button(ui, Glyph::Close, "Close new download")
                            })
                            .inner
                            .clicked()
                        {
                            self.add_open = false;
                        }
                    });
                });
                secondary(ui, "Paste the file link and choose where to save it.");
                ui.add_space(12.);
                field_label(ui, "Download link");
                let response = ui.add_sized(
                    [ui.available_width(), 38.],
                    egui::TextEdit::singleline(&mut self.url)
                        .id(egui::Id::new("download-url"))
                        .margin(egui::vec2(10., 9.))
                        .hint_text("https://example.com/file.zip"),
                );
                if self.focus_url {
                    response.request_focus();
                    self.focus_url = false;
                }
                if response.changed() && !self.custom_filename {
                    self.filename = suggested_filename(&self.url);
                }
                ui.add_space(4.);
                field_label(ui, "File name");
                if ui
                    .add_sized(
                        [ui.available_width(), 38.],
                        egui::TextEdit::singleline(&mut self.filename)
                            .id(egui::Id::new("download-filename"))
                            .margin(egui::vec2(10., 9.))
                            .hint_text("Filled in from your link"),
                    )
                    .changed()
                {
                    self.custom_filename = true;
                }
                ui.add_space(4.);
                field_label(ui, "Save to");
                if folder_field(
                    ui,
                    "download-folder",
                    &mut self.directory,
                    self.folder_pending.is_some() || self.pending_add.is_some(),
                ) {
                    self.choose_folder(FolderTarget::NewDownload);
                }
                small(ui, "Existing files are never replaced.");
                if let Some(error) = &self.form_error {
                    notice(ui, error, colors.error, colors.error_surface);
                }
                ui.add_space(12.);
                ui.separator();
                ui.add_space(6.);
                ui.horizontal(|ui| {
                    small(ui, "Enter to download");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
                            .add_enabled(!pending, egui::Button::new("Cancel").frame(false))
                            .clicked()
                        {
                            self.add_open = false;
                        }
                    });
                });
            });
        if submit_from_keyboard {
            self.submit_add(snapshot);
        }
        if modal.should_close() && self.pending_add.is_none() {
            self.add_open = false;
        }
    }

    fn settings_page(&mut self, ui: &mut egui::Ui, snapshot: &Snapshot) {
        let colors = palette(ui.ctx());
        ui.spacing_mut().item_spacing.y = 6.;
        ui.spacing_mut().button_padding = egui::vec2(10., 6.);
        ui.spacing_mut().interact_size.y = 30.;
        ui.horizontal(|ui| {
            if icon_button(
                ui,
                Glyph::Back,
                "Back to downloads · Discard unsaved changes",
            )
            .clicked()
            {
                self.settings_open = false;
            }
            ui.label(title("Settings", 22.));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                small(ui, &format!("DownloadManager  {VERSION}"));
            });
        });
        ui.add_space(8.);
        let full = ui.available_rect_before_wrap();
        let footer_rect =
            egui::Rect::from_min_max(egui::pos2(full.left(), full.bottom() - 36.), full.max);
        let body_rect =
            egui::Rect::from_min_max(full.min, egui::pos2(full.right(), footer_rect.top() - 16.));
        let mut body = ui.new_child(egui::UiBuilder::new().max_rect(body_rect));
        body.set_clip_rect(body_rect.intersect(ui.clip_rect()));
        egui::ScrollArea::vertical()
            .id_salt("settings")
            .max_height(body_rect.height())
            .auto_shrink([false, false])
            .show(&mut body, |ui| {
                settings_group(ui, "Downloads", |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Save folder");
                        ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), 34.),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                if folder_field(
                                    ui,
                                    "default-folder",
                                    &mut self.settings_directory,
                                    self.folder_pending.is_some(),
                                ) {
                                    self.choose_folder(FolderTarget::Defaults);
                                }
                            },
                        );
                    });
                    ui.add_space(3.);
                    ui.separator();
                    setting_row(ui, "Simultaneous downloads", |ui| {
                        egui::ComboBox::from_id_salt("concurrency")
                            .selected_text(format!("{} files", self.settings.concurrency))
                            .width(110.)
                            .show_ui(ui, |ui| {
                                for n in 1..=4 {
                                    ui.selectable_value(
                                        &mut self.settings.concurrency,
                                        n,
                                        format!("{n} {}", if n == 1 { "file" } else { "files" }),
                                    );
                                }
                            });
                    });
                    setting_row(ui, "Download speed", |ui| {
                        let mut limited = self.settings.bandwidth_kib > 0;
                        ui.horizontal(|ui| {
                            if limited {
                                ui.add(
                                    egui::DragValue::new(&mut self.settings.bandwidth_kib)
                                        .range(1..=1_048_576)
                                        .suffix(" KiB/s")
                                        .speed(64.),
                                );
                            } else {
                                small(ui, "Unlimited");
                            }
                            switch(ui, &mut limited, "Limit download speed");
                        });
                        if limited != (self.settings.bandwidth_kib > 0) {
                            self.settings.bandwidth_kib = if limited { 1024 } else { 0 };
                        }
                    });
                });
                ui.add_space(10.);
                settings_group(ui, "Appearance & behavior", |ui| {
                    setting_row(ui, "Theme", |ui| {
                        egui::ComboBox::from_id_salt("theme")
                            .selected_text(match self.settings.theme {
                                Theme::Dark => "Dark",
                                Theme::Light => "Light",
                            })
                            .width(110.)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.settings.theme, Theme::Dark, "Dark");
                                ui.selectable_value(
                                    &mut self.settings.theme,
                                    Theme::Light,
                                    "Light",
                                );
                            });
                    });
                    setting_row(ui, "Keep running when closed", |ui| {
                        switch(
                            ui,
                            &mut self.settings.close_to_tray,
                            "Keep downloads running in the tray when the window closes",
                        );
                    });
                });
                ui.add_space(10.);
                settings_group(ui, "Updates", |ui| {
                    let busy = matches!(
                        snapshot.update,
                        UpdateStatus::Checking
                            | UpdateStatus::Downloading { .. }
                            | UpdateStatus::Staged(_)
                    );
                    ui.horizontal(|ui| {
                        ui.label("Check on startup");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add_enabled(!busy, egui::Button::new("Check for updates"))
                                .clicked()
                            {
                                self.send(Command::CheckUpdates);
                            }
                            switch(
                                ui,
                                &mut self.settings.startup_update_check,
                                "Check for updates automatically at startup",
                            );
                        });
                    });
                    match &snapshot.update {
                        UpdateStatus::Idle => {}
                        UpdateStatus::Checking => {
                            small(ui, "Checking for an update…");
                        }
                        UpdateStatus::Current => {
                            ui.colored_label(colors.success, "You're up to date.");
                        }
                        UpdateStatus::Error(error) => {
                            ui.colored_label(
                                colors.error,
                                "Couldn't check for updates. Try again later.",
                            );
                            egui::CollapsingHeader::new("Error details").show(ui, |ui| {
                                ui.label(error);
                            });
                        }
                        UpdateStatus::Available(release) => {
                            ui.label(
                                title(format!("Version {} is available", release.version), 15.)
                                    .color(colors.success),
                            );
                            small(
                                ui,
                                "Downloads pause during installation. Files and settings are kept.",
                            );
                            if ui.add(primary(ui, "Download and install")).clicked() {
                                self.send(Command::InstallUpdate);
                            }
                        }
                        UpdateStatus::Downloading { bytes: done, total } => {
                            small(
                                ui,
                                &format!(
                                    "Downloading update · {} / {}",
                                    bytes(*done),
                                    bytes(*total)
                                ),
                            );
                            progress(ui, *done as f32 / (*total).max(1) as f32, colors.accent);
                        }
                        UpdateStatus::Staged(_) => {
                            small(ui, "Update verified. Saving your queue before restarting…");
                        }
                    }
                });
            });
        let mut draft = self.settings.clone();
        draft.default_directory = PathBuf::from(&self.settings_directory);
        let changed = draft != snapshot.state.settings;
        let mut footer = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(egui::Rect::from_min_max(
                    footer_rect.min,
                    egui::pos2(footer_rect.right() - 128., footer_rect.bottom()),
                ))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        if let Some(error) = self.local_error.as_ref().or(snapshot.notice.as_ref()) {
            footer
                .add(
                    egui::Label::new(RichText::new(error).size(12.).color(colors.error)).truncate(),
                )
                .on_hover_text(error);
        } else if self.saved_settings && !changed {
            small(&mut footer, "All changes saved");
        } else if changed {
            small(&mut footer, "Unsaved changes");
        } else {
            small(&mut footer, "Changes apply after saving");
        }
        let save_rect = egui::Rect::from_min_max(
            egui::pos2(footer_rect.right() - 116., footer_rect.top()),
            footer_rect.max,
        );
        let mut save = ui.new_child(egui::UiBuilder::new().max_rect(save_rect));
        if save
            .add_enabled(
                changed,
                primary(&save, "Save changes").min_size(save_rect.size()),
            )
            .clicked()
        {
            self.saved_settings = self.send(Command::Settings(draft));
        }
    }

    fn download_row(&mut self, ui: &mut egui::Ui, job: &Download) {
        let colors = palette(ui.ctx());
        let filename = job
            .destination
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        ui.spacing_mut().item_spacing.y = 7.;
        ui.spacing_mut().interact_size.y = 26.;
        egui::Frame::new().inner_margin(16.).show(ui, |ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), 56.),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    file_icon(ui, &filename, 44.);
                    ui.add_space(2.);
                    let action_width = if job_action(job).is_some() { 118. } else { 32. };
                    let width = (ui.available_width() - action_width - 10.).max(160.);
                    ui.allocate_ui_with_layout(
                        egui::vec2(width, 40.),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.add(egui::Label::new(title(filename.as_ref(), 16.)).truncate())
                                .on_hover_text(job.destination.display().to_string());
                            ui.horizontal(|ui| {
                                let host = url::Url::parse(&job.url)
                                    .ok()
                                    .and_then(|u| u.host_str().map(str::to_owned))
                                    .unwrap_or_else(|| "Download".into());
                                small(ui, &host);
                                let (label, color) = status_label(job.status, colors);
                                badge(
                                    ui,
                                    label,
                                    color,
                                    match job.status {
                                        Status::Failed => colors.error_surface,
                                        Status::Completed => colors.success_surface,
                                        Status::Downloading => colors.badge,
                                        _ => colors.hover,
                                    },
                                );
                            });
                        },
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        self.job_menu(ui, job);
                        if let Some((label, command)) = job_action(job)
                            && ui
                                .add(
                                    egui::Button::new(title(label, 13.))
                                        .min_size(egui::vec2(74., 32.)),
                                )
                                .clicked()
                        {
                            self.send(command);
                        }
                    });
                },
            );
            ui.add_space(3.);
            progress(
                ui,
                fraction(job),
                match job.status {
                    Status::Completed => colors.success,
                    Status::Failed => colors.error,
                    Status::Downloading => colors.accent,
                    _ => colors.muted,
                },
            );
            ui.horizontal(|ui| {
                small(
                    ui,
                    &if job.status == Status::Completed {
                        format!(
                            "{} · Saved to {}",
                            bytes(job.bytes),
                            job.destination
                                .parent()
                                .and_then(|p| p.file_name())
                                .unwrap_or_default()
                                .to_string_lossy()
                        )
                    } else {
                        format!(
                            "{} / {}",
                            bytes(job.bytes),
                            job.total
                                .map(bytes)
                                .unwrap_or_else(|| "Unknown size".into())
                        )
                    },
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if job.status == Status::Downloading {
                        if let Some(total) = job.total.filter(|_| job.speed > 0.) {
                            small(
                                ui,
                                &format!(
                                    "{} left",
                                    duration(total.saturating_sub(job.bytes) as f64 / job.speed)
                                ),
                            );
                        }
                        ui.label(
                            title(format!("{}/s", bytes(job.speed.max(0.) as u64)), 12.)
                                .color(colors.accent),
                        );
                    } else if job.total.is_some() {
                        small(ui, &format!("{:.0}%", fraction(job) * 100.));
                    }
                });
            });
            if let Some(error) = &job.error {
                egui::Frame::new()
                    .fill(colors.error_surface)
                    .inner_margin(egui::Margin::symmetric(10, 8))
                    .corner_radius(6.)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (rect, _) =
                                ui.allocate_exact_size(Vec2::splat(18.), egui::Sense::hover());
                            glyph(ui, rect, Glyph::Alert, colors.error);
                            ui.add(
                                egui::Label::new(
                                    RichText::new(error).size(13.).color(colors.error),
                                )
                                .truncate(),
                            )
                            .on_hover_text(error);
                        });
                        egui::CollapsingHeader::new("Partial file saved · Recovery options")
                            .id_salt(job.id)
                            .show(ui, |ui| {
                                ui.label(error);
                                if matches!(
                                    job.status,
                                    Status::Failed | Status::Paused | Status::Canceled
                                ) && ui.button("Restart from zero…").clicked()
                                {
                                    self.restart = Some(job.id);
                                }
                            });
                    });
            }
        });
    }

    fn job_menu(&mut self, ui: &mut egui::Ui, job: &Download) {
        let button = egui::Button::new("•••")
            .frame(false)
            .min_size(egui::vec2(32., 32.));
        egui::containers::menu::MenuButton::from_button(button)
            .ui(ui, |ui| {
                ui.set_max_width(420.);
                field_label(ui, "Save location");
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
            })
            .0
            .on_hover_text("More download actions");
    }

    pub fn update(&mut self, ctx: &egui::Context) {
        let snapshot = self.snapshot();
        self.sync_theme(ctx, &snapshot);
        let colors = palette(ctx);
        if snapshot.stopped && snapshot.fatal.is_none() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if snapshot.ready
            && !snapshot.stopping
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::N))
        {
            self.open_add(&snapshot);
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(colors.background)
                    .inner_margin(if self.settings_open { 20. } else { 28. }),
            )
            .show(ctx, |ui| {
                if self.settings_open {
                    self.settings_page(ui, &snapshot);
                    return;
                }
                ui.horizontal(|ui| {
                    ui.add(egui::Image::new(&self.icon).fit_to_exact_size(Vec2::splat(24.)));
                    ui.label(title("DownloadManager", 15.));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled_ui(snapshot.ready, |ui| {
                                icon_button(ui, Glyph::Settings, "Settings")
                            })
                            .inner
                            .clicked()
                        {
                            self.open_settings();
                        }
                        if ui
                            .add_enabled(
                                snapshot.ready && !snapshot.stopping,
                                primary(ui, "+ New download"),
                            )
                            .on_hover_text("New download · Ctrl+N")
                            .clicked()
                        {
                            self.open_add(&snapshot);
                        }
                    });
                });
                ui.add_space(22.);
                ui.label(title("Downloads", 28.));
                let completed = snapshot
                    .state
                    .downloads
                    .iter()
                    .filter(|j| j.status == Status::Completed)
                    .count();
                let active = snapshot
                    .state
                    .downloads
                    .iter()
                    .filter(|j| j.status == Status::Downloading)
                    .count();
                let pending = snapshot.state.downloads.len() - completed;
                secondary(
                    ui,
                    if active > 0 {
                        "Your downloads are on their way."
                    } else if pending > 0 {
                        "Pick up where you left off."
                    } else {
                        "Your files, in one place."
                    },
                );
                ui.add_space(14.);
                if let Some(error) = self.startup_error.as_ref().or(snapshot.fatal.as_ref()) {
                    notice(
                        ui,
                        "Couldn't open your queue",
                        colors.error,
                        colors.error_surface,
                    );
                    ui.label(error);
                    return;
                }
                if !snapshot.ready {
                    secondary(ui, "Loading your saved downloads…");
                    return;
                }
                if let Some(error) = self.local_error.as_ref().or(snapshot.notice.as_ref()) {
                    notice(ui, error, colors.error, colors.error_surface);
                }
                if matches!(snapshot.update, UpdateStatus::Available(_))
                    && ui.link("Update available · Open settings").clicked()
                {
                    self.open_settings();
                }
                if snapshot.stopping {
                    secondary(ui, "Saving your downloads before closing…");
                }
                let footer_height = 28.;
                let full = ui.available_rect_before_wrap();
                let footer_rect = egui::Rect::from_min_max(
                    egui::pos2(full.left(), full.bottom() - footer_height),
                    full.max,
                );
                let body_rect = egui::Rect::from_min_max(
                    full.min,
                    egui::pos2(full.right(), footer_rect.top() - 14.),
                );
                let mut body = ui.new_child(egui::UiBuilder::new().max_rect(body_rect));
                body.set_clip_rect(body_rect.intersect(ui.clip_rect()));
                body.add_enabled_ui(!snapshot.stopping, |ui| {
                    ui.horizontal(|ui| {
                        if tab(ui, "Queue", pending, !self.completed).clicked() {
                            self.completed = false;
                        }
                        if tab(ui, "Completed", completed, self.completed).clicked() {
                            self.completed = true;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if !self.completed {
                                if snapshot.state.downloads.iter().any(|j| {
                                    matches!(j.status, Status::Downloading | Status::Queued)
                                }) && ui
                                    .add(egui::Button::new("Pause all").frame(false))
                                    .clicked()
                                {
                                    self.send(Command::PauseAll);
                                }
                                if snapshot
                                    .state
                                    .downloads
                                    .iter()
                                    .any(|j| j.status == Status::Paused)
                                    && ui
                                        .add(egui::Button::new("Resume paused").frame(false))
                                        .clicked()
                                {
                                    self.send(Command::ResumeAll);
                                }
                            }
                        });
                    });
                    ui.separator();
                    ui.add_space(12.);
                    let jobs: Vec<_> = snapshot
                        .state
                        .downloads
                        .iter()
                        .filter(|j| (j.status == Status::Completed) == self.completed)
                        .collect();
                    if jobs.is_empty() {
                        let available = ui.available_rect_before_wrap();
                        let mut empty = ui.new_child(
                            egui::UiBuilder::new()
                                .max_rect(available)
                                .layout(egui::Layout::top_down(egui::Align::Center)),
                        );
                        empty.add_space(((available.height() - 220.) / 2.).max(12.));
                        let (rect, _) =
                            empty.allocate_exact_size(Vec2::splat(72.), egui::Sense::hover());
                        empty.painter().rect_filled(rect, 20., colors.badge);
                        glyph(
                            &empty,
                            egui::Rect::from_center_size(rect.center(), Vec2::splat(36.)),
                            if self.completed {
                                Glyph::Check
                            } else {
                                Glyph::Download
                            },
                            colors.accent,
                        );
                        empty.add_space(16.);
                        empty.label(title(
                            if self.completed {
                                "A home for your finished downloads"
                            } else {
                                "Your next download starts here"
                            },
                            22.,
                        ));
                        secondary(
                            &mut empty,
                            if self.completed {
                                "Completed files stay in the folder you chose."
                            } else {
                                "Paste a link. Choose a folder. We'll handle the rest."
                            },
                        );
                        empty.add_space(10.);
                        if !self.completed {
                            if empty.add(primary(&empty, "+ Add a download")).clicked() {
                                self.open_add(&snapshot);
                            }
                            small(&mut empty, "or press Ctrl+N");
                        }
                    } else {
                        egui::ScrollArea::vertical()
                            .id_salt(if self.completed {
                                "completed"
                            } else {
                                "downloads"
                            })
                            .max_height(ui.available_height())
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                egui::Frame::new()
                                    .fill(colors.surface)
                                    .corner_radius(10.)
                                    .stroke(Stroke::new(1., colors.border))
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.spacing_mut().item_spacing.y = 0.;
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
                let mut footer = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(footer_rect)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                footer.painter().line_segment(
                    [footer_rect.left_top(), footer_rect.right_top()],
                    Stroke::new(1., colors.border),
                );
                let failures = snapshot
                    .state
                    .downloads
                    .iter()
                    .filter(|j| j.status == Status::Failed)
                    .count();
                small(
                    &mut footer,
                    &if failures > 0 {
                        format!("{pending} in queue · {failures} needs attention")
                    } else if pending > 0 {
                        format!("{pending} in queue · {active} downloading")
                    } else {
                        "Ready for a new download".into()
                    },
                );
                footer.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    small(
                        ui,
                        if snapshot.state.settings.close_to_tray {
                            "Close to keep downloading in the tray"
                        } else {
                            "Closing exits DownloadManager"
                        },
                    );
                });
            });
        self.add_dialog(ctx, &snapshot);
        if let Some(id) = self.restart {
            let modal=egui::Modal::new(egui::Id::new("restart_download")).frame(egui::Frame::new().fill(colors.surface).inner_margin(24.).corner_radius(12.).stroke(Stroke::new(1.,colors.border))).show(ctx,|ui| {
                ui.set_max_width(420.);ui.label(title("Restart this download?",22.));ui.label("This discards the partial file and downloads it again. Existing completed files are kept.");ui.add_space(12.);
                ui.horizontal(|ui|{if ui.button("Keep partial file").clicked(){self.restart=None;}if ui.add(primary(ui,"Restart download")).clicked(){self.send(Command::Restart(id));self.restart=None;}});
            });
            if modal.should_close() {
                self.restart = None;
            }
        }
        self.sync_theme(ctx, &snapshot);
    }
}

fn primary<'a>(ui: &egui::Ui, text: &'a str) -> egui::Button<'a> {
    egui::Button::new(title(text, 14.).color(Color32::WHITE))
        .fill(palette(ui.ctx()).primary)
        .stroke(Stroke::NONE)
        .corner_radius(7.)
        .min_size(egui::vec2(0., 36.))
}
fn secondary(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).color(palette(ui.ctx()).muted));
}
fn small(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(12.).color(palette(ui.ctx()).muted));
}
fn field_label(ui: &mut egui::Ui, text: &str) {
    ui.label(title(text, 13.));
}
fn folder_field(ui: &mut egui::Ui, id: &str, value: &mut String, busy: bool) -> bool {
    ui.horizontal(|ui| {
        ui.add_sized(
            [
                (ui.available_width() - 95. - ui.spacing().item_spacing.x).max(80.),
                34.,
            ],
            egui::TextEdit::singleline(value)
                .id(egui::Id::new(id))
                .margin(egui::vec2(9., 7.)),
        )
        .on_hover_text(value.as_str());
        ui.add_enabled(
            !busy,
            egui::Button::new("Browse…").min_size(egui::vec2(95., 34.)),
        )
        .on_hover_text("Choose a folder")
        .clicked()
    })
    .inner
}
fn notice(ui: &mut egui::Ui, text: &str, fg: Color32, bg: Color32) {
    egui::Frame::new()
        .fill(bg)
        .corner_radius(7.)
        .inner_margin(10.)
        .show(ui, |ui| {
            ui.colored_label(fg, text);
        });
}
fn settings_group(ui: &mut egui::Ui, label: &str, contents: impl FnOnce(&mut egui::Ui)) {
    field_label(ui, label);
    ui.add_space(1.);
    egui::Frame::new()
        .fill(palette(ui.ctx()).surface)
        .stroke(Stroke::new(1., palette(ui.ctx()).border))
        .corner_radius(8.)
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            contents(ui);
        });
}
fn setting_row(ui: &mut egui::Ui, label: &str, contents: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.set_min_height(30.);
        ui.label(label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), contents);
    });
}
fn status_label(status: Status, c: theme::Palette) -> (&'static str, Color32) {
    match status {
        Status::Queued => ("Waiting", c.muted),
        Status::Downloading => ("Downloading", c.accent),
        Status::Pausing => ("Pausing…", c.muted),
        Status::Paused => ("Paused", c.muted),
        Status::Canceled => ("Canceled", c.muted),
        Status::Failed => ("Needs attention", c.error),
        Status::Completed => ("Complete", c.success),
    }
}
fn fraction(job: &Download) -> f32 {
    if job.status == Status::Completed {
        1.
    } else {
        job.total
            .filter(|n| *n > 0)
            .map(|n| job.bytes as f32 / n as f32)
            .unwrap_or(0.)
            .clamp(0., 1.)
    }
}
fn job_action(job: &Download) -> Option<(&'static str, Command)> {
    match job.status {
        Status::Downloading | Status::Queued => Some(("Pause", Command::Pause(job.id))),
        Status::Paused | Status::Canceled => Some(("Resume", Command::Resume(job.id))),
        Status::Failed => Some(("Retry", Command::Resume(job.id))),
        _ => None,
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
