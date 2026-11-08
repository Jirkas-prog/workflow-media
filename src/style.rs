use crate::model::Theme;
use eframe::egui::{self, Color32, Pos2, RichText, Stroke, Vec2};

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color32,
    pub panel: Color32,
    pub sidebar: Color32,
    pub line: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub blue: Color32,
    pub green: Color32,
    pub red: Color32,
    pub field: Color32,
    pub stripe: Color32,
    pub header: Color32,
    pub selection: Color32,
    pub button: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    pub outline: Color32,
    pub primary: Color32,
    pub primary_hover: Color32,
    pub primary_pressed: Color32,
    pub primary_text: Color32,
    pub pattern: Color32,
    pub pattern_line: Color32,
    pub watermark: Color32,
}
const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}
pub const DARK: Palette = Palette {
    bg: rgb(20, 34, 43),
    panel: rgb(31, 51, 63),
    sidebar: rgb(15, 30, 39),
    line: rgb(55, 80, 94),
    text: rgb(220, 235, 242),
    muted: rgb(170, 198, 210),
    accent: rgb(196, 250, 103),
    blue: rgb(140, 207, 226),
    green: rgb(139, 211, 178),
    red: rgb(255, 173, 150),
    field: rgb(19, 37, 47),
    stripe: rgb(29, 50, 61),
    header: rgb(16, 33, 42),
    selection: rgb(55, 81, 67),
    button: rgb(40, 64, 77),
    hover: rgb(51, 80, 94),
    pressed: rgb(32, 57, 67),
    outline: rgb(65, 94, 109),
    primary: rgb(196, 250, 103),
    primary_hover: rgb(213, 255, 141),
    primary_pressed: rgb(169, 222, 80),
    primary_text: rgb(20, 34, 43),
    pattern: rgb(24, 42, 51),
    pattern_line: rgb(28, 46, 55),
    watermark: rgb(36, 54, 60),
};
pub const LIGHT: Palette = Palette {
    bg: rgb(238, 244, 244),
    panel: rgb(252, 254, 253),
    sidebar: rgb(228, 238, 235),
    line: rgb(185, 204, 201),
    text: rgb(25, 47, 53),
    muted: rgb(76, 103, 110),
    accent: rgb(45, 100, 28),
    blue: rgb(24, 100, 127),
    green: rgb(28, 112, 80),
    red: rgb(170, 57, 36),
    field: rgb(255, 255, 255),
    stripe: rgb(235, 243, 241),
    header: rgb(224, 236, 232),
    selection: rgb(211, 232, 199),
    button: rgb(228, 239, 235),
    hover: rgb(211, 229, 222),
    pressed: rgb(196, 217, 207),
    outline: rgb(145, 171, 165),
    primary: rgb(49, 110, 37),
    primary_hover: rgb(39, 95, 28),
    primary_pressed: rgb(30, 80, 22),
    primary_text: rgb(255, 255, 255),
    pattern: rgb(230, 239, 235),
    pattern_line: rgb(222, 233, 228),
    watermark: rgb(210, 226, 215),
};
pub fn palette(theme: Theme) -> &'static Palette {
    match theme {
        Theme::Dark => &DARK,
        Theme::Light => &LIGHT,
    }
}
fn colors(ui: &egui::Ui) -> &'static Palette {
    if ui.visuals().dark_mode {
        &DARK
    } else {
        &LIGHT
    }
}
fn egui_theme(theme: Theme) -> egui::Theme {
    match theme {
        Theme::Dark => egui::Theme::Dark,
        Theme::Light => egui::Theme::Light,
    }
}
/// Explicit preference also protects against later OS theme-change events.
pub fn select(ctx: &egui::Context, theme: Theme) {
    ctx.set_theme(egui_theme(theme));
    ctx.send_viewport_cmd(egui::ViewportCommand::SetTheme(match theme {
        Theme::Dark => egui::SystemTheme::Dark,
        Theme::Light => egui::SystemTheme::Light,
    }));
    ctx.request_repaint();
}
pub fn sync(ctx: &egui::Context, theme: Theme) {
    if ctx.options(|o| o.theme_preference) != egui::ThemePreference::from(egui_theme(theme)) {
        select(ctx, theme);
    }
}
fn theme_style(theme: Theme) -> egui::Style {
    let c = palette(theme);
    let mut s = egui::Style::default();
    s.visuals = match theme {
        Theme::Dark => egui::Visuals::dark(),
        Theme::Light => egui::Visuals::light(),
    };
    s.visuals.override_text_color = Some(c.text);
    s.visuals.weak_text_color = Some(c.muted);
    s.visuals.hyperlink_color = c.blue;
    s.visuals.warn_fg_color = c.red;
    s.visuals.error_fg_color = c.red;
    s.visuals.panel_fill = c.bg;
    s.visuals.window_fill = c.panel;
    s.visuals.extreme_bg_color = c.header;
    s.visuals.text_edit_bg_color = Some(c.field);
    s.visuals.faint_bg_color = c.stripe;
    s.visuals.code_bg_color = c.field;
    s.visuals.selection.bg_fill = c.selection;
    s.visuals.selection.stroke = Stroke::new(1.5_f32, c.accent);
    s.visuals.text_cursor.stroke = Stroke::new(1.5_f32, c.accent);
    s.visuals.interact_cursor = Some(egui::CursorIcon::PointingHand);
    s.visuals.widgets.noninteractive.bg_fill = c.panel;
    s.visuals.widgets.noninteractive.weak_bg_fill = c.panel;
    s.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, c.line);
    s.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, c.muted);
    for w in [
        &mut s.visuals.widgets.inactive,
        &mut s.visuals.widgets.hovered,
        &mut s.visuals.widgets.active,
        &mut s.visuals.widgets.open,
    ] {
        w.corner_radius = egui::CornerRadius::same(8);
        w.bg_stroke = Stroke::new(1.0_f32, c.outline);
        w.fg_stroke = Stroke::new(1.0_f32, c.text);
        w.expansion = 0.;
    }
    s.visuals.widgets.inactive.bg_fill = c.button;
    s.visuals.widgets.hovered.bg_fill = c.hover;
    s.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, c.blue);
    s.visuals.widgets.active.bg_fill = c.pressed;
    s.visuals.widgets.active.bg_stroke = Stroke::new(1.5_f32, c.blue);
    s.visuals.widgets.open.bg_fill = c.hover;
    s.visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, c.blue);
    s.visuals.widgets.inactive.weak_bg_fill = s.visuals.widgets.inactive.bg_fill;
    s.visuals.widgets.hovered.weak_bg_fill = s.visuals.widgets.hovered.bg_fill;
    s.visuals.widgets.active.weak_bg_fill = s.visuals.widgets.active.bg_fill;
    s.visuals.widgets.open.weak_bg_fill = s.visuals.widgets.open.bg_fill;
    s.visuals.window_corner_radius = egui::CornerRadius::same(12);
    s.visuals.menu_corner_radius = egui::CornerRadius::same(10);
    s.visuals.window_stroke = Stroke::new(1.0_f32, c.line);
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
    s
}
pub fn apply(ctx: &egui::Context, theme: Theme) {
    // Install BOTH complete styles rather than overwriting only the style currently selected by the OS.
    ctx.set_style_of(egui::Theme::Dark, theme_style(Theme::Dark));
    ctx.set_style_of(egui::Theme::Light, theme_style(Theme::Light));
    select(ctx, theme);
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
    let c = colors(ui);
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
        c.accent
    } else if hover {
        c.text
    } else {
        c.muted
    };
    if selected || hover {
        ui.painter().rect(
            rect,
            8.,
            if selected { c.selection } else { c.hover },
            Stroke::new(1.0_f32, c.line),
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
    let c = colors(ui);
    let r = ui.max_rect();
    let p = ui.painter().with_clip_rect(r);
    p.rect_filled(r, 0., c.bg);
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
        c.pattern,
        Stroke::NONE,
    ));
    let mut x = r.left() + w * 0.60;
    while x < r.right() + h * 0.67 {
        p.line_segment(
            [Pos2::new(x, r.top()), Pos2::new(x - h * 0.67, r.bottom())],
            Stroke::new(0.7_f32, c.pattern_line),
        );
        x += 36.;
    }
    logo(
        &p,
        Pos2::new(r.right() - 145., r.top() + 155.),
        210.,
        c.watermark,
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
pub fn card(ui: &egui::Ui) -> egui::Frame {
    let c = colors(ui);
    egui::Frame::new()
        .fill(c.panel)
        .stroke(Stroke::new(1.0_f32, c.line))
        .inner_margin(18)
        .corner_radius(11)
}
pub fn checkbox(
    ui: &mut egui::Ui,
    checked: &mut bool,
    label: impl Into<egui::WidgetText>,
) -> egui::Response {
    let c = colors(ui);
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
            w.bg_fill = if selected { c.selection } else { c.field };
            w.bg_stroke = Stroke::new(1.2_f32, if selected { c.accent } else { c.outline });
            w.fg_stroke = Stroke::new(1.7_f32, c.accent);
        }
        ui.checkbox(checked, label)
    })
    .inner
}
pub fn primary(ui: &mut egui::Ui, label: &str) -> egui::Response {
    primary_sized(ui, label, Vec2::new(136., 38.))
}
pub fn primary_sized(ui: &mut egui::Ui, label: &str, size: Vec2) -> egui::Response {
    let c = colors(ui);
    ui.scope(|ui| {
        let widgets = &mut ui.visuals_mut().widgets;
        for (widget, fill) in [
            (&mut widgets.inactive, c.primary),
            (&mut widgets.hovered, c.primary_hover),
            (&mut widgets.active, c.primary_pressed),
            (&mut widgets.open, c.primary),
        ] {
            widget.bg_fill = fill;
            widget.weak_bg_fill = fill;
            widget.bg_stroke = Stroke::new(1.0_f32, c.primary);
            widget.fg_stroke = Stroke::new(1.0_f32, c.primary_text);
        }
        ui.add(
            egui::Button::new(RichText::new(label).color(c.primary_text).strong())
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
    ui.label(RichText::new(label).size(12.).color(colors(ui).muted));
}

#[cfg(test)]
mod layout_tests {
    use super::*;
    #[test]
    fn application_styles_ignore_initial_and_changing_system_themes() {
        for initial in [None, Some(egui::Theme::Light), Some(egui::Theme::Dark)] {
            let ctx = egui::Context::default();
            let _ = ctx.run(
                egui::RawInput {
                    system_theme: initial,
                    ..Default::default()
                },
                |_| {},
            );
            apply(&ctx, Theme::Dark);
            for chosen in [Theme::Dark, Theme::Light, Theme::Dark] {
                select(&ctx, chosen);
                let expected = theme_style(chosen);
                for system in [Some(egui::Theme::Light), Some(egui::Theme::Dark), None] {
                    let _ = ctx.run(
                        egui::RawInput {
                            system_theme: system,
                            ..Default::default()
                        },
                        |ctx| {
                            assert_eq!(ctx.theme(), egui_theme(chosen));
                            assert_ne!(
                                ctx.options(|o| o.theme_preference),
                                egui::ThemePreference::System
                            );
                            assert_eq!(ctx.style().visuals, expected.visuals);
                            assert_eq!(
                                ctx.style().spacing.button_padding,
                                expected.spacing.button_padding
                            );
                            assert_eq!(ctx.style().text_styles, expected.text_styles);
                            egui::CentralPanel::default().show(ctx, |ui| {
                                assert_eq!(
                                    ui.visuals().override_text_color,
                                    Some(palette(chosen).text)
                                );
                                assert_eq!(
                                    ui.visuals().text_edit_bg_color,
                                    Some(palette(chosen).field)
                                );
                            });
                        },
                    );
                }
            }
        }
    }

    // Relative luminance: https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html
    fn contrast(a: Color32, b: Color32) -> f64 {
        let luminance = |c: Color32| {
            let linear = |n: u8| {
                let n = n as f64 / 255.;
                if n <= 0.04045 {
                    n / 12.92
                } else {
                    ((n + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * linear(c.r()) + 0.7152 * linear(c.g()) + 0.0722 * linear(c.b())
        };
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn both_palettes_keep_readable_text_fields_buttons_and_selected_rows() {
        for theme in [Theme::Dark, Theme::Light] {
            let c = palette(theme);
            for bg in [
                c.bg,
                c.panel,
                c.sidebar,
                c.field,
                c.stripe,
                c.header,
                c.selection,
                c.button,
                c.hover,
            ] {
                for fg in [c.text, c.muted, c.accent, c.blue, c.green, c.red] {
                    assert!(
                        contrast(fg, bg) >= 4.5,
                        "{theme:?}: {fg:?} on {bg:?} = {}",
                        contrast(fg, bg)
                    );
                }
            }
            for bg in [c.primary, c.primary_hover, c.primary_pressed] {
                assert!(contrast(c.primary_text, bg) >= 4.5);
            }
        }
    }

    #[test]
    fn long_paths_wrap_without_expanding_the_field_or_changing_the_value() {
        let value = format!(
            "C:\\Example\\{}cookies.txt",
            "long-folder-name\\".repeat(12)
        );
        let measure = |width: f32| {
            let ctx = egui::Context::default();
            apply(&ctx, Theme::Dark);
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
