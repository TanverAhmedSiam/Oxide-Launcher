//! Look and feel: colours, spacing, fonts, icons and a few small widgets.
//! Everything is drawn by egui itself (no web view), so it costs almost no memory.
//! Pure black "AMOLED" background, neutral greys, patina-green accent and copper highlights
//! (matching the Oxide logo). Soft-yellow accent, no blue or indigo anywhere.
use eframe::egui::{
    self, vec2, Align2, Color32, FontFamily, FontId, Margin, RichText, Rounding, Sense, Stroke, TextStyle,
};

/// Phosphor icon font (one small font file, loaded once).
pub use egui_phosphor::regular as ic;

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color32,
    pub card: Color32,
    pub card_hi: Color32,
    pub input: Color32,
    pub stroke: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub accent_text: Color32,
    pub danger: Color32,
    pub warn: Color32,
    pub ok: Color32,
    /// Copper, used for secondary highlights (loader chips, "will be installed").
    pub info: Color32,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

pub fn palette(dark: bool) -> Palette {
    if dark {
        Palette {
            bg: rgb(0x00, 0x00, 0x00),
            card: rgb(0x0d, 0x0d, 0x0d),
            card_hi: rgb(0x18, 0x18, 0x18),
            input: rgb(0x05, 0x05, 0x05),
            stroke: rgb(0x24, 0x24, 0x24),
            text: rgb(0xec, 0xec, 0xec),
            muted: rgb(0x8c, 0x8c, 0x8c),
            accent: rgb(0xf0, 0xd2, 0x6e),      // soft yellow
            accent_text: rgb(0x1b, 0x15, 0x04),
            danger: rgb(0xf2, 0x6b, 0x6b),
            warn: rgb(0xf5, 0x9e, 0x4b),        // orange, clearly different from the accent
            ok: rgb(0x6f, 0xd4, 0x7f),
            info: rgb(0x4f, 0xd1, 0xa5),        // patina green from the logo
        }
    } else {
        Palette {
            bg: rgb(0xf4, 0xf4, 0xf4),
            card: rgb(0xff, 0xff, 0xff),
            card_hi: rgb(0xee, 0xee, 0xee),
            input: rgb(0xf9, 0xf9, 0xf9),
            stroke: rgb(0xdc, 0xdc, 0xdc),
            text: rgb(0x18, 0x18, 0x18),
            muted: rgb(0x6b, 0x6b, 0x6b),
            accent: rgb(0xe9, 0xc2, 0x3a),
            accent_text: rgb(0x1b, 0x15, 0x04),
            danger: rgb(0xd2, 0x3b, 0x3b),
            warn: rgb(0xc2, 0x6a, 0x0a),
            ok: rgb(0x1a, 0x9a, 0x4a),
            info: rgb(0x0f, 0x8f, 0x6c),
        }
    }
}

pub fn pal(ui: &egui::Ui) -> Palette {
    palette(ui.visuals().dark_mode)
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

fn line(color: Color32) -> Stroke {
    Stroke::new(1.0_f32, color)
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    ctx.set_fonts(fonts);
}

/// Colours, rounding, spacing and text sizes for the whole app.
pub fn apply(ctx: &egui::Context, dark: bool) {
    let p = palette(dark);
    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.override_text_color = Some(p.text);
    v.panel_fill = p.bg;
    v.window_fill = p.card;
    v.extreme_bg_color = p.input;
    v.faint_bg_color = p.card;
    v.code_bg_color = p.input;
    v.window_stroke = line(p.stroke);
    v.window_rounding = Rounding::same(14.0);
    v.menu_rounding = Rounding::same(10.0);
    let shadow = egui::epaint::Shadow {
        offset: vec2(0.0, 10.0),
        blur: 28.0,
        spread: 0.0,
        color: Color32::from_black_alpha(if dark { 200 } else { 45 }),
    };
    v.window_shadow = shadow;
    v.popup_shadow = shadow;
    v.hyperlink_color = p.accent;
    // The progress bar and selected list entries use the selection colour.
    v.selection.bg_fill = p.accent;
    v.selection.stroke = line(p.accent_text);

    let r = Rounding::same(9.0);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.card;
    w.noninteractive.weak_bg_fill = p.card;
    w.noninteractive.bg_stroke = line(p.stroke);
    w.noninteractive.fg_stroke = line(p.text);
    w.noninteractive.rounding = r;

    w.inactive.bg_fill = p.card_hi;
    w.inactive.weak_bg_fill = p.card_hi;
    w.inactive.bg_stroke = line(p.stroke);
    w.inactive.fg_stroke = line(p.text);
    w.inactive.rounding = r;

    let hover = mix(p.card_hi, p.accent, 0.14);
    w.hovered.bg_fill = hover;
    w.hovered.weak_bg_fill = hover;
    w.hovered.bg_stroke = line(p.accent);
    w.hovered.fg_stroke = line(p.text);
    w.hovered.rounding = r;
    w.hovered.expansion = 0.0;

    let active = mix(p.card_hi, p.accent, 0.28);
    w.active.bg_fill = active;
    w.active.weak_bg_fill = active;
    w.active.bg_stroke = line(p.accent);
    w.active.fg_stroke = line(p.text);
    w.active.rounding = r;
    w.active.expansion = 0.0;

    w.open.bg_fill = p.card_hi;
    w.open.weak_bg_fill = p.card_hi;
    w.open.bg_stroke = line(p.accent);
    w.open.fg_stroke = line(p.text);
    w.open.rounding = r;

    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        s.spacing.item_spacing = vec2(8.0, 8.0);
        s.spacing.button_padding = vec2(12.0, 7.0);
        s.spacing.interact_size.y = 32.0;
        s.spacing.window_margin = Margin::same(16.0);
        s.spacing.menu_margin = Margin::same(8.0);
        s.text_styles = [
            (TextStyle::Heading, FontId::new(22.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(14.5, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(14.5, FontFamily::Proportional)),
            (TextStyle::Small, FontId::new(12.0, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
        ]
        .into();
    });
}

// ---------------- logo ----------------

/// The Oxide logo: a big version for the home screen and a small one for the header.
pub struct Logos {
    pub big: egui::TextureHandle,
    pub small: egui::TextureHandle,
}

fn decode(bytes: &[u8]) -> Option<egui::ColorImage> {
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    Some(egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], img.as_raw()))
}

pub fn load_logos(ctx: &egui::Context) -> Option<Logos> {
    let big = decode(include_bytes!("../assets/logo.png"))?;
    let small = decode(include_bytes!("../assets/logo_small.png"))?;
    Some(Logos {
        big: ctx.load_texture("logo_big", big, egui::TextureOptions::LINEAR),
        small: ctx.load_texture("logo_small", small, egui::TextureOptions::LINEAR),
    })
}

/// Draws a logo texture at a given height, keeping its proportions.
pub fn logo_image(ui: &mut egui::Ui, tex: &egui::TextureHandle, height: f32) {
    let s = tex.size_vec2();
    let w = height * s.x / s.y.max(1.0);
    ui.add(egui::Image::from_texture(egui::load::SizedTexture::from_handle(tex)).fit_to_exact_size(vec2(w, height)));
}

/// Window / taskbar icon (the logo, 64x64).
pub fn app_icon() -> egui::IconData {
    match image::load_from_memory(include_bytes!("../assets/icon_64.png")) {
        Ok(img) => {
            let img = img.to_rgba8();
            let (width, height) = img.dimensions();
            egui::IconData { rgba: img.into_raw(), width, height }
        }
        Err(_) => egui::IconData { rgba: vec![0; 4], width: 1, height: 1 },
    }
}

// ---------------- small widgets ----------------

/// A rounded panel that groups related controls.
pub fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    card_with(ui, false, add)
}

/// Same, with an accent outline when `selected`.
pub fn card_with<R>(ui: &mut egui::Ui, selected: bool, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let p = pal(ui);
    const MARGIN: f32 = 14.0;
    // The frame adds its margin and outline on both sides, so shrink the content to fit exactly.
    let inner = (ui.available_width() - 2.0 * (MARGIN + 1.0)).max(0.0);
    egui::Frame::none()
        .fill(p.card)
        .stroke(line(if selected { p.accent } else { p.stroke }))
        .rounding(Rounding::same(14.0))
        .inner_margin(Margin::same(MARGIN))
        .show(ui, |ui| {
            ui.set_width(inner);
            add(ui)
        })
        .inner
}

/// "icon  Title" heading for a card.
pub fn section(ui: &mut egui::Ui, icon: &str, title: &str) {
    let p = pal(ui);
    ui.horizontal(|ui| {
        ui.label(RichText::new(icon).size(16.0).color(p.accent));
        ui.label(RichText::new(title).size(14.5).strong());
    });
}

pub fn muted(ui: &mut egui::Ui, text: impl Into<String>) {
    let p = pal(ui);
    ui.label(RichText::new(text.into()).size(12.0).color(p.muted));
}

pub fn chip(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::none()
        .fill(color.gamma_multiply(0.16))
        .rounding(Rounding::same(7.0))
        .inner_margin(Margin::symmetric(8.0, 2.0))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(11.5).strong().color(color));
        });
}

/// The big call-to-action button (Install / Play).
pub fn primary_button(ui: &mut egui::Ui, label: &str, fill: Color32, enabled: bool) -> egui::Response {
    let p = pal(ui);
    let btn = egui::Button::new(RichText::new(label).size(18.0).strong().color(p.accent_text))
        .fill(fill)
        .rounding(Rounding::same(12.0))
        .min_size(vec2(ui.available_width(), 50.0));
    ui.add_enabled(enabled, btn)
}

/// Compact green button.
pub fn accent_btn(p: &Palette, text: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text).strong().color(p.accent_text))
        .fill(p.accent)
        .stroke(Stroke::NONE)
        .rounding(Rounding::same(9.0))
}

/// Compact outlined red button.
pub fn danger_btn(p: &Palette, text: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text).color(p.danger))
        .fill(Color32::TRANSPARENT)
        .stroke(line(p.danger.gamma_multiply(0.6)))
        .rounding(Rounding::same(9.0))
}

/// Tile with an icon over a small label (toolbar).
pub fn tool_button(ui: &mut egui::Ui, icon: &str, label: &str, enabled: bool, width: f32) -> egui::Response {
    let p = pal(ui);
    let (rect, resp) = ui.allocate_exact_size(
        vec2(width, 58.0),
        if enabled { Sense::click() } else { Sense::hover() },
    );
    let hot = enabled && resp.hovered();
    let (fill, stroke) = if hot {
        (mix(p.card, p.accent, 0.12), p.accent)
    } else {
        (p.card, p.stroke)
    };
    let painter = ui.painter();
    painter.rect(rect, Rounding::same(12.0), fill, line(stroke));
    let icon_col = if !enabled {
        p.muted.gamma_multiply(0.5)
    } else if hot {
        p.accent
    } else {
        p.text
    };
    painter.text(
        rect.center_top() + vec2(0.0, 22.0),
        Align2::CENTER_CENTER,
        icon,
        FontId::proportional(21.0),
        icon_col,
    );
    painter.text(
        rect.center_top() + vec2(0.0, 44.0),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(11.5),
        if enabled { p.muted } else { p.muted.gamma_multiply(0.5) },
    );
    if enabled {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
}

/// Pill-style selector: all options side by side, the chosen one filled green.
pub fn segmented<T: PartialEq + Copy>(ui: &mut egui::Ui, value: &mut T, items: &[(T, &str)]) {
    let p = pal(ui);
    let gap = 4.0;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        let n = items.len().max(1) as f32;
        let w = ((ui.available_width() - gap * (n - 1.0)) / n).floor().max(40.0);
        for (v, label) in items {
            let sel = *value == *v;
            let (fill, txt, stroke) = if sel {
                (p.accent, p.accent_text, Stroke::NONE)
            } else {
                (p.card, p.text, line(p.stroke))
            };
            let b = egui::Button::new(RichText::new(*label).strong().color(txt))
                .fill(fill)
                .stroke(stroke)
                .rounding(Rounding::same(9.0))
                .min_size(vec2(w, 32.0));
            if ui.add(b).clicked() {
                *value = *v;
            }
        }
    });
}

pub fn progress(ui: &mut egui::Ui, frac: f32, text: impl Into<RichText>, animate: bool) {
    let text: RichText = text.into();
    ui.add(
        egui::ProgressBar::new(frac)
            .text(text)
            .animate(animate)
            .rounding(Rounding::same(8.0))
            .desired_height(20.0),
    );
}

/// Round avatar with the first letter of the name.
pub fn avatar(ui: &mut egui::Ui, name: &str, size: f32) {
    const COLORS: [Color32; 6] = [
        rgb(0xf0, 0xd2, 0x6e), // soft yellow
        rgb(0x4f, 0xd1, 0xa5), // patina
        rgb(0xe0, 0x8a, 0x52), // copper
        rgb(0xf2, 0x6b, 0x6b), // coral
        rgb(0xa3, 0xd9, 0x5b), // lime
        rgb(0x2f, 0xb7, 0xa8), // teal
    ];
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    let idx = name.bytes().fold(0u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32)) as usize % COLORS.len();
    let letter = name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into());
    let painter = ui.painter();
    painter.circle_filled(rect.center(), size / 2.0, COLORS[idx]);
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        letter,
        FontId::proportional(size * 0.5),
        Color32::from_rgb(0x08, 0x08, 0x08),
    );
}
