#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod system_monitor;
mod theme;

use app::TaskManagerApp;
use eframe::egui;
use std::sync::Arc;

const APP_ICON_RGBA: &[u8] = include_bytes!("../assets/icon_64.rgba");

fn main() -> eframe::Result<()> {
    let icon_data = egui::IconData {
        rgba: APP_ICON_RGBA.to_vec(),
        width: 64,
        height: 64,
    };

    let viewport = egui::ViewportBuilder::default()
        .with_title("RigMap")
        .with_icon(Arc::new(icon_data))
        .with_inner_size([1080.0, 680.0])
        .with_min_inner_size([760.0, 480.0])
        .with_transparent(true);

    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "RigMap",
        native_options,
        Box::new(|cc| {
            // Try applying native Windows 11 Mica backdrop (with fallback to Acrylic)
            let mica_active = window_vibrancy::apply_mica(cc, Some(true))
                .or_else(|_| window_vibrancy::apply_acrylic(cc, Some((24, 24, 24, 200))))
                .is_ok();

            Ok(Box::new(TaskManagerApp::new(cc, mica_active)))
        }),
    )
}
