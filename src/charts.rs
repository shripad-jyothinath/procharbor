use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Mesh, Pos2, Rect, Shape, Stroke, Vec2,
};

use crate::fmt::nice_max;
use crate::sampler::HISTORY_LEN;
use crate::theme::Palette;

pub struct ChartSeries<'a> {
    pub data: &'a [f64],
    pub color: Color32,
    pub fill: bool,
}

pub fn draw_sparkline(ui: &mut egui::Ui, data: &[f64], color: Color32, height: f32) {
    let desired = Vec2::new(ui.available_width().max(40.0), height);
    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
    if !ui.is_rect_visible(rect) || data.len() < 2 {
        return;
    }

    let mx = data.iter().copied().fold(0.0_f64, f64::max).max(1.0);
    let slots = HISTORY_LEN.max(data.len());
    let step = rect.width() / (slots - 1) as f32;
    let h = rect.height();

    let mut pts = Vec::with_capacity(data.len());
    for (i, &v) in data.iter().enumerate() {
        let from_right = (data.len() - 1 - i) as f32;
        let x = rect.right() - from_right * step;
        let frac = (v / mx).clamp(0.0, 1.0) as f32;
        let y = rect.bottom() - 1.0 - frac * (h - 2.0).max(1.0);
        pts.push(Pos2::new(x, y));
    }

    ui.painter().add(Shape::line(pts, Stroke::new(1.4, color)));
}

pub fn draw_chart_area<F>(
    ui: &mut egui::Ui,
    pal: &Palette,
    series: &[ChartSeries<'_>],
    maxv_opt: Option<f64>,
    fmt: F,
    height: f32,
) where
    F: Fn(f64) -> String,
{
    let desired = Vec2::new(ui.available_width().max(120.0), height);
    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
    if !ui.is_rect_visible(rect) || series.is_empty() {
        return;
    }

    let pad_l = 4.0;
    let pad_r = 68.0;
    let pad_t = 8.0;
    let pad_b = 16.0;

    let pw = (rect.width() - pad_l - pad_r).max(10.0);
    let ph = (rect.height() - pad_t - pad_b).max(10.0);
    let plot = Rect::from_min_size(
        Pos2::new(rect.left() + pad_l, rect.top() + pad_t),
        Vec2::new(pw, ph),
    );

    let maxv = match maxv_opt {
        Some(v) if v > 0.0 => v,
        _ => {
            let raw_max = series
                .iter()
                .flat_map(|s| s.data.iter().copied())
                .fold(0.0_f64, f64::max);
            nice_max(raw_max)
        }
    };

    let painter = ui.painter_at(rect);
    let font = FontId::proportional(10.5);

    for i in 0..5 {
        let y = plot.top() + (ph / 4.0) * i as f32;
        painter.line_segment(
            [Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)],
            Stroke::new(1.0, pal.chart_grid),
        );
        let label = fmt(maxv * (1.0 - i as f64 / 4.0));
        painter.text(
            Pos2::new(plot.right() + 7.0, y),
            Align2::LEFT_CENTER,
            label,
            font.clone(),
            pal.chart_text,
        );
    }

    painter.text(
        Pos2::new(plot.left(), rect.bottom() - 2.0),
        Align2::LEFT_BOTTOM,
        "-2m",
        font.clone(),
        pal.chart_text,
    );
    painter.text(
        Pos2::new(plot.right(), rect.bottom() - 2.0),
        Align2::RIGHT_BOTTOM,
        "now",
        font,
        pal.chart_text,
    );

    for s in series {
        if s.data.is_empty() {
            continue;
        }
        let n = s.data.len();
        let slots = HISTORY_LEN.max(n);
        let step = pw / (slots - 1) as f32;
        let mut pts = Vec::with_capacity(n);
        for (i, &v) in s.data.iter().enumerate() {
            let from_right = (n - 1 - i) as f32;
            let x = plot.right() - from_right * step;
            let frac = (v / maxv).clamp(0.0, 1.0) as f32;
            let y = plot.bottom() - frac * ph;
            pts.push(Pos2::new(x, y));
        }

        if s.fill && pts.len() >= 2 {
            let [r, g, b, _] = s.color.to_array();
            let top_col = Color32::from_rgba_unmultiplied(r, g, b, 76);
            let bot_col = Color32::from_rgba_unmultiplied(r, g, b, 6);
            let mut mesh = Mesh::default();
            for i in 0..pts.len() - 1 {
                let p0 = pts[i];
                let p1 = pts[i + 1];
                let b0 = Pos2::new(p0.x, plot.bottom());
                let b1 = Pos2::new(p1.x, plot.bottom());
                let idx = mesh.vertices.len() as u32;
                mesh.colored_vertex(p0, top_col);
                mesh.colored_vertex(p1, top_col);
                mesh.colored_vertex(b1, bot_col);
                mesh.colored_vertex(b0, bot_col);
                mesh.add_triangle(idx, idx + 1, idx + 2);
                mesh.add_triangle(idx, idx + 2, idx + 3);
            }
            painter.add(Shape::mesh(mesh));
        }

        if pts.len() >= 2 {
            painter.add(Shape::line(pts, Stroke::new(1.7, s.color)));
        } else if let Some(&pt) = pts.first() {
            painter.circle_filled(pt, 2.5, s.color);
        }
    }
}

pub fn draw_composition_bar(ui: &mut egui::Ui, pal: &Palette, used: f64, cached: f64, free: f64) {
    let desired = Vec2::new(ui.available_width().max(40.0), 10.0);
    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }

    let total = (used + cached + free.max(0.0)).max(1.0);
    let free_val = (total - used - cached).max(0.0);
    let painter = ui.painter();

    let mut x = rect.left();
    for (val, color) in [
        (used, pal.accent),
        (cached, pal.purple),
        (free_val, pal.border),
    ] {
        let seg_w = (rect.width() * (val / total) as f32).max(0.0);
        if seg_w > 0.0 {
            let seg_rect =
                Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new(seg_w, rect.height()));
            painter.rect_filled(seg_rect, CornerRadius::same(2), color);
            x += seg_w;
        }
    }
}

pub fn draw_thin_bar(ui: &mut egui::Ui, pal: &Palette, frac: f32, color: Color32, width: f32) {
    let desired = Vec2::new(width, 7.0);
    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(3), pal.panel2);
    let fill_w = (rect.width() * frac.clamp(0.0, 1.0)).max(0.0);
    if fill_w > 0.0 {
        let fill_rect = Rect::from_min_size(rect.min, Vec2::new(fill_w, rect.height()));
        painter.rect_filled(fill_rect, CornerRadius::same(3), color);
    }
}
