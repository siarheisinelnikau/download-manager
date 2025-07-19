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
}

impl Palette {
    pub fn for_theme(theme: Theme) -> Self {
        match theme {
            Theme::Dark => Self {
                background: Color32::from_rgb(18, 24, 32),
                surface: Color32::from_rgb(27, 36, 48),
                ink: Color32::from_rgb(233, 238, 246),
                muted: Color32::from_rgb(162, 176, 194),
                border: Color32::from_rgb(51, 65, 84),
                accent: Color32::from_rgb(134, 180, 255),
                primary: Color32::from_rgb(49, 108, 205),
                error: Color32::from_rgb(255, 157, 163),
                success: Color32::from_rgb(125, 215, 172),
                hover: Color32::from_rgb(38, 54, 75),
                selection: Color32::from_rgb(40, 70, 110),
                track: Color32::from_rgb(43, 56, 74),
                badge: Color32::from_rgb(33, 56, 83),
            },
            Theme::Light => Self {
                background: Color32::from_rgb(244, 246, 250),
                surface: Color32::WHITE,
                ink: Color32::from_rgb(28, 39, 57),
                muted: Color32::from_rgb(83, 99, 120),
                border: Color32::from_rgb(218, 225, 234),
                accent: Color32::from_rgb(33, 99, 209),
                primary: Color32::from_rgb(33, 99, 209),
                error: Color32::from_rgb(166, 47, 51),
                success: Color32::from_rgb(29, 111, 77),
                hover: Color32::from_rgb(234, 241, 253),
                selection: Color32::from_rgb(219, 233, 255),
                track: Color32::from_rgb(231, 236, 244),
                badge: Color32::from_rgb(226, 236, 253),
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
    visuals.extreme_bg_color = colors.surface;
    visuals.faint_bg_color = colors.background;
    visuals.hyperlink_color = colors.accent;
    visuals.error_fg_color = colors.error;
    visuals.warn_fg_color = colors.error;
    visuals.selection.bg_fill = colors.selection;
    visuals.selection.stroke = Stroke::new(1., colors.ink);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1., colors.muted);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1., colors.border);
    visuals.widgets.inactive.bg_fill = colors.surface;
    visuals.widgets.inactive.weak_bg_fill = colors.surface;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1., colors.border);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1., colors.ink);
    visuals.widgets.hovered.bg_fill = colors.hover;
    visuals.widgets.hovered.weak_bg_fill = colors.hover;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1., colors.accent);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1., colors.ink);
    visuals.widgets.active.bg_fill = colors.selection;
    visuals.widgets.active.weak_bg_fill = colors.selection;
    visuals.widgets.active.bg_stroke = Stroke::new(1., colors.accent);
    visuals.widgets.active.fg_stroke = Stroke::new(1., colors.ink);
    style.visuals = visuals;
    ctx.set_style_of(target, style);
    ctx.set_theme(target);
}
