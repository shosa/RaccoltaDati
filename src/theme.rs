//! Bright touch theme: light blue-grey background, white cards, deep blue accent,
//! large type (the person's reference picture sets shapes, colours and sizes).

use std::sync::Arc;

use eframe::egui::{self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle};

pub const BG: Color32 = Color32::from_rgb(234, 239, 245);
pub const BG_TOP: Color32 = Color32::from_rgb(244, 247, 251);
pub const HEADER: Color32 = Color32::from_rgb(250, 251, 253);
pub const CARD: Color32 = Color32::WHITE;
pub const CARD_SOFT: Color32 = Color32::from_rgb(241, 245, 250);
pub const BORDER: Color32 = Color32::from_rgb(211, 220, 231);
pub const BORDER_HI: Color32 = Color32::from_rgb(170, 185, 204);
pub const SEL_FILL: Color32 = Color32::from_rgb(220, 234, 251);
pub const SEL_BORDER: Color32 = Color32::from_rgb(59, 130, 246);
pub const PRIMARY: Color32 = Color32::from_rgb(21, 101, 192);
pub const PRIMARY_DARK: Color32 = Color32::from_rgb(10, 63, 134);
pub const TEXT: Color32 = Color32::from_rgb(15, 23, 42);
pub const MUTED: Color32 = Color32::from_rgb(71, 85, 105);
pub const FAINT: Color32 = Color32::from_rgb(148, 163, 184);
pub const GOOD: Color32 = Color32::from_rgb(22, 128, 61);
pub const GOOD_DARK: Color32 = Color32::from_rgb(12, 92, 42);
pub const DANGER: Color32 = Color32::from_rgb(224, 68, 46);
pub const WARN: Color32 = Color32::from_rgb(217, 119, 6);
/// INFO COMMESSA button (search, reads only).
pub const INFO: Color32 = Color32::from_rgb(0, 131, 143);
pub const INFO_DARK: Color32 = Color32::from_rgb(0, 96, 100);
/// BARCODE button (person's choice). Kept apart from the original's only orange, the SPC
/// alarm `Color.OrangeRed` (255,69,0) of ESP_FormMain::timerSPC_Tick 0x25120.
pub const ORANGE: Color32 = Color32::from_rgb(245, 124, 0);
pub const ORANGE_DARK: Color32 = Color32::from_rgb(196, 86, 0);
pub const STATUS: Color32 = Color32::from_rgb(28, 39, 56);
pub const EXIT: Color32 = Color32::from_rgb(51, 65, 85);

pub const SEM_VERDE: Color32 = Color32::from_rgb(34, 164, 71);
pub const SEM_GIALLO: Color32 = Color32::from_rgb(245, 214, 10);
pub const SEM_AZZURRO: Color32 = Color32::from_rgb(125, 196, 242);
pub const SEM_BIANCO: Color32 = Color32::from_rgb(255, 255, 255);
pub const SEM_ROSSO: Color32 = Color32::from_rgb(224, 72, 72);

pub fn alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

pub fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

fn load_font(fonts: &mut FontDefinitions, name: &str, files: &[&str]) -> bool {
    for f in files {
        if let Ok(bytes) = std::fs::read(format!(r"C:\Windows\Fonts\{f}")) {
            fonts.font_data.insert(name.to_string(), Arc::new(FontData::from_owned(bytes)));
            return true;
        }
    }
    false
}

/// Segoe UI from Windows when present; egui's own fonts stay as fallback.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let base = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mut prop = Vec::new();
    if load_font(&mut fonts, "ui", &["segoeui.ttf", "SegUIVar.ttf"]) {
        prop.push("ui".to_string());
    }
    let mut bold = Vec::new();
    if load_font(&mut fonts, "ui-bold", &["segoeuib.ttf", "seguisb.ttf"]) {
        bold.push("ui-bold".to_string());
    } else {
        bold = prop.clone();
    }
    let sym = load_font(&mut fonts, "sym", &["seguisym.ttf"]);
    for v in [&mut prop, &mut bold] {
        if sym {
            v.push("sym".into());
        }
        v.extend(base.iter().cloned());
    }
    fonts.families.insert(FontFamily::Proportional, prop);
    fonts.families.insert(FontFamily::Name("bold".into()), bold);
    // Phosphor icon set: line and filled variants, each as its own family
    for (name, bytes) in [(crate::ui::PH, egui_phosphor::bytes::regular::FONT), (crate::ui::PH_FILL, egui_phosphor::bytes::fill::FONT)] {
        fonts.font_data.insert(name.to_string(), Arc::new(FontData::from_static(bytes)));
        fonts.families.insert(FontFamily::Name(name.into()), vec![name.to_string()]);
    }
    ctx.set_fonts(fonts);
}

pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}

pub fn reg(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

pub fn apply(ctx: &egui::Context) {
    install_fonts(ctx);
    ctx.set_theme(egui::Theme::Light);
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Heading, bold(26.0)),
            (TextStyle::Body, reg(18.0)),
            (TextStyle::Button, bold(18.0)),
            (TextStyle::Small, reg(14.0)),
            (TextStyle::Monospace, FontId::new(16.0, FontFamily::Monospace)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(10.0, 10.0);
        style.spacing.button_padding = egui::vec2(16.0, 10.0);
        style.spacing.interact_size.y = 44.0;
        style.spacing.scroll.bar_width = 14.0;
        style.spacing.scroll.floating = false;
        // Touch: finger drags scroll lists
        style.interaction.selectable_labels = false;

        let v = &mut style.visuals;
        v.dark_mode = false;
        v.panel_fill = BG;
        v.window_fill = CARD;
        v.window_stroke = Stroke::new(1.0, BORDER);
        v.window_corner_radius = CornerRadius::same(16);
        v.extreme_bg_color = CARD;
        v.text_edit_bg_color = Some(CARD);
        v.faint_bg_color = CARD_SOFT;
        v.selection.bg_fill = SEL_FILL;
        v.selection.stroke = Stroke::new(1.5, SEL_BORDER);
        v.override_text_color = Some(TEXT);
        let r = CornerRadius::same(10);
        for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
            w.corner_radius = r;
        }
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
        v.widgets.inactive.bg_fill = CARD;
        v.widgets.inactive.weak_bg_fill = CARD;
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
        v.widgets.hovered.weak_bg_fill = CARD_SOFT;
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, BORDER_HI);
        v.widgets.active.weak_bg_fill = SEL_FILL;
        v.widgets.active.bg_stroke = Stroke::new(1.5, SEL_BORDER);
    });
}
