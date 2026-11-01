use crate::theme::{palette, title};
use egui::{Color32, Pos2, Response, Sense, Stroke, StrokeKind, Ui, Vec2};

#[derive(Clone, Copy)]
pub enum Glyph {
    Plus,
    Close,
    Settings,
    Back,
    Download,
    Check,
    Alert,
}

pub fn glyph(ui: &Ui, rect: egui::Rect, icon: Glyph, color: Color32) {
    let p = ui.painter();
    let c = rect.center();
    let at = |x, y| c + egui::vec2(x, y) * (rect.width() / 24.);
    let stroke = Stroke::new(1.6_f32, color);
    let line = |a: (f32, f32), b: (f32, f32)| {
        p.line_segment([at(a.0, a.1), at(b.0, b.1)], stroke);
    };
    match icon {
        Glyph::Plus => {
            line((-6., 0.), (6., 0.));
            line((0., -6.), (0., 6.));
        }
        Glyph::Close => {
            line((-4.5, -4.5), (4.5, 4.5));
            line((-4.5, 4.5), (4.5, -4.5));
        }
        Glyph::Back => {
            line((2., -5.), (-3., 0.));
            line((-3., 0.), (2., 5.));
        }
        Glyph::Download => {
            line((0., -7.), (0., 3.));
            line((-4., -1.), (0., 3.));
            line((4., -1.), (0., 3.));
            line((-7., 4.), (-7., 8.));
            line((-7., 8.), (7., 8.));
            line((7., 8.), (7., 4.));
        }
        Glyph::Check => {
            line((-5., 0.), (-1., 4.));
            line((-1., 4.), (6., -4.));
        }
        Glyph::Alert => {
            p.circle_stroke(c, rect.width() * 0.34, stroke);
            line((0., -4.), (0., 1.));
            p.circle_filled(at(0., 5.), 1., color);
        }
        Glyph::Settings => {
            p.circle_stroke(c, rect.width() * 0.26, stroke);
            p.circle_stroke(c, rect.width() * 0.10, stroke);
            for n in 0..8 {
                let a = n as f32 * std::f32::consts::TAU / 8.;
                let v = egui::vec2(a.cos(), a.sin());
                p.line_segment(
                    [c + v * rect.width() * 0.26, c + v * rect.width() * 0.36],
                    stroke,
                );
            }
        }
    }
}

pub fn icon_button(ui: &mut Ui, icon: Glyph, label: &str) -> Response {
    let colors = palette(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(32.), Sense::click());
    if response.hovered() || response.has_focus() {
        ui.painter().rect_filled(rect, 7., colors.hover);
        if response.has_focus() {
            ui.painter().rect_stroke(
                rect,
                7.,
                Stroke::new(1.0_f32, colors.accent),
                StrokeKind::Inside,
            );
        }
    }
    glyph(
        ui,
        egui::Rect::from_center_size(rect.center(), Vec2::splat(22.)),
        icon,
        colors.muted,
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    response.on_hover_text(label)
}

pub fn file_icon(ui: &mut Ui, filename: &str, size: f32) {
    let colors = palette(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    ui.painter().rect_filled(rect, 9., colors.badge);
    let c = rect.center();
    let s = size / 44.;
    let at = |x, y| c + egui::vec2(x, y) * s;
    let stroke = Stroke::new(1.3_f32, colors.accent);
    ui.painter().add(egui::Shape::line(
        vec![
            at(-9., -13.),
            at(3., -13.),
            at(10., -6.),
            at(10., 12.),
            at(-9., 12.),
            at(-9., -13.),
        ],
        stroke,
    ));
    ui.painter().add(egui::Shape::line(
        vec![at(3., -13.), at(3., -6.), at(10., -6.)],
        stroke,
    ));
    let extension = std::path::Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .filter(|e| e.len() <= 4)
        .unwrap_or("FILE")
        .to_uppercase();
    ui.painter().text(
        at(0., 4.),
        egui::Align2::CENTER_CENTER,
        extension,
        egui::FontId::new(8.5 * s, egui::FontFamily::Name("semibold".into())),
        colors.accent,
    );
}

pub fn badge(ui: &mut Ui, text: &str, foreground: Color32, background: Color32) {
    let galley = ui.painter().layout_no_wrap(
        text.into(),
        egui::FontId::new(12., egui::FontFamily::Name("semibold".into())),
        foreground,
    );
    let size = galley.size() + egui::vec2(14., 4.);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_filled(rect, 5., background);
    ui.painter()
        .galley(rect.center() - galley.size() / 2., galley, foreground);
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, ui.is_enabled(), text));
}

pub fn progress(ui: &mut Ui, fraction: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 4.), Sense::hover());
    ui.painter().rect_filled(rect, 2., palette(ui.ctx()).track);
    if fraction > 0. {
        ui.painter().rect_filled(
            egui::Rect::from_min_size(
                rect.min,
                egui::vec2(rect.width() * fraction.clamp(0., 1.), 4.),
            ),
            2.,
            color,
        );
    }
}

pub fn tab(ui: &mut Ui, label: &str, count: usize, selected: bool) -> Response {
    let colors = palette(ui.ctx());
    let text = format!("{label}   {count}");
    let response = ui.add(
        egui::Button::new(title(text, 14.).color(if selected { colors.ink } else { colors.muted }))
            .frame(false)
            .min_size(egui::vec2(110., 38.)),
    );
    if selected {
        let r = response.rect;
        ui.painter().line_segment(
            [
                Pos2::new(r.left() + 10., r.bottom()),
                Pos2::new(r.right() - 10., r.bottom()),
            ],
            Stroke::new(2.0_f32, colors.accent),
        );
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            label,
        )
    });
    response
}

pub fn switch(ui: &mut Ui, value: &mut bool, label: &str) -> Response {
    let colors = palette(ui.ctx());
    let (rect, mut response) = ui.allocate_exact_size(egui::vec2(34., 20.), Sense::click());
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }
    ui.painter().rect_filled(
        rect,
        10.,
        if *value { colors.primary } else { colors.track },
    );
    ui.painter().circle_filled(
        egui::pos2(
            if *value {
                rect.right() - 10.
            } else {
                rect.left() + 10.
            },
            rect.center().y,
        ),
        7.,
        if *value { Color32::WHITE } else { colors.muted },
    );
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect.expand(3.),
            12.,
            Stroke::new(1.0_f32, colors.accent),
            StrokeKind::Inside,
        );
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), *value, label)
    });
    response.on_hover_text(label)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn switch_frame(
        ctx: &egui::Context,
        value: &mut bool,
        events: Vec<egui::Event>,
        enabled: bool,
    ) -> Response {
        let mut response = None;
        let _ = ctx.run(
            egui::RawInput {
                events,
                screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, Vec2::new(300., 200.))),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    response = Some(
                        ui.add_enabled_ui(enabled, |ui| switch(ui, value, "Check on startup"))
                            .inner,
                    );
                });
            },
        );
        response.unwrap()
    }

    #[test]
    fn keyboard_can_toggle_a_focused_switch_and_disabled_switch_stays_unchanged() {
        let ctx = egui::Context::default();
        let mut value = false;
        let initial = switch_frame(&ctx, &mut value, vec![], true);
        ctx.memory_mut(|m| m.request_focus(initial.id));
        let key = |pressed| egui::Event::Key {
            key: egui::Key::Space,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let activated = switch_frame(&ctx, &mut value, vec![key(true)], true);
        assert!(value && activated.changed());
        switch_frame(&ctx, &mut value, vec![key(false)], true);
        switch_frame(&ctx, &mut value, vec![key(true)], false);
        assert!(value, "A disabled switch must not change its setting");
    }
}
