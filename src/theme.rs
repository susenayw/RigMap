use egui::{Color32, FontData, FontDefinitions, FontFamily, Margin, Rounding, Stroke, Style, Visuals};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ThemeMode {
    Dark,
    Light,
}

#[derive(Clone, Copy)]
pub struct WinUiPalette {
    pub bg_base: Color32,
    pub bg_sidebar: Color32,
    pub bg_card: Color32,
    pub bg_card_hover: Color32,
    pub bg_card_selected: Color32,
    pub bg_subtle: Color32,
    pub border_card: Color32,
    pub border_subtle: Color32,
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub text_muted: Color32,
    pub accent: Color32,
    pub accent_text_on_fill: Color32,
    pub accent_subtle_bg: Color32,
    pub danger: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub chart_cpu: Color32,
    pub chart_mem: Color32,
    pub chart_disk: Color32,
    pub chart_net: Color32,
}

impl WinUiPalette {
    pub fn for_mode(mode: ThemeMode, mica_active: bool) -> Self {
        match mode {
            ThemeMode::Dark => Self {
                bg_base: if mica_active {
                    Color32::from_rgba_unmultiplied(28, 28, 28, 215)
                } else {
                    Color32::from_rgb(32, 32, 32)
                },
                bg_sidebar: if mica_active {
                    Color32::from_rgba_unmultiplied(22, 22, 22, 195)
                } else {
                    Color32::from_rgb(25, 25, 25)
                },
                bg_card: Color32::from_rgba_unmultiplied(43, 43, 43, 240),
                bg_card_hover: Color32::from_rgb(54, 54, 54),
                bg_card_selected: Color32::from_rgb(62, 62, 62),
                bg_subtle: Color32::from_rgb(36, 36, 36),
                border_card: Color32::from_rgba_unmultiplied(255, 255, 255, 20),
                border_subtle: Color32::from_rgba_unmultiplied(255, 255, 255, 12),
                text_primary: Color32::from_rgb(255, 255, 255),
                text_secondary: Color32::from_rgb(200, 200, 200),
                text_muted: Color32::from_rgb(145, 145, 145),
                accent: Color32::from_rgb(96, 205, 255),
                accent_text_on_fill: Color32::from_rgb(0, 26, 46),
                accent_subtle_bg: Color32::from_rgba_unmultiplied(96, 205, 255, 36),
                danger: Color32::from_rgb(255, 153, 164),
                success: Color32::from_rgb(108, 203, 95),
                warning: Color32::from_rgb(252, 225, 0),
                chart_cpu: Color32::from_rgb(96, 205, 255),
                chart_mem: Color32::from_rgb(180, 128, 255),
                chart_disk: Color32::from_rgb(108, 203, 95),
                chart_net: Color32::from_rgb(247, 168, 75),
            },
            ThemeMode::Light => Self {
                bg_base: if mica_active {
                    Color32::from_rgba_unmultiplied(243, 243, 243, 225)
                } else {
                    Color32::from_rgb(243, 243, 243)
                },
                bg_sidebar: if mica_active {
                    Color32::from_rgba_unmultiplied(235, 235, 235, 205)
                } else {
                    Color32::from_rgb(238, 238, 238)
                },
                bg_card: Color32::from_rgb(255, 255, 255),
                bg_card_hover: Color32::from_rgb(246, 246, 246),
                bg_card_selected: Color32::from_rgb(234, 234, 234),
                bg_subtle: Color32::from_rgb(249, 249, 249),
                border_card: Color32::from_rgba_unmultiplied(0, 0, 0, 18),
                border_subtle: Color32::from_rgba_unmultiplied(0, 0, 0, 11),
                text_primary: Color32::from_rgb(26, 26, 26),
                text_secondary: Color32::from_rgb(92, 92, 92),
                text_muted: Color32::from_rgb(138, 138, 138),
                accent: Color32::from_rgb(0, 95, 184),
                accent_text_on_fill: Color32::from_rgb(255, 255, 255),
                accent_subtle_bg: Color32::from_rgba_unmultiplied(0, 95, 184, 24),
                danger: Color32::from_rgb(196, 43, 28),
                success: Color32::from_rgb(15, 123, 15),
                warning: Color32::from_rgb(157, 93, 0),
                chart_cpu: Color32::from_rgb(0, 103, 192),
                chart_mem: Color32::from_rgb(135, 60, 210),
                chart_disk: Color32::from_rgb(16, 137, 62),
                chart_net: Color32::from_rgb(202, 80, 16),
            },
        }
    }

    pub fn heatmap_cell_bg(&self, ratio: f32, mode: ThemeMode) -> Color32 {
        let clamped = ratio.clamp(0.0, 1.0);
        match mode {
            ThemeMode::Dark => {
                let alpha = (24.0 + clamped * 115.0) as u8;
                if clamped > 0.65 {
                    Color32::from_rgba_unmultiplied(215, 105, 45, alpha)
                } else if clamped > 0.25 {
                    Color32::from_rgba_unmultiplied(185, 125, 55, alpha)
                } else {
                    Color32::from_rgba_unmultiplied(96, 165, 215, alpha)
                }
            }
            ThemeMode::Light => {
                let alpha = (26.0 + clamped * 120.0) as u8;
                if clamped > 0.65 {
                    Color32::from_rgba_unmultiplied(235, 120, 60, alpha)
                } else if clamped > 0.25 {
                    Color32::from_rgba_unmultiplied(245, 175, 85, alpha)
                } else {
                    Color32::from_rgba_unmultiplied(255, 220, 145, alpha)
                }
            }
        }
    }
}

pub fn install_windows_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    let font_candidates = [
        r"C:\Windows\Fonts\SegUIVar.ttf",
        r"C:\Windows\Fonts\segoeui.ttf",
    ];

    for path in font_candidates {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert("SegoeUI".to_owned(), FontData::from_owned(bytes));
            if let Some(family) = fonts.families.get_mut(&FontFamily::Proportional) {
                family.insert(0, "SegoeUI".to_owned());
            }
            break;
        }
    }

    let mono_candidates = [
        r"C:\Windows\Fonts\CascadiaCode.ttf",
        r"C:\Windows\Fonts\consola.ttf",
    ];
    for path in mono_candidates {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert("WinMono".to_owned(), FontData::from_owned(bytes));
            if let Some(family) = fonts.families.get_mut(&FontFamily::Monospace) {
                family.insert(0, "WinMono".to_owned());
            }
            break;
        }
    }

    ctx.set_fonts(fonts);
}

pub fn apply_winui_style(ctx: &egui::Context, mode: ThemeMode, mica_active: bool) -> WinUiPalette {
    let palette = WinUiPalette::for_mode(mode, mica_active);
    let mut style: Style = (*ctx.style()).clone();

    // Generous WinUI 3 spacing so buttons, cards, and columns never touch side by side
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    style.spacing.window_margin = Margin::same(16.0);
    style.spacing.scroll.bar_width = 6.0;

    let mut visuals = match mode {
        ThemeMode::Dark => Visuals::dark(),
        ThemeMode::Light => Visuals::light(),
    };

    visuals.panel_fill = palette.bg_base;
    visuals.window_fill = palette.bg_card;
    visuals.faint_bg_color = palette.bg_subtle;
    visuals.extreme_bg_color = palette.bg_sidebar;
    visuals.override_text_color = Some(palette.text_primary);
    visuals.window_rounding = Rounding::same(8.0);
    visuals.menu_rounding = Rounding::same(8.0);
    visuals.window_stroke = Stroke::new(1.0_f32, palette.border_card);

    let control_rounding = Rounding::same(6.0);

    visuals.widgets.noninteractive.bg_fill = palette.bg_card;
    visuals.widgets.noninteractive.weak_bg_fill = palette.bg_card;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, palette.border_card);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, palette.text_primary);
    visuals.widgets.noninteractive.rounding = control_rounding;

    visuals.widgets.inactive.bg_fill = palette.bg_card;
    visuals.widgets.inactive.weak_bg_fill = palette.bg_card;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, palette.border_card);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, palette.text_primary);
    visuals.widgets.inactive.rounding = control_rounding;

    visuals.widgets.hovered.bg_fill = palette.bg_card_hover;
    visuals.widgets.hovered.weak_bg_fill = palette.bg_card_hover;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, palette.accent);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, palette.text_primary);
    visuals.widgets.hovered.rounding = control_rounding;

    visuals.widgets.active.bg_fill = palette.bg_card_selected;
    visuals.widgets.active.weak_bg_fill = palette.bg_card_selected;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, palette.accent);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, palette.text_primary);
    visuals.widgets.active.rounding = control_rounding;

    visuals.selection.bg_fill = palette.accent_subtle_bg;
    visuals.selection.stroke = Stroke::new(1.0_f32, palette.accent);

    style.visuals = visuals;
    ctx.set_style(style);

    palette
}
