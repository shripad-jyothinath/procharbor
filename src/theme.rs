use eframe::egui::{self, Color32, CornerRadius, Stroke, Style, Visuals};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ThemeMode {
    Dark,
    Light,
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color32,
    pub side: Color32,
    pub panel: Color32,
    pub panel2: Color32,
    pub border: Color32,
    pub border_soft: Color32,
    pub text: Color32,
    pub dim: Color32,
    pub faint: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub danger: Color32,
    pub green: Color32,
    pub amber: Color32,
    pub orange: Color32,
    pub purple: Color32,
    pub sel: Color32,
    pub hover: Color32,
    pub chart_grid: Color32,
    pub chart_text: Color32,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

pub const DARK_PALETTE: Palette = Palette {
    bg: rgb(0x1f, 0x1f, 0x1f),
    side: rgb(0x1a, 0x1a, 0x1a),
    panel: rgb(0x27, 0x27, 0x27),
    panel2: rgb(0x2d, 0x2d, 0x2d),
    border: rgb(0x3a, 0x3a, 0x3a),
    border_soft: rgb(0x31, 0x31, 0x31),
    text: rgb(0xec, 0xec, 0xec),
    dim: rgb(0xa2, 0xa2, 0xa2),
    faint: rgb(0x78, 0x78, 0x78),
    accent: rgb(0x4c, 0xc2, 0xff),
    accent_soft: Color32::from_rgba_premultiplied(10, 25, 33, 33),
    danger: rgb(0xff, 0x76, 0x76),
    green: rgb(0x5a, 0xd4, 0x8a),
    amber: rgb(0xf7, 0xb9, 0x55),
    orange: rgb(0xff, 0x9f, 0x43),
    purple: rgb(0xa6, 0x8b, 0xff),
    sel: rgb(0x12, 0x3a, 0x52),
    hover: rgb(0x2b, 0x2b, 0x2b),
    chart_grid: rgb(0x3a, 0x3a, 0x3a),
    chart_text: rgb(0x78, 0x78, 0x78),
};

pub const LIGHT_PALETTE: Palette = Palette {
    bg: rgb(0xf4, 0xf4, 0xf4),
    side: rgb(0xea, 0xea, 0xea),
    panel: rgb(0xff, 0xff, 0xff),
    panel2: rgb(0xf7, 0xf7, 0xf7),
    border: rgb(0xd2, 0xd2, 0xd2),
    border_soft: rgb(0xe3, 0xe3, 0xe3),
    text: rgb(0x1b, 0x1b, 0x1b),
    dim: rgb(0x5d, 0x5d, 0x5d),
    faint: rgb(0x8a, 0x8a, 0x8a),
    accent: rgb(0x0f, 0x7a, 0xc2),
    accent_soft: Color32::from_rgba_premultiplied(2, 15, 24, 31),
    danger: rgb(0xc2, 0x2f, 0x2f),
    green: rgb(0x1a, 0x8a, 0x4a),
    amber: rgb(0xa8, 0x6a, 0x00),
    orange: rgb(0xb4, 0x53, 0x09),
    purple: rgb(0x6d, 0x4f, 0xd6),
    sel: rgb(0xcf, 0xe8, 0xfb),
    hover: rgb(0xe8, 0xe8, 0xe8),
    chart_grid: rgb(0xd2, 0xd2, 0xd2),
    chart_text: rgb(0x8a, 0x8a, 0x8a),
};

impl ThemeMode {
    pub fn palette(self) -> &'static Palette {
        match self {
            ThemeMode::Dark => &DARK_PALETTE,
            ThemeMode::Light => &LIGHT_PALETTE,
        }
    }
}

pub fn load_color(pal: &Palette, value: f64, is_cpu: bool) -> Color32 {
    if is_cpu {
        if value >= 50.0 {
            return pal.danger;
        }
        if value >= 25.0 {
            return pal.orange;
        }
        if value >= 5.0 {
            return pal.amber;
        }
    } else {
        if value >= 15.0 {
            return pal.danger;
        }
        if value >= 8.0 {
            return pal.orange;
        }
        if value >= 2.0 {
            return pal.amber;
        }
    }
    pal.text
}

pub fn apply_egui_style(ctx: &egui::Context, mode: ThemeMode) {
    let pal = mode.palette();
    let mut visuals = match mode {
        ThemeMode::Dark => Visuals::dark(),
        ThemeMode::Light => Visuals::light(),
    };

    visuals.panel_fill = pal.bg;
    visuals.window_fill = pal.panel;
    visuals.extreme_bg_color = pal.panel;
    visuals.faint_bg_color = pal.panel2;
    visuals.override_text_color = Some(pal.text);
    visuals.selection.bg_fill = pal.sel;
    visuals.selection.stroke = Stroke::new(1.0, pal.accent);

    visuals.widgets.noninteractive.bg_fill = pal.panel;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, pal.border_soft);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, pal.text);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(6);

    visuals.widgets.inactive.bg_fill = pal.panel;
    visuals.widgets.inactive.weak_bg_fill = pal.panel;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, pal.border);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, pal.text);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(6);

    visuals.widgets.hovered.bg_fill = pal.panel2;
    visuals.widgets.hovered.weak_bg_fill = pal.hover;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, pal.border);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, pal.text);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(6);

    visuals.widgets.active.bg_fill = pal.sel;
    visuals.widgets.active.weak_bg_fill = pal.sel;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, pal.accent);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, pal.text);
    visuals.widgets.active.corner_radius = CornerRadius::same(6);

    visuals.window_stroke = Stroke::new(1.0, pal.border);
    visuals.window_corner_radius = CornerRadius::same(8);

    let mut style = Style {
        visuals,
        ..Default::default()
    };
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.spacing.scroll.bar_width = 8.0;
    let egui_theme = match mode {
        ThemeMode::Dark => egui::Theme::Dark,
        ThemeMode::Light => egui::Theme::Light,
    };
    ctx.set_theme(egui_theme);
    ctx.set_style_of(egui_theme, style);
}
