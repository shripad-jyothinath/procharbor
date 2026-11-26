use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};

use crate::desktop::{APP_ID, home_dir};

pub const LOGO_SVG: &[u8] = include_bytes!("../assets/logo.svg");

pub struct IconCache {
    paths: HashMap<String, (u8, PathBuf)>,
    textures: HashMap<String, Option<TextureHandle>>,
    logo_tex: Option<TextureHandle>,
}

impl IconCache {
    pub fn new() -> Self {
        let mut paths = HashMap::new();
        let active_theme = detect_icon_theme();
        let home = home_dir();

        let mut theme_names = Vec::new();
        if let Some(t) = active_theme {
            theme_names.push(t);
        }
        for fallback in ["hicolor", "Adwaita", "Yaru", "Zorin", "ZorinBlue-Dark"] {
            if !theme_names.iter().any(|x| x == fallback) {
                theme_names.push(fallback.to_string());
            }
        }

        let icon_roots = [
            format!("{}/.local/share/icons", home),
            "/usr/local/share/icons".to_string(),
            "/usr/share/icons".to_string(),
            "/var/lib/flatpak/exports/share/icons".to_string(),
        ];

        // Priority order: lower score wins (theme match / ideal size).
        for (theme_idx, theme) in theme_names.iter().enumerate() {
            let base_prio = (theme_idx as u8).saturating_mul(10);
            for root in &icon_roots {
                let theme_dir = Path::new(root).join(theme);
                if !theme_dir.is_dir() {
                    continue;
                }
                scan_theme_dir(&theme_dir, base_prio, &mut paths);
            }
        }

        // Scan /usr/share/pixmaps as lowest priority fallback
        if let Ok(entries) = fs::read_dir("/usr/share/pixmaps") {
            for ent in entries.flatten() {
                let p = ent.path();
                if let Some((stem, prio_ext)) = icon_stem_and_ext(&p) {
                    insert_if_better(&mut paths, stem, 200 + prio_ext, p);
                }
            }
        }

        Self {
            paths,
            textures: HashMap::new(),
            logo_tex: None,
        }
    }

    pub fn has_icon(&self, name: &str) -> bool {
        if name == APP_ID {
            return true;
        }
        if Path::new(name).is_file() {
            return true;
        }
        let stem = strip_icon_ext(name);
        self.paths.contains_key(stem)
    }

    pub fn logo(&mut self, ctx: &egui::Context) -> Option<&TextureHandle> {
        if self.logo_tex.is_none() {
            if let Some(img) = render_svg_bytes(LOGO_SVG, 48) {
                self.logo_tex =
                    Some(ctx.load_texture("procharbor-logo", img, TextureOptions::LINEAR));
            }
        }
        self.logo_tex.as_ref()
    }

    pub fn get_texture(&mut self, ctx: &egui::Context, name: &str) -> Option<TextureHandle> {
        if name.is_empty() {
            return None;
        }
        if let Some(cached) = self.textures.get(name) {
            return cached.clone();
        }

        let img = if name == APP_ID {
            render_svg_bytes(LOGO_SVG, 36)
        } else {
            let path = if Path::new(name).is_file() {
                Some(PathBuf::from(name))
            } else {
                self.paths.get(strip_icon_ext(name)).map(|(_, p)| p.clone())
            };
            path.and_then(|p| load_icon_file(&p, 36))
                .or_else(|| fallback_icon_image(name))
        };

        let handle =
            img.map(|ci| ctx.load_texture(format!("icon:{}", name), ci, TextureOptions::LINEAR));
        self.textures.insert(name.to_string(), handle.clone());
        handle
    }
}

pub fn render_svg_bytes(bytes: &[u8], target_px: u32) -> Option<ColorImage> {
    let pixmap = render_svg_pixmap(bytes, target_px)?;
    let w = pixmap.width() as usize;
    let h = pixmap.height() as usize;
    // tiny_skia produces premultiplied RGBA
    Some(ColorImage::from_rgba_premultiplied([w, h], pixmap.data()))
}

fn render_svg_pixmap(bytes: &[u8], target_px: u32) -> Option<resvg::tiny_skia::Pixmap> {
    let opt = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(bytes, &opt).ok()?;
    let sz = tree.size();
    let w = sz.width();
    let h = sz.height();
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let scale = (target_px as f32 / w).min(target_px as f32 / h);
    let out_w = (w * scale).round().max(1.0) as u32;
    let out_h = (h * scale).round().max(1.0) as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(out_w, out_h)?;
    let transform = resvg::tiny_skia::Transform::from_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Some(pixmap)
}

pub fn export_png_icons(icon_root: &Path) -> Result<(), String> {
    for size in [32u32, 48, 64, 128, 256, 512] {
        let dest_dir = icon_root.join(format!("{}x{}", size, size)).join("apps");
        fs::create_dir_all(&dest_dir)
            .map_err(|e| format!("Failed to create {}: {}", dest_dir.display(), e))?;
        let pixmap = render_svg_pixmap(LOGO_SVG, size)
            .ok_or_else(|| "Failed to render logo.svg".to_string())?;
        let mut rgba = Vec::with_capacity(pixmap.data().len());
        for px in pixmap.pixels() {
            let demul = px.demultiply();
            rgba.extend_from_slice(&[demul.red(), demul.green(), demul.blue(), demul.alpha()]);
        }
        let dest = dest_dir.join(format!("{}.png", APP_ID));
        image::save_buffer_with_format(
            &dest,
            &rgba,
            pixmap.width(),
            pixmap.height(),
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        )
        .map_err(|e| format!("Failed to save {}: {}", dest.display(), e))?;
    }
    Ok(())
}

fn load_icon_file(path: &Path, target_px: u32) -> Option<ColorImage> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())?;
    let bytes = fs::read(path).ok()?;
    if ext == "svg" || ext == "svgz" {
        render_svg_bytes(&bytes, target_px)
    } else if ext == "png" {
        let dyn_img = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).ok()?;
        let resized = if dyn_img.width() > target_px * 2 || dyn_img.height() > target_px * 2 {
            dyn_img.resize(target_px, target_px, image::imageops::FilterType::Triangle)
        } else {
            dyn_img
        };
        let rgba = resized.to_rgba8();
        let size = [rgba.width() as usize, rgba.height() as usize];
        Some(ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()))
    } else {
        None
    }
}

fn detect_icon_theme() -> Option<String> {
    if let Ok(out) = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "icon-theme"])
        .output()
    {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout)
                .trim()
                .trim_matches('\'')
                .trim_matches('"')
                .to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
    }
    let home = home_dir();
    for cfg in [
        format!("{}/.config/gtk-4.0/settings.ini", home),
        format!("{}/.config/gtk-3.0/settings.ini", home),
    ] {
        if let Ok(text) = fs::read_to_string(cfg) {
            for line in text.lines() {
                if let Some((k, v)) = line.split_once('=') {
                    if k.trim() == "gtk-icon-theme-name" {
                        let val = v.trim().to_string();
                        if !val.is_empty() {
                            return Some(val);
                        }
                    }
                }
            }
        }
    }
    None
}

fn strip_icon_ext(name: &str) -> &str {
    name.strip_suffix(".png")
        .or_else(|| name.strip_suffix(".svg"))
        .or_else(|| name.strip_suffix(".svgz"))
        .unwrap_or(name)
}

fn icon_stem_and_ext(path: &Path) -> Option<(String, u8)> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    let prio = match ext.as_str() {
        "svg" | "svgz" => 0,
        "png" => 1,
        _ => return None,
    };
    let stem = path.file_stem()?.to_str()?.to_string();
    Some((stem, prio))
}

fn insert_if_better(
    map: &mut HashMap<String, (u8, PathBuf)>,
    key: String,
    prio: u8,
    path: PathBuf,
) {
    match map.get_mut(&key) {
        Some((cur_prio, cur_path)) => {
            if prio < *cur_prio {
                *cur_prio = prio;
                *cur_path = path;
            }
        }
        None => {
            map.insert(key, (prio, path));
        }
    }
}

fn scan_theme_dir(theme_dir: &Path, base_prio: u8, paths: &mut HashMap<String, (u8, PathBuf)>) {
    // Prioritized subdirs matching common Linux icon layouts
    let subdirs = [
        ("scalable/apps", 0u8),
        ("48x48/apps", 1),
        ("32x32/apps", 2),
        ("64x64/apps", 2),
        ("24x24/apps", 3),
        ("22x22/apps", 3),
        ("16x16/apps", 4),
        ("128x128/apps", 4),
        ("256x256/apps", 5),
        ("scalable/status", 6),
        ("scalable/categories", 6),
        ("scalable/mimetypes", 7),
        ("32x32/categories", 7),
        ("24x24/categories", 7),
        ("16x16/categories", 8),
        ("24x24/mimetypes", 8),
        ("16x16/mimetypes", 9),
    ];

    for (sub, sub_prio) in subdirs {
        let dir = theme_dir.join(sub);
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for ent in entries.flatten() {
            let p = ent.path();
            if let Some((stem, ext_prio)) = icon_stem_and_ext(&p) {
                insert_if_better(
                    paths,
                    stem,
                    base_prio.saturating_add(sub_prio) + ext_prio,
                    p,
                );
            }
        }
    }
}

fn fallback_icon_image(name: &str) -> Option<ColorImage> {
    // Crisp 18x18 programmatic fallback badge if a symbolic/app icon wasn't found in themes
    let (r, g, b) = if name.contains("terminal") {
        (0x5a, 0xd4, 0x8a)
    } else if name.contains("system") {
        (0x4c, 0xc2, 0xff)
    } else {
        (0xa2, 0xa2, 0xa2)
    };
    let size = 18usize;
    let mut pixels = vec![0u8; size * size * 4];
    for y in 2..size - 2 {
        for x in 2..size - 2 {
            let is_border = x == 2 || x == size - 3 || y == 2 || y == size - 3;
            let idx = (y * size + x) * 4;
            if is_border {
                pixels[idx] = r;
                pixels[idx + 1] = g;
                pixels[idx + 2] = b;
                pixels[idx + 3] = 220;
            } else {
                pixels[idx] = r;
                pixels[idx + 1] = g;
                pixels[idx + 2] = b;
                pixels[idx + 3] = 45;
            }
        }
    }
    Some(ColorImage::from_rgba_unmultiplied([size, size], &pixels))
}
