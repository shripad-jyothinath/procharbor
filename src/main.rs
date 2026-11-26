mod charts;
mod desktop;
mod fmt;
mod icons;
mod sampler;
mod theme;
mod ui;

use std::env;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use eframe::egui;

use crate::desktop::APP_ID;
use crate::icons::{LOGO_SVG, export_png_icons, render_svg_bytes};
use crate::sampler::Sampler;
use crate::ui::ProcHarborApp;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn load_window_icon() -> Option<egui::IconData> {
    let img = render_svg_bytes(LOGO_SVG, 64)?;
    let rgba: Vec<u8> = img.as_raw().to_vec();
    Some(egui::IconData {
        rgba,
        width: img.width() as u32,
        height: img.height() as u32,
    })
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if let Some(pos) = args.iter().position(|a| a == "--export-icons") {
        let Some(dir) = args.get(pos + 1) else {
            eprintln!("Usage: procharbor --export-icons <ICON_ROOT>");
            return ExitCode::FAILURE;
        };
        if let Err(err) = export_png_icons(Path::new(dir)) {
            eprintln!("{}", err);
            return ExitCode::FAILURE;
        }
        return ExitCode::SUCCESS;
    }
    for arg in &args {
        if arg == "-v" || arg == "--version" {
            println!("ProcHarbor {}", VERSION);
            return ExitCode::SUCCESS;
        }
        if arg == "-h" || arg == "--help" {
            println!(
                "ProcHarbor {} — Native Linux task manager and system monitor",
                VERSION
            );
            println!("\nUsage: procharbor [OPTION]");
            println!("  -h, --help     Show help options");
            println!("  -v, --version  Show version");
            return ExitCode::SUCCESS;
        }
    }

    if env::var_os("DISPLAY").is_none() && env::var_os("WAYLAND_DISPLAY").is_none() {
        eprintln!("No graphical session detected (DISPLAY/WAYLAND_DISPLAY unset).");
        return ExitCode::FAILURE;
    }

    let sampler = Sampler::new();

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("ProcHarbor")
        .with_app_id(APP_ID)
        .with_inner_size([1200.0, 720.0])
        .with_min_inner_size([760.0, 480.0]);
    if let Some(icon) = load_window_icon() {
        viewport = viewport.with_icon(Arc::new(icon));
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    let res = eframe::run_native(
        "ProcHarbor",
        options,
        Box::new(move |cc| {
            let ctx = cc.egui_ctx.clone();
            sampler.start(Some(Arc::new(move || {
                ctx.request_repaint();
            })));
            Ok(Box::new(ProcHarborApp::new(cc, sampler)))
        }),
    );

    if let Err(err) = res {
        eprintln!("Failed to launch ProcHarbor: {}", err);
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
