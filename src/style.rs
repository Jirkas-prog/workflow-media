use eframe::egui::{self, Color32, Pos2, RichText, Stroke, Vec2};
pub const BG: Color32 = Color32::from_rgb(20, 34, 43);
pub const PANEL: Color32 = Color32::from_rgb(31, 51, 63);
pub const LINE: Color32 = Color32::from_rgb(55, 80, 94);
pub const TEXT: Color32 = Color32::from_rgb(220, 235, 242);
pub const MUTED: Color32 = Color32::from_rgb(158, 187, 201);
pub const LIME: Color32 = Color32::from_rgb(196, 250, 103);
pub const BLUE: Color32 = Color32::from_rgb(126, 196, 218);
pub const GREEN: Color32 = Color32::from_rgb(139, 211, 178);
pub const RED: Color32 = Color32::from_rgb(255, 155, 127);
pub fn apply(ctx: &egui::Context) {
    let mut s = egui::Style::default();
    s.visuals = egui::Visuals::dark();
    s.visuals.override_text_color = Some(TEXT);
    s.visuals.panel_fill = BG;
    s.visuals.window_fill = PANEL;
    s.visuals.extreme_bg_color = Color32::from_rgb(17, 34, 43);
    s.visuals.text_edit_bg_color = Some(Color32::from_rgb(19, 37, 47));
    s.visuals.faint_bg_color = Color32::from_rgb(29, 50, 61);
    s.visuals.code_bg_color = BG;
    s.visuals.selection.bg_fill = Color32::from_rgb(55, 81, 67);
    s.visuals.selection.stroke = Stroke::new(1.5_f32, LIME);
    s.visuals.text_cursor.stroke = Stroke::new(1.5_f32, LIME);
    s.visuals.interact_cursor = Some(egui::CursorIcon::PointingHand);
    s.visuals.widgets.noninteractive.bg_fill = PANEL;
    s.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, LINE);
    s.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, MUTED);
    for w in [
        &mut s.visuals.widgets.inactive,
        &mut s.visuals.widgets.hovered,
        &mut s.visuals.widgets.active,
        &mut s.visuals.widgets.open,
    ] {
        w.corner_radius = egui::CornerRadius::same(8);
        w.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(65, 94, 109));
        w.fg_stroke = Stroke::new(1.0_f32, TEXT);
        w.expansion = 0.;
    }
    s.visuals.widgets.inactive.bg_fill = Color32::from_rgb(40, 64, 77);
    s.visuals.widgets.hovered.bg_fill = Color32::from_rgb(51, 80, 94);
    s.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, BLUE);
    s.visuals.widgets.active.bg_fill = Color32::from_rgb(32, 57, 67);
    s.visuals.widgets.active.bg_stroke = Stroke::new(1.5_f32, BLUE);
    s.visuals.widgets.open.bg_fill = Color32::from_rgb(35, 59, 71);
    s.visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, BLUE);
    s.visuals.widgets.inactive.weak_bg_fill = s.visuals.widgets.inactive.bg_fill;
    s.visuals.widgets.hovered.weak_bg_fill = s.visuals.widgets.hovered.bg_fill;
    s.visuals.widgets.active.weak_bg_fill = s.visuals.widgets.active.bg_fill;
    s.visuals.widgets.open.weak_bg_fill = s.visuals.widgets.open.bg_fill;
    s.visuals.window_corner_radius = egui::CornerRadius::same(12);
    s.visuals.menu_corner_radius = egui::CornerRadius::same(10);
    s.visuals.window_stroke = Stroke::new(1.0_f32, LINE);
    s.visuals.popup_shadow = egui::epaint::Shadow {
        offset: [0, 5],
        blur: 18,
        spread: 1,
        color: Color32::from_black_alpha(100),
    };
    s.spacing.item_spacing = Vec2::new(10., 9.);
    s.spacing.button_padding = Vec2::new(14., 8.);
    s.spacing.interact_size = Vec2::new(40., 34.);
    s.spacing.combo_width = 165.;
    s.spacing.scroll = egui::style::ScrollStyle::solid();
    s.spacing.scroll.bar_width = 10.;
    s.spacing.scroll.handle_min_length = 32.;
    s.animation_time = 0.07;
    for (kind, size) in [
        (egui::TextStyle::Body, 15.),
        (egui::TextStyle::Button, 14.),
        (egui::TextStyle::Small, 12.),
        (egui::TextStyle::Heading, 29.),
        (egui::TextStyle::Monospace, 13.),
    ] {
        s.text_styles.insert(kind, egui::FontId::proportional(size));
    }
    ctx.set_style(s);
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "Inter".into(),
        egui::FontData::from_static(include_bytes!("../assets/Inter.ttf")).into(),
    );
    fonts.font_data.insert(
        "Symbols".into(),
        egui::FontData::from_static(include_bytes!("../assets/Symbols.ttf")).into(),
    );
    let family = fonts
        .families
        .get_mut(&egui::FontFamily::Proportional)
        .unwrap();
    family.insert(0, "Inter".into());
    family.push("Symbols".into());
    ctx.set_fonts(fonts);
}
#[derive(Clone, Copy)]
pub enum NavIcon {
    Download,
    Search,
    Library,
    Settings,
}
/// Vector paths share one 24-unit grid and remain sharp at every display scale.
pub fn nav_button(ui: &mut egui::Ui, label: &str, icon: NavIcon, selected: bool) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 44.), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            label,
        )
    });
    let hover = response.hovered() || response.has_focus();
    let color = if selected {
        LIME
    } else if hover {
        TEXT
    } else {
        MUTED
    };
    if selected || hover {
        ui.painter().rect(
            rect,
            8.,
            if selected {
                PANEL
            } else {
                Color32::from_rgb(23, 42, 52)
            },
            Stroke::new(1.0_f32, LINE),
            egui::StrokeKind::Inside,
        );
    }
    let c = Pos2::new(rect.left() + 23., rect.center().y);
    let p = ui.painter();
    draw_icon(p, c, icon, color);
    p.text(
        Pos2::new(rect.left() + 44., rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(16.),
        color,
    );
    response
}
fn draw_icon(p: &egui::Painter, c: Pos2, icon: NavIcon, color: Color32) {
    let pt = |x: f32, y: f32| c + Vec2::new(x, y);
    let stroke = Stroke::new(1.7_f32, color);
    let path = |points: &[(f32, f32)]| {
        p.add(egui::Shape::line(
            points.iter().map(|&(x, y)| pt(x, y)).collect(),
            stroke,
        ))
    };
    match icon {
        NavIcon::Download => {
            path(&[(0., -9.), (0., 3.)]);
            path(&[(-4.5, -1.), (0., 3.5), (4.5, -1.)]);
            path(&[(-8., 4.), (-8., 8.), (8., 8.), (8., 4.)]);
        }
        NavIcon::Search => {
            p.circle_stroke(pt(-2., -2.), 6.3, stroke);
            path(&[(2.8, 2.8), (8.5, 8.5)]);
        }
        NavIcon::Library => {
            for x in [-8., -2.] {
                p.rect_stroke(
                    egui::Rect::from_min_max(pt(x, -8.), pt(x + 4., 8.)),
                    1.,
                    stroke,
                    egui::StrokeKind::Inside,
                );
                path(&[(x + 0.8, -3.), (x + 3.2, -3.)]);
            }
            path(&[(3., -6.), (7., -7.), (10., 7.), (6., 8.), (3., -6.)]);
        }
        NavIcon::Settings => {
            let mut points = Vec::with_capacity(49);
            for tooth in 0..8 {
                for (part, radius) in [(0., 7.), (0.18, 9.5), (0.62, 9.5), (0.8, 7.), (1., 7.)] {
                    let a = (tooth as f32 + part) * std::f32::consts::TAU / 8.;
                    points.push(pt(a.cos() * radius, a.sin() * radius));
                }
            }
            p.add(egui::Shape::closed_line(points, stroke));
            p.circle_stroke(c, 3.2, stroke);
        }
    }
}
pub fn background(ui: &egui::Ui) {
    let r = ui.max_rect();
    let p = ui.painter().with_clip_rect(r);
    p.rect_filled(r, 0., BG);
    let w = r.width();
    let h = r.height();
    p.add(egui::Shape::convex_polygon(
        vec![
            Pos2::new(r.left() + w * 0.47, r.bottom()),
            Pos2::new(r.left() + w * 0.94, r.top()),
            r.right_top(),
            Pos2::new(r.right(), r.top() + h * 0.55),
            Pos2::new(r.left() + w * 0.78, r.bottom()),
        ],
        Color32::from_rgb(24, 42, 51),
        Stroke::NONE,
    ));
    let mut x = r.left() + w * 0.60;
    while x < r.right() + h * 0.67 {
        p.line_segment(
            [Pos2::new(x, r.top()), Pos2::new(x - h * 0.67, r.bottom())],
            Stroke::new(0.7_f32, Color32::from_rgba_unmultiplied(150, 183, 172, 8)),
        );
        x += 36.;
    }
    logo(
        &p,
        Pos2::new(r.right() - 145., r.top() + 155.),
        210.,
        Color32::from_rgba_unmultiplied(170, 203, 173, 15),
    );
}
pub fn logo(p: &egui::Painter, c: Pos2, size: f32, color: Color32) {
    let s = size / 100.;
    let point = |x: f32, y: f32| Pos2::new(c.x + x * s, c.y + y * s);
    let stroke = Stroke::new(10. * s, color);
    p.line_segment([point(0., -48.), point(0., 14.)], stroke);
    p.line_segment([point(-28., -10.), point(0., 18.)], stroke);
    p.line_segment([point(28., -10.), point(0., 18.)], stroke);
    p.line_segment([point(-40., 18.), point(-40., 45.)], stroke);
    p.line_segment([point(40., 18.), point(40., 45.)], stroke);
    p.line_segment([point(-45., 40.), point(45., 40.)], stroke);
}
pub fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::from_rgba_unmultiplied(31, 51, 63, 246))
        .stroke(Stroke::new(1.0_f32, LINE))
        .inner_margin(18)
        .corner_radius(11)
}
pub fn checkbox(
    ui: &mut egui::Ui,
    checked: &mut bool,
    label: impl Into<egui::WidgetText>,
) -> egui::Response {
    ui.scope(|ui| {
        let selected = *checked;
        let v = ui.visuals_mut();
        for w in [
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            w.corner_radius = egui::CornerRadius::same(3);
            w.bg_fill = if selected {
                Color32::from_rgb(52, 79, 61)
            } else {
                BG
            };
            w.bg_stroke = Stroke::new(
                1.2_f32,
                if selected {
                    LIME
                } else {
                    Color32::from_rgb(82, 115, 126)
                },
            );
            w.fg_stroke = Stroke::new(1.7_f32, LIME);
        }
        ui.checkbox(checked, label)
    })
    .inner
}
pub fn primary(ui: &mut egui::Ui, label: &str) -> egui::Response {
    primary_sized(ui, label, Vec2::new(136., 38.))
}
pub fn primary_sized(ui: &mut egui::Ui, label: &str, size: Vec2) -> egui::Response {
    ui.scope(|ui| {
        let widgets = &mut ui.visuals_mut().widgets;
        for (widget, fill) in [
            (&mut widgets.inactive, LIME),
            (&mut widgets.hovered, Color32::from_rgb(213, 255, 141)),
            (&mut widgets.active, Color32::from_rgb(169, 222, 80)),
            (&mut widgets.open, LIME),
        ] {
            widget.bg_fill = fill;
            widget.weak_bg_fill = fill;
            widget.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(223, 255, 171));
            widget.fg_stroke = Stroke::new(1.0_f32, BG);
        }
        ui.add(
            egui::Button::new(RichText::new(label).color(BG).strong())
                .corner_radius(8)
                .min_size(size),
        )
    })
    .inner
}
pub fn text_field(text: &mut dyn egui::TextBuffer) -> egui::TextEdit<'_> {
    egui::TextEdit::singleline(text).margin(Vec2::new(11., 8.))
}
pub fn text_area(text: &mut dyn egui::TextBuffer) -> egui::TextEdit<'_> {
    egui::TextEdit::multiline(text).margin(Vec2::new(11., 10.))
}
/// A single path/value with visual wrapping and content-driven height, not horizontal scrolling.
pub fn wrapping_field(
    ui: &mut egui::Ui,
    text: &mut String,
    width: f32,
    hint: &str,
) -> egui::Response {
    let response = ui.add(
        egui::TextEdit::multiline(text)
            .margin(Vec2::new(11., 8.))
            .desired_rows(1)
            .desired_width((width - 22.).max(24.))
            .hint_text(hint),
    );
    if response.changed() {
        // Line breaks are visual; a pasted path still remains one logical value.
        text.retain(|c| c != '\r' && c != '\n');
    }
    response
}
pub fn title(ui: &mut egui::Ui, label: &str) {
    ui.label(RichText::new(label).size(18.).strong());
}
pub fn caption(ui: &mut egui::Ui, label: &str) {
    ui.label(RichText::new(label).size(12.).color(MUTED));
}

#[cfg(test)]
mod layout_tests {
    use super::*;
    #[test]
    fn long_paths_wrap_without_expanding_the_field_or_changing_the_value() {
        let value = format!(
            "C:\\Example\\{}cookies.txt",
            "long-folder-name\\".repeat(12)
        );
        let measure = |width: f32| {
            let ctx = egui::Context::default();
            apply(&ctx);
            let mut text = value.clone();
            let mut rect = egui::Rect::NOTHING;
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    rect = wrapping_field(ui, &mut text, width, "").rect;
                });
            });
            assert_eq!(text, value);
            assert!(rect.width() <= width + 1., "{rect:?}");
            rect.height()
        };
        let narrow = measure(280.);
        let wide = measure(560.);
        assert!(narrow > wide && wide > 34., "{narrow} {wide}");
    }
}
