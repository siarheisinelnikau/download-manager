use download_manager::model::Theme;
use egui::{Color32, Stroke};

#[derive(Clone, Copy)]
pub struct Palette {
    pub background: Color32,
    pub surface: Color32,
    pub ink: Color32,
    pub muted: Color32,
    pub border: Color32,
    pub accent: Color32,
    pub primary: Color32,
    pub error: Color32,
    pub success: Color32,
    pub hover: Color32,
    pub selection: Color32,
    pub track: Color32,
    pub badge: Color32,
    pub input: Color32,
    pub error_surface: Color32,
    pub success_surface: Color32,
}

impl Palette {
    pub fn for_theme(theme: Theme) -> Self {
        match theme {
            Theme::Dark => Self {
                background: Color32::from_rgb(17, 19, 24),
                surface: Color32::from_rgb(25, 28, 35),
                ink: Color32::from_rgb(238, 241, 248),
                muted: Color32::from_rgb(162, 171, 190),
                border: Color32::from_rgb(48, 53, 65),
                accent: Color32::from_rgb(149, 177, 255),
                primary: Color32::from_rgb(61, 99, 221),
                error: Color32::from_rgb(255, 171, 165),
                success: Color32::from_rgb(121, 215, 180),
                hover: Color32::from_rgb(37, 42, 53),
                selection: Color32::from_rgb(39, 53, 88),
                track: Color32::from_rgb(45, 50, 63),
                badge: Color32::from_rgb(36, 47, 73),
                input: Color32::from_rgb(20, 23, 30),
                error_surface: Color32::from_rgb(62, 39, 41),
                success_surface: Color32::from_rgb(29, 54, 48),
            },
            Theme::Light => Self {
                background: Color32::from_rgb(245, 246, 249),
                surface: Color32::WHITE,
                ink: Color32::from_rgb(31, 36, 49),
                muted: Color32::from_rgb(94, 105, 125),
                border: Color32::from_rgb(222, 226, 234),
                accent: Color32::from_rgb(50, 82, 187),
                primary: Color32::from_rgb(55, 88, 204),
                error: Color32::from_rgb(163, 51, 45),
                success: Color32::from_rgb(24, 112, 78),
                hover: Color32::from_rgb(237, 240, 247),
                selection: Color32::from_rgb(225, 232, 252),
                track: Color32::from_rgb(230, 234, 242),
                badge: Color32::from_rgb(231, 237, 253),
                input: Color32::from_rgb(249, 250, 252),
                error_surface: Color32::from_rgb(253, 237, 234),
                success_surface: Color32::from_rgb(228, 245, 236),
            },
        }
    }
}

pub fn palette(ctx: &egui::Context) -> Palette {
    Palette::for_theme(if ctx.style().visuals.dark_mode {
        Theme::Dark
    } else {
        Theme::Light
    })
}

pub fn apply(ctx: &egui::Context, theme: Theme) {
    let colors = Palette::for_theme(theme);
    let target = match theme {
        Theme::Dark => egui::Theme::Dark,
        Theme::Light => egui::Theme::Light,
    };
    let mut style = (*ctx.style()).clone();
    let mut visuals = match theme {
        Theme::Dark => egui::Visuals::dark(),
        Theme::Light => egui::Visuals::light(),
    };
    visuals.override_text_color = Some(colors.ink);
    visuals.panel_fill = colors.background;
    visuals.window_fill = colors.surface;
    visuals.extreme_bg_color = colors.input;
    visuals.faint_bg_color = colors.background;
    visuals.hyperlink_color = colors.accent;
    visuals.error_fg_color = colors.error;
    visuals.warn_fg_color = colors.error;
    visuals.selection.bg_fill = colors.selection;
    visuals.selection.stroke = Stroke::new(1., colors.ink);
    visuals.window_corner_radius = egui::CornerRadius::same(12);
    visuals.menu_corner_radius = egui::CornerRadius::same(8);
    visuals.window_stroke = Stroke::new(1., colors.border);
    visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(7);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1., colors.muted);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1., colors.border);
    visuals.widgets.inactive.bg_fill = colors.surface;
    visuals.widgets.inactive.weak_bg_fill = colors.surface;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1., colors.border);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1., colors.ink);
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(7);
    visuals.widgets.hovered.bg_fill = colors.hover;
    visuals.widgets.hovered.weak_bg_fill = colors.hover;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1., colors.accent);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1., colors.ink);
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(7);
    visuals.widgets.active.bg_fill = colors.selection;
    visuals.widgets.active.weak_bg_fill = colors.selection;
    visuals.widgets.active.bg_stroke = Stroke::new(1., colors.accent);
    visuals.widgets.active.fg_stroke = Stroke::new(1., colors.ink);
    visuals.widgets.active.corner_radius = egui::CornerRadius::same(7);
    style.visuals = visuals;
    ctx.set_style_of(target, style);
    ctx.set_theme(target);
}

pub fn configure(ctx: &egui::Context) {
    // Use the platform's installed UI fonts; keep egui's bundled fonts as fallback.
    let mut fonts = egui::FontDefinitions::default();
    if let Some(windows) = std::env::var_os("WINDIR") {
        let folder = std::path::PathBuf::from(windows).join("Fonts");
        if let Ok(data) = std::fs::read(folder.join("segoeui.ttf")) {
            fonts
                .font_data
                .insert("Segoe UI".into(), egui::FontData::from_owned(data).into());
            fonts
                .families
                .get_mut(&egui::FontFamily::Proportional)
                .unwrap()
                .insert(0, "Segoe UI".into());
        }
        if let Ok(data) = std::fs::read(folder.join("seguisb.ttf")) {
            fonts.font_data.insert(
                "Segoe UI Semibold".into(),
                egui::FontData::from_owned(data).into(),
            );
            let mut family = vec!["Segoe UI Semibold".into()];
            family.extend(fonts.families[&egui::FontFamily::Proportional].clone());
            fonts
                .families
                .insert(egui::FontFamily::Name("semibold".into()), family);
        }
    }
    if !fonts
        .families
        .contains_key(&egui::FontFamily::Name("semibold".into()))
    {
        fonts.families.insert(
            egui::FontFamily::Name("semibold".into()),
            fonts.families[&egui::FontFamily::Proportional].clone(),
        );
    }
    ctx.set_fonts(fonts);
    let mut style = (*ctx.style()).clone();
    style.animation_time = 0.;
    style.spacing.item_spacing = egui::vec2(10., 8.);
    style.spacing.button_padding = egui::vec2(12., 8.);
    style.spacing.interact_size = egui::vec2(36., 34.);
    style.spacing.text_edit_width = 220.;
    style.spacing.scroll.floating = false;
    for (name, size) in [
        (egui::TextStyle::Body, 14.),
        (egui::TextStyle::Button, 14.),
        (egui::TextStyle::Small, 12.),
        (egui::TextStyle::Heading, 26.),
    ] {
        style
            .text_styles
            .insert(name, egui::FontId::proportional(size));
    }
    ctx.set_style(style);
}

pub fn title(text: impl Into<String>, size: f32) -> egui::RichText {
    egui::RichText::new(text)
        .size(size)
        .family(egui::FontFamily::Name("semibold".into()))
}
