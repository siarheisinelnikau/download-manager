mod components;
use crate::folder_picker::{Request as FolderRequest, Target as FolderTarget};
use crate::theme::{self, palette, title};
use components::{Glyph, badge, file_icon, glyph, icon_button, progress, switch, tab};
use download_manager::{
    i18n::Language,
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
    applied_language: Language,
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
            applied_language: Language::English,
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
    pub fn show_completed(&mut self) {
        self.settings_open = false;
        self.completed = true;
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
        let language = self.applied_language;
        self.local_error = Some(language.format(
            "System tray unavailable. Closing the window will exit. {error}",
            &[("error", error.to_owned())],
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
        let language = self.applied_language;
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
                    ui.label(title(language.text("Downloads"), 18.));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icon_button(ui, Glyph::Close, language.text("Close popup · Esc"))
                            .clicked()
                        {
                            action = Some(crate::tray::Command::Popup { x: 0., y: 0. });
                        }
                        if icon_button(ui, Glyph::Plus, language.text("New download")).clicked() {
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
                    language.text("Ready when you are").into()
                } else if active > 0 {
                    language.format(
                        "{active} downloading · {count} in queue",
                        &[
                            ("active", active.to_string()),
                            ("count", jobs.len().to_string()),
                        ],
                    )
                } else {
                    language.format("{count} in queue", &[("count", jobs.len().to_string())])
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
                                    ui.label(title(language.text("Nothing in the queue"), 15.));
                                    small(ui, language.text("Use + to add your next download."));
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
                                        let (label, color) =
                                            status_label(job.status, colors, language);
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
                                if let Some((label, command)) = job_action(job, language)
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
                                        .unwrap_or_else(|| language.text("Unknown size").into())
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
                    egui::Button::new(title(language.text("Open downloads"), 14.))
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
            language: self.applied_language,
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
        let language = self.applied_language;
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
                        language.text("Choose another folder: this folder name cannot be represented as text.")
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
                let message = language.format(
                    "Couldn't choose a folder. You can enter its path manually. {error}",
                    &[("error", error.to_string())],
                );
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
        let selected_language = if self.settings_open {
            self.settings.language
        } else {
            snapshot.state.settings.language
        };
        if selected_language != self.applied_language {
            self.applied_language = selected_language;
            ctx.request_repaint();
        }
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
        let language = self.applied_language;
        let validation = validate_url(&self.url).and_then(|_| validate_filename(&self.filename));
        if let Err(error) = validation {
            self.form_error = Some(language.text(&error.to_string()).to_owned());
            return;
        }
        if !PathBuf::from(&self.directory).is_absolute() {
            self.form_error = Some(
                language
                    .text("Enter the full path to an existing folder.")
                    .into(),
            );
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
        let language = self.applied_language;
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
                    ui.label(title(language.text("New download"), 22.));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled_ui(self.pending_add.is_none(), |ui| {
                                icon_button(ui, Glyph::Close, language.text("Close new download"))
                            })
                            .inner
                            .clicked()
                        {
                            self.add_open = false;
                        }
                    });
                });
                secondary(
                    ui,
                    language.text("Paste the file link and choose where to save it."),
                );
                ui.add_space(12.);
                field_label(ui, language.text("Download link"));
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
                field_label(ui, language.text("File name"));
                if ui
                    .add_sized(
                        [ui.available_width(), 38.],
                        egui::TextEdit::singleline(&mut self.filename)
                            .id(egui::Id::new("download-filename"))
                            .margin(egui::vec2(10., 9.))
                            .hint_text(language.text("Filled in from your link")),
                    )
                    .changed()
                {
                    self.custom_filename = true;
                }
                ui.add_space(4.);
                field_label(ui, language.text("Save to"));
                if folder_field(
                    ui,
                    language,
                    "download-folder",
                    &mut self.directory,
                    self.folder_pending.is_some() || self.pending_add.is_some(),
                ) {
                    self.choose_folder(FolderTarget::NewDownload);
                }
                small(ui, language.text("Existing files are never replaced."));
                if let Some(error) = &self.form_error {
                    error_notice(ui, language, error, colors.error, colors.error_surface);
                }
                ui.add_space(12.);
                ui.separator();
                ui.add_space(6.);
                ui.horizontal(|ui| {
                    small(ui, language.text("Enter to download"));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let pending = self.pending_add.is_some();
                        if ui
                            .add_enabled(
                                !pending,
                                primary(
                                    ui,
                                    if pending {
                                        language.text("Adding…")
                                    } else {
                                        language.text("Download")
                                    },
                                ),
                            )
                            .clicked()
                        {
                            self.submit_add(snapshot);
                        }
                        if ui
                            .add_enabled(
                                !pending,
                                egui::Button::new(language.text("Cancel")).frame(false),
                            )
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
        let language = self.applied_language;
        let colors = palette(ui.ctx());
        ui.spacing_mut().item_spacing.y = 6.;
        ui.spacing_mut().button_padding = egui::vec2(10., 6.);
        ui.spacing_mut().interact_size.y = 30.;
        ui.horizontal(|ui| {
            if icon_button(
                ui,
                Glyph::Back,
                language.text("Back to downloads · Discard unsaved changes"),
            )
            .clicked()
            {
                self.settings_open = false;
            }
            ui.label(title(language.text("Settings"), 22.));
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
                settings_group(ui, language.text("Downloads"), |ui| {
                    ui.horizontal(|ui| {
                        ui.label(language.text("Save folder"));
                        ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), 34.),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                if folder_field(
                                    ui,
                                    language,
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
                    setting_row(ui, language.text("Simultaneous downloads"), |ui| {
                        egui::ComboBox::from_id_salt("concurrency")
                            .selected_text(self.settings.concurrency.to_string())
                            .width(110.)
                            .show_ui(ui, |ui| {
                                for n in 1..=4 {
                                    ui.selectable_value(
                                        &mut self.settings.concurrency,
                                        n,
                                        n.to_string(),
                                    );
                                }
                            });
                    });
                    setting_row(ui, language.text("Download speed"), |ui| {
                        let mut limited = self.settings.bandwidth_kib > 0;
                        if switch(ui, &mut limited, language.text("Limit download speed")).changed() {
                            self.settings.bandwidth_kib = if limited { 1024 } else { 0 };
                        }
                        if limited {
                            ui.add(
                                egui::DragValue::new(&mut self.settings.bandwidth_kib)
                                    .range(1..=1_048_576)
                                    .suffix(" KiB/s")
                                    .speed(64.),
                            );
                        } else {
                            small(ui, language.text("Unlimited"));
                        }
                    });
                });
                ui.add_space(6.);
                settings_group(ui, language.text("Appearance & behavior"), |ui| {
                    setting_row(ui, language.text("Language"), |ui| {
                        egui::ComboBox::from_id_salt("language")
                            .selected_text(self.settings.language.native_name())
                            .width(140.)
                            .show_ui(ui, |ui| {
                                for option in Language::ALL {
                                    ui.selectable_value(&mut self.settings.language, option, option.native_name());
                                }
                            });
                    });
                    setting_row(ui, language.text("Theme"), |ui| {
                        egui::ComboBox::from_id_salt("theme")
                            .selected_text(match self.settings.theme {
                                Theme::Dark => language.text("Dark"),
                                Theme::Light => language.text("Light"),
                            })
                            .width(110.)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.settings.theme, Theme::Dark, language.text("Dark"));
                                ui.selectable_value(
                                    &mut self.settings.theme,
                                    Theme::Light,
                                    language.text("Light"),
                                );
                            });
                    });
                    setting_row(ui, language.text("Keep running when closed"), |ui| {
                        switch(
                            ui,
                            &mut self.settings.close_to_tray,
                            language.text("Keep downloads running in the tray when the window closes"),
                        );
                    });
                });
                ui.add_space(6.);
                settings_group(ui, language.text("Updates"), |ui| {
                    let busy = matches!(
                        snapshot.update,
                        UpdateStatus::Checking
                            | UpdateStatus::Downloading { .. }
                            | UpdateStatus::Staged(_)
                    );
                    ui.horizontal(|ui| {
                        ui.label(language.text("Check on startup"));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add_enabled(!busy, egui::Button::new(language.text("Check for updates")))
                                .clicked()
                            {
                                self.send(Command::CheckUpdates);
                            }
                            switch(
                                ui,
                                &mut self.settings.startup_update_check,
                                language.text("Check for updates automatically at startup"),
                            );
                        });
                    });
                    match &snapshot.update {
                        UpdateStatus::Idle => {}
                        UpdateStatus::Checking => {
                            small(ui, language.text("Checking for an update…"));
                        }
                        UpdateStatus::Current => {
                            ui.colored_label(colors.success, language.text("You're up to date."));
                        }
                        UpdateStatus::Error(error) => {
                            ui.colored_label(
                                colors.error,
                                language.text("Couldn't check for updates. Try again later."),
                            );
                            egui::CollapsingHeader::new(language.text("Error details")).show(ui, |ui| {
                                ui.label(language.text(error));
                            });
                        }
                        UpdateStatus::Available(release) => {
                            ui.label(
                                title(language.format("Version {version} is available", &[("version", release.version.to_string())]), 15.)
                                    .color(colors.success),
                            );
                            small(
                                ui,
                                language.text("Downloads pause during installation. Files and settings are kept."),
                            );
                            if ui.add(primary(ui, language.text("Download and install"))).clicked() {
                                self.send(Command::InstallUpdate);
                            }
                        }
                        UpdateStatus::Downloading { bytes: done, total } => {
                            small(
                                ui,
                                &language.format("Downloading update · {done} / {total}", &[("done", bytes(*done)), ("total", bytes(*total))]),
                            );
                            progress(ui, *done as f32 / (*total).max(1) as f32, colors.accent);
                        }
                        UpdateStatus::Staged(_) => {
                            small(ui, language.text("Update verified. Saving your queue before restarting…"));
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
                    egui::Label::new(
                        RichText::new(language.error_summary(error))
                            .size(12.)
                            .color(colors.error),
                    )
                    .truncate(),
                )
                .on_hover_text(error);
        } else if self.saved_settings && !changed {
            small(&mut footer, language.text("All changes saved"));
        } else if changed {
            small(&mut footer, language.text("Unsaved changes"));
        } else {
            small(&mut footer, language.text("Changes apply after saving"));
        }
        let save_rect = egui::Rect::from_min_max(
            egui::pos2(footer_rect.right() - 116., footer_rect.top()),
            footer_rect.max,
        );
        let mut save = ui.new_child(egui::UiBuilder::new().max_rect(save_rect));
        if save
            .add_enabled(
                changed,
                primary(&save, language.text("Save changes")).min_size(save_rect.size()),
            )
            .clicked()
        {
            self.saved_settings = self.send(Command::Settings(draft));
        }
    }

    fn download_row(&mut self, ui: &mut egui::Ui, job: &Download) {
        let language = self.applied_language;
        let colors = palette(ui.ctx());
        let filename = job
            .destination
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        ui.spacing_mut().item_spacing.y = 5.;
        ui.spacing_mut().interact_size.y = 20.;
        egui::Frame::new().inner_margin(16.).show(ui, |ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), 52.),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    file_icon(ui, &filename, 44.);
                    ui.add_space(2.);
                    let action_width = if job.status == Status::Completed {
                        160.
                    } else if job_action(job, language).is_some() {
                        118.
                    } else {
                        32.
                    };
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
                                    .unwrap_or_else(|| language.text("Download").into());
                                small(ui, &host);
                                let (label, color) = status_label(job.status, colors, language);
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
                        if job.status == Status::Completed
                            && ui
                                .button(language.text("Open folder"))
                                .on_hover_text(job.destination.display().to_string())
                                .clicked()
                        {
                            self.open_folder(job);
                        }
                        if let Some((label, command)) = job_action(job, language)
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
            if job.status != Status::Completed {
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
            }
            ui.horizontal(|ui| {
                small(
                    ui,
                    &if job.status == Status::Completed {
                        language.format(
                            "{size} · Saved to {folder}",
                            &[
                                ("size", bytes(job.bytes)),
                                (
                                    "folder",
                                    job.destination
                                        .parent()
                                        .and_then(|p| p.file_name())
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                        .into_owned(),
                                ),
                            ],
                        )
                    } else {
                        format!(
                            "{} / {}",
                            bytes(job.bytes),
                            job.total
                                .map(bytes)
                                .unwrap_or_else(|| language.text("Unknown size").into())
                        )
                    },
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if job.status == Status::Downloading {
                        if let Some(total) = job.total.filter(|_| job.speed > 0.) {
                            small(
                                ui,
                                &language.format(
                                    "{time} left",
                                    &[(
                                        "time",
                                        duration(
                                            total.saturating_sub(job.bytes) as f64 / job.speed,
                                            language,
                                        ),
                                    )],
                                ),
                            );
                        }
                        ui.label(
                            title(format!("{}/s", bytes(job.speed.max(0.) as u64)), 12.)
                                .color(colors.accent),
                        );
                    } else if job.status != Status::Completed && job.total.is_some() {
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
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let (rect, _) =
                                ui.allocate_exact_size(Vec2::splat(18.), egui::Sense::hover());
                            glyph(ui, rect, Glyph::Alert, colors.error);
                            ui.add(
                                egui::Label::new(
                                    RichText::new(language.error_summary(error))
                                        .size(13.)
                                        .color(colors.error),
                                )
                                .truncate(),
                            )
                            .on_hover_text(error);
                        });
                        egui::CollapsingHeader::new(
                            language.text("Partial file saved · Recovery options"),
                        )
                        .id_salt(job.id)
                        .show(ui, |ui| {
                            ui.label(language.text(error));
                            if matches!(
                                job.status,
                                Status::Failed | Status::Paused | Status::Canceled
                            ) && ui.button(language.text("Restart from zero…")).clicked()
                            {
                                self.restart = Some(job.id);
                            }
                        });
                    });
            }
        });
    }

    fn job_menu(&mut self, ui: &mut egui::Ui, job: &Download) {
        let language = self.applied_language;
        let button = egui::Button::new("•••")
            .frame(false)
            .min_size(egui::vec2(32., 32.));
        let response = egui::containers::menu::MenuButton::from_button(button)
            .ui(ui, |ui| {
                ui.set_max_width(420.);
                field_label(ui, language.text("Save location"));
                ui.label(job.destination.display().to_string());
                ui.separator();
                if job.status == Status::Queued
                    && ui.button(language.text("Move up in queue")).clicked()
                {
                    self.send(Command::MoveUp(job.id));
                    ui.close();
                }
                if matches!(
                    job.status,
                    Status::Paused | Status::Failed | Status::Canceled
                ) && ui.button(language.text("Restart from zero…")).clicked()
                {
                    self.restart = Some(job.id);
                    ui.close();
                }
                if matches!(
                    job.status,
                    Status::Downloading | Status::Queued | Status::Paused | Status::Failed
                ) && ui.button(language.text("Cancel download")).clicked()
                {
                    self.send(Command::Cancel(job.id));
                    ui.close();
                }
                if job.status == Status::Completed
                    && ui.button(language.text("Remove from history")).clicked()
                {
                    self.send(Command::RemoveCompleted(job.id));
                    ui.close();
                }
            })
            .0;
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                ui.is_enabled(),
                language.text("More download actions"),
            )
        });
        response.on_hover_text(language.text("More download actions"));
    }

    fn open_folder(&mut self, job: &Download) {
        let language = self.applied_language;
        #[cfg(windows)]
        if let Some(folder) = job.destination.parent()
            && let Err(error) = std::process::Command::new("explorer.exe")
                .arg(folder)
                .spawn()
        {
            self.local_error = Some(language.format(
                "Couldn't open the save folder. {folder}: {error}",
                &[
                    ("folder", folder.display().to_string()),
                    ("error", error.to_string()),
                ],
            ));
        }
        #[cfg(not(windows))]
        {
            let _ = job;
            self.local_error = Some(
                language
                    .text("Opening a save folder is currently supported on Windows only.")
                    .into(),
            );
        }
    }

    pub fn update(&mut self, ctx: &egui::Context) {
        let snapshot = self.snapshot();
        self.sync_theme(ctx, &snapshot);
        let language = self.applied_language;
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
                                icon_button(ui, Glyph::Settings, language.text("Settings"))
                            })
                            .inner
                            .clicked()
                        {
                            self.open_settings();
                        }
                        if ui
                            .add_enabled(
                                snapshot.ready && !snapshot.stopping,
                                primary(ui, language.text("+ New download")),
                            )
                            .on_hover_text(language.text("New download · Ctrl+N"))
                            .clicked()
                        {
                            self.open_add(&snapshot);
                        }
                    });
                });
                ui.add_space(10.);
                ui.label(title(language.text("Downloads"), 28.));
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
                    if self.completed {
                        language.text("Open a save folder or manage your history.")
                    } else if active > 0 {
                        language.text("Your downloads are on their way.")
                    } else if pending > 0 {
                        language.text("Pick up where you left off.")
                    } else {
                        language.text("Your files, in one place.")
                    },
                );
                ui.add_space(4.);
                if let Some(error) = self.startup_error.as_ref().or(snapshot.fatal.as_ref()) {
                    notice(
                        ui,
                        language.text("Couldn't open your queue"),
                        colors.error,
                        colors.error_surface,
                    );
                    egui::CollapsingHeader::new(language.text("Error details")).show(ui, |ui| {
                        ui.label(language.text(error));
                    });
                    return;
                }
                if !snapshot.ready {
                    secondary(ui, language.text("Loading your saved downloads…"));
                    return;
                }
                if let Some(error) = self.local_error.as_ref().or(snapshot.notice.as_ref()) {
                    error_notice(ui, language, error, colors.error, colors.error_surface);
                }
                if matches!(snapshot.update, UpdateStatus::Available(_))
                    && ui
                        .link(language.text("Update available · Open settings"))
                        .clicked()
                {
                    self.open_settings();
                }
                if snapshot.stopping {
                    secondary(ui, language.text("Saving your downloads before closing…"));
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
                        if tab(ui, language.text("Queue"), pending, !self.completed).clicked() {
                            self.completed = false;
                        }
                        if tab(ui, language.text("Completed"), completed, self.completed).clicked()
                        {
                            self.completed = true;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if !self.completed {
                                if snapshot.state.downloads.iter().any(|j| {
                                    matches!(j.status, Status::Downloading | Status::Queued)
                                }) && ui
                                    .add(egui::Button::new(language.text("Pause all")).frame(false))
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
                                        .add(
                                            egui::Button::new(language.text("Resume paused"))
                                                .frame(false),
                                        )
                                        .clicked()
                                {
                                    self.send(Command::ResumeAll);
                                }
                            }
                        });
                    });
                    ui.separator();
                    ui.add_space(4.);
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
                                language.text("A home for your finished downloads")
                            } else {
                                language.text("Your next download starts here")
                            },
                            22.,
                        ));
                        secondary(
                            &mut empty,
                            if self.completed {
                                language.text("Completed files stay in the folder you chose.")
                            } else {
                                language
                                    .text("Paste a link. Choose a folder. We'll handle the rest.")
                            },
                        );
                        empty.add_space(10.);
                        if !self.completed {
                            if empty
                                .add(primary(&empty, language.text("+ Add a download")))
                                .clicked()
                            {
                                self.open_add(&snapshot);
                            }
                            small(&mut empty, language.text("or press Ctrl+N"));
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
                        language.format(
                            "{count} in queue · {errors} need attention",
                            &[
                                ("count", pending.to_string()),
                                ("errors", failures.to_string()),
                            ],
                        )
                    } else if pending > 0 {
                        language.format(
                            "{count} in queue · {active} downloading",
                            &[
                                ("count", pending.to_string()),
                                ("active", active.to_string()),
                            ],
                        )
                    } else {
                        language.text("Ready for a new download").into()
                    },
                );
                footer.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    small(
                        ui,
                        if snapshot.state.settings.close_to_tray {
                            language.text("Close to keep downloading in the tray")
                        } else {
                            language.text("Closing exits DownloadManager")
                        },
                    );
                });
            });
        self.add_dialog(ctx, &snapshot);
        if let Some(id) = self.restart {
            let modal = egui::Modal::new(egui::Id::new("restart_download"))
                .frame(egui::Frame::new().fill(colors.surface).inner_margin(24.).corner_radius(12.).stroke(Stroke::new(1., colors.border)))
                .show(ctx, |ui| {
                    ui.set_max_width(420.);
                    ui.label(title(language.text("Restart this download?"), 22.));
                    ui.label(language.text("This discards the partial file and downloads it again. Existing completed files are kept."));
                    ui.add_space(12.);
                    ui.horizontal(|ui| {
                        if ui.button(language.text("Keep partial file")).clicked() { self.restart = None; }
                        if ui.add(primary(ui, language.text("Restart download"))).clicked() {
                            self.send(Command::Restart(id));
                            self.restart = None;
                        }
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
fn folder_field(
    ui: &mut egui::Ui,
    language: Language,
    id: &str,
    value: &mut String,
    busy: bool,
) -> bool {
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
            egui::Button::new(language.text("Browse…")).min_size(egui::vec2(95., 34.)),
        )
        .on_hover_text(language.text("Choose a folder"))
        .clicked()
    })
    .inner
}
fn error_notice(ui: &mut egui::Ui, language: Language, diagnostic: &str, fg: Color32, bg: Color32) {
    notice(ui, language.error_summary(diagnostic), fg, bg);
    if language.error_summary(diagnostic) != language.text(diagnostic) {
        egui::CollapsingHeader::new(language.text("Error details")).show(ui, |ui| {
            ui.label(diagnostic);
        });
    }
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
        .inner_margin(egui::Margin::symmetric(12, 6))
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
fn status_label(status: Status, c: theme::Palette, language: Language) -> (&'static str, Color32) {
    match status {
        Status::Queued => (language.text("Waiting"), c.muted),
        Status::Downloading => (language.text("Downloading"), c.accent),
        Status::Pausing => (language.text("Pausing…"), c.muted),
        Status::Paused => (language.text("Paused"), c.muted),
        Status::Canceled => (language.text("Canceled"), c.muted),
        Status::Failed => (language.text("Needs attention"), c.error),
        Status::Completed => (language.text("Complete"), c.success),
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
fn job_action(job: &Download, language: Language) -> Option<(&'static str, Command)> {
    match job.status {
        Status::Downloading | Status::Queued => {
            Some((language.text("Pause"), Command::Pause(job.id)))
        }
        Status::Paused | Status::Canceled => {
            Some((language.text("Resume"), Command::Resume(job.id)))
        }
        Status::Failed => Some((language.text("Retry"), Command::Resume(job.id))),
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
fn duration(seconds: f64, language: Language) -> String {
    let seconds = seconds.max(0.0) as u64;
    if seconds >= 3600 {
        language.format(
            "{hours}h {minutes}m",
            &[
                ("hours", (seconds / 3600).to_string()),
                ("minutes", (seconds % 3600 / 60).to_string()),
            ],
        )
    } else {
        language.format(
            "{minutes}m {seconds}s",
            &[
                ("minutes", (seconds / 60).to_string()),
                ("seconds", (seconds % 60).to_string()),
            ],
        )
    }
}
