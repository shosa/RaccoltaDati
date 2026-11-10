//! Touch widgets: big tiles and buttons, cards, traffic lights and icons
//! (Phosphor icon set) and the company logo.

use eframe::egui::{self, Align2, Color32, CornerRadius, Pos2, Rect, Response, Sense, Shadow, Shape, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};

use crate::dati::Semaforo;
use crate::theme::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Layers,
    Factory,
    Person,
    Play,
    Pause,
    Stop,
    Cube,
    Chart,
    Gear,
    Doc,
    Filter,
    Up,
    Down,
    Refresh,
    Exit,
    Calendar,
    Tag,
    Clipboard,
    Clock,
    Database,
    Check,
    Close,
    Plus,
    Key,
    Login,
    Logout,
    Pin,
    Backspace,
    Barcode,
    Keyboard,
    Search,
}

/// Font family names of the Phosphor icon set (registered in theme::install_fonts).
pub const PH: &str = "phosphor";
pub const PH_FILL: &str = "phosphor-fill";

/// Phosphor glyph for each icon; `true` = filled variant.
fn glifo(ic: Icon) -> (&'static str, bool) {
    use egui_phosphor::regular as ph;
    match ic {
        Icon::Layers => (ph::STACK, false),
        Icon::Factory => (ph::FACTORY, false),
        Icon::Person => (ph::USER, false),
        Icon::Play => (ph::PLAY, true),
        Icon::Pause => (ph::PAUSE, true),
        Icon::Stop => (ph::STOP, true),
        Icon::Cube => (ph::CUBE, false),
        Icon::Chart => (ph::CHART_BAR, false),
        Icon::Gear => (ph::GEAR, false),
        Icon::Doc => (ph::FILE_TEXT, false),
        Icon::Filter => (ph::FUNNEL, false),
        Icon::Up => (ph::ARROW_UP, false),
        Icon::Down => (ph::ARROW_DOWN, false),
        Icon::Refresh => (ph::ARROWS_CLOCKWISE, false),
        Icon::Exit | Icon::Logout => (ph::SIGN_OUT, false),
        Icon::Login => (ph::SIGN_IN, false),
        Icon::Calendar => (ph::CALENDAR_BLANK, false),
        Icon::Tag => (ph::TAG, false),
        Icon::Clipboard => (ph::CLIPBOARD_TEXT, false),
        Icon::Clock => (ph::CLOCK, false),
        Icon::Database => (ph::DATABASE, false),
        Icon::Check => (ph::CHECK, false),
        Icon::Close => (ph::X, false),
        Icon::Plus => (ph::PLUS, false),
        Icon::Key => (ph::KEY, false),
        Icon::Pin => (ph::MAP_PIN, false),
        Icon::Backspace => (ph::BACKSPACE, false),
        Icon::Barcode => (ph::BARCODE, false),
        Icon::Keyboard => (ph::KEYBOARD, false),
        Icon::Search => (ph::MAGNIFYING_GLASS, false),
    }
}

/// Icon from the Phosphor icon font, centred in `rect` and sized to fit it.
pub fn icon(p: &egui::Painter, rect: Rect, ic: Icon, color: Color32) {
    let (g, fill) = glifo(ic);
    let size = rect.width().min(rect.height());
    let font = egui::FontId::new(size, egui::FontFamily::Name(if fill { PH_FILL } else { PH }.into()));
    let galley = p.layout_no_wrap(g.to_string(), font, color);
    // centre the glyph's own box (the row height includes the font's line spacing)
    let pos = rect.center() - galley.rect.center().to_vec2();
    p.galley(pos, galley, color);
}

/// The company logo (Emmegiemme, supplied by the company), embedded in the program.
const LOGO_PNG: &[u8] = include_bytes!("../assets/logo_emmegiemme.png");

fn logo_texture(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let id = egui::Id::new("logo-aziendale");
    if let Some(t) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return Some(t);
    }
    let img = image::load_from_memory(LOGO_PNG).ok()?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    let tex = ctx.load_texture("logo-aziendale", color, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, tex.clone()));
    Some(tex)
}

/// Draws the company logo inside `rect`, keeping its proportions, left-aligned and
/// vertically centred. Returns the rectangle actually used.
pub fn logo_aziendale(p: &egui::Painter, rect: Rect) -> Rect {
    let Some(tex) = logo_texture(p.ctx()) else { return Rect::from_min_size(rect.min, Vec2::ZERO) };
    let [w, h] = tex.size();
    let ratio = w as f32 / h.max(1) as f32;
    let mut size = vec2(rect.height() * ratio, rect.height());
    if size.x > rect.width() {
        size = vec2(rect.width(), rect.width() / ratio);
    }
    let r = Rect::from_min_size(pos2(rect.left(), rect.center().y - size.y / 2.0), size);
    p.image(tex.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    r
}

/// Flat look test (person's request): no gradients, shadows or gloss. `false` brings them back.
/// Like the original, which paints none (ESP_FormMain::InitializeComponent 0x28bb0: system
/// buttons, UseVisualStyleBackColor, no FlatStyle, no LinearGradientBrush anywhere).
pub const PIATTO: bool = true;

pub fn shadow(p: &egui::Painter, rect: Rect, r: u8, strength: u8) {
    if PIATTO {
        return;
    }
    let sh = Shadow { offset: [0, 2], blur: 10, spread: 0, color: Color32::from_black_alpha(strength) };
    p.add(sh.as_shape(rect, CornerRadius::same(r)));
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Normal,
    Primary,
    Good,
    Danger,
    Dark,
    Orange,
    Info,
}

/// Big touch button: icon above the label. `enabled == false` greys it out.
pub fn tile(ui: &mut Ui, size: Vec2, ic: Option<Icon>, label: &str, kind: Kind, enabled: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let p = ui.painter();
    let pressed = resp.is_pointer_button_down_on();
    let (fill, fill2, fg, border) = match kind {
        Kind::Normal => (CARD, CARD_SOFT, TEXT, BORDER),
        Kind::Primary => (PRIMARY, PRIMARY_DARK, Color32::WHITE, PRIMARY_DARK),
        Kind::Good => (GOOD, GOOD_DARK, Color32::WHITE, GOOD_DARK),
        Kind::Danger => (DANGER, lerp(DANGER, Color32::BLACK, 0.25), Color32::WHITE, lerp(DANGER, Color32::BLACK, 0.25)),
        Kind::Dark => (EXIT, lerp(EXIT, Color32::BLACK, 0.3), Color32::WHITE, lerp(EXIT, Color32::BLACK, 0.3)),
        Kind::Orange => (ORANGE, ORANGE_DARK, Color32::WHITE, ORANGE_DARK),
        Kind::Info => (INFO, INFO_DARK, Color32::WHITE, INFO_DARK),
    };
    // disabled: faded colours
    let (fill, fill2) = if enabled { (fill, fill2) } else { (lerp(fill, BG, 0.55), lerp(fill2, BG, 0.55)) };
    let r = rect.shrink(if pressed { 1.5 } else { 0.0 });
    if enabled {
        shadow(p, r, 12, if pressed { 10 } else { 26 });
    }
    if PIATTO {
        // flat: one colour, the darker one while pressed
        p.rect_filled(r, 12.0, if pressed { fill2 } else { fill });
    } else {
        gradient_rect(p, r, 12, fill, fill2);
    }
    p.rect_stroke(r, 12.0, Stroke::new(1.0, border), StrokeKind::Inside);
    let fg = if enabled { fg } else { alpha(fg, 90) };
    let font = bold(if size.y >= 80.0 { 19.0 } else { 17.0 });
    match ic {
        // Icon-only button: icon centred in the tile, a bit larger
        Some(ic) if label.is_empty() => {
            let isz = (size.y.min(size.x) * 0.42).min(38.0);
            icon(p, Rect::from_center_size(r.center(), vec2(isz, isz)), ic, if kind == Kind::Normal && enabled { PRIMARY } else { fg });
        }
        Some(ic) if size.y >= 70.0 => {
            let isz = (size.y * 0.34).min(34.0);
            let ic_rect = Rect::from_center_size(pos2(r.center().x, r.top() + size.y * 0.36), vec2(isz, isz));
            icon(p, ic_rect, ic, if kind == Kind::Normal && enabled { PRIMARY } else { fg });
            p.text(pos2(r.center().x, r.top() + size.y * 0.74), Align2::CENTER_CENTER, label, font, fg);
        }
        Some(ic) => {
            let isz = size.y * 0.42;
            let galley = p.layout_no_wrap(label.to_string(), font, fg);
            let total = isz + if label.is_empty() { 0.0 } else { 10.0 + galley.size().x };
            let x0 = r.center().x - total / 2.0;
            icon(p, Rect::from_min_size(pos2(x0, r.center().y - isz / 2.0), vec2(isz, isz)), ic, if kind == Kind::Normal && enabled { PRIMARY } else { fg });
            if !label.is_empty() {
                p.galley(pos2(x0 + isz + 10.0, r.center().y - galley.size().y / 2.0), galley, fg);
            }
        }
        None => {
            p.text(r.center(), Align2::CENTER_CENTER, label, font, fg);
        }
    }
    resp
}

pub fn gradient_rect(p: &egui::Painter, rect: Rect, r: u8, top: Color32, bottom: Color32) {
    if top == bottom {
        p.rect_filled(rect, r as f32, top);
        return;
    }
    p.rect_filled(rect, r as f32, bottom);
    let upper = Rect::from_min_max(rect.min, pos2(rect.max.x, rect.center().y));
    p.rect_filled(upper, CornerRadius { nw: r, ne: r, sw: 0, se: 0 }, top);
    // soft blend band
    let band = Rect::from_min_max(pos2(rect.min.x, rect.center().y - 1.0), pos2(rect.max.x, rect.center().y + rect.height() * 0.2));
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(band.left_top(), top);
    mesh.colored_vertex(band.right_top(), top);
    mesh.colored_vertex(band.right_bottom(), bottom);
    mesh.colored_vertex(band.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(Shape::mesh(mesh));
}

/// Tab of the main tab row.
pub fn tab(ui: &mut Ui, size: Vec2, ic: Icon, label: &str, selected: bool) -> Response {
    tile(ui, size, Some(ic), label, if selected { Kind::Primary } else { Kind::Normal }, true)
}

/// Selectable card background; returns the response for the whole card.
pub fn card(ui: &mut Ui, rect: Rect, selected: bool, id: egui::Id) -> Response {
    let resp = ui.interact(rect, id, Sense::click());
    let p = ui.painter();
    shadow(p, rect, 14, if selected { 34 } else { 20 });
    p.rect_filled(rect, 14.0, if selected { SEL_FILL } else { CARD });
    p.rect_stroke(rect, 14.0, if selected { Stroke::new(2.5, SEL_BORDER) } else { Stroke::new(1.0, BORDER) }, StrokeKind::Inside);
    resp
}

pub fn sem_color(s: Semaforo) -> Color32 {
    match s {
        Semaforo::Verde => SEM_VERDE,
        Semaforo::Giallo => SEM_GIALLO,
        Semaforo::Azzurro => SEM_AZZURRO,
        Semaforo::Bianco => SEM_BIANCO,
        Semaforo::Rosso => SEM_ROSSO,
    }
}

pub fn semaforo(p: &egui::Painter, c: Pos2, r: f32, s: Semaforo) {
    let col = sem_color(s);
    if !PIATTO {
        p.circle_filled(c + vec2(0.0, 1.5), r, Color32::from_black_alpha(25));
    }
    p.circle_filled(c, r, col);
    p.circle_stroke(c, r, Stroke::new(1.2, lerp(col, Color32::BLACK, 0.25)));
    if !PIATTO {
        p.circle_filled(c + vec2(-r * 0.3, -r * 0.35), r * 0.28, alpha(Color32::WHITE, 110));
    }
}

/// Small caps label above a big value (header blocks, card figures).
pub fn label_value(p: &egui::Painter, pos: Pos2, label: &str, value: &str, size: f32, color: Color32, max_w: f32) {
    p.text(pos, Align2::LEFT_TOP, label, reg(size * 0.62), MUTED);
    let v = elide(p, value, bold(size), max_w);
    p.text(pos + vec2(0.0, size * 0.78), Align2::LEFT_TOP, v, bold(size), color);
}

/// Cuts `text` with "…" so it fits in `max_w`.
pub fn elide(p: &egui::Painter, text: &str, font: egui::FontId, max_w: f32) -> String {
    let w = |t: &str| p.layout_no_wrap(t.to_string(), font.clone(), TEXT).size().x;
    if max_w <= 0.0 || w(text) <= max_w {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        let t: String = chars[..mid].iter().collect::<String>() + "…";
        if w(&t) <= max_w { lo = mid } else { hi = mid - 1 }
    }
    chars[..lo].iter().collect::<String>() + "…"
}

/// A vertical divider line.
pub fn vsep(p: &egui::Painter, x: f32, y0: f32, y1: f32) {
    p.line_segment([pos2(x, y0), pos2(x, y1)], Stroke::new(1.0, BORDER));
}
