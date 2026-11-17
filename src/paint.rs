//! Desenho da janela de configurações com Direct2D/DirectWrite: cores do tema escuro,
//! retângulos arredondados, círculos e texto. Tudo em DIPs (1 DIP = 1 pixel a 100% de
//! escala); o Direct2D aplica a escala da tela sozinho.

use windows::Win32::Graphics::Direct2D::Common::{D2D_RECT_F, D2D1_COLOR_F};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_DRAW_TEXT_OPTIONS_CLIP, D2D1_ELLIPSE, D2D1_ROUNDED_RECT,
    ID2D1HwndRenderTarget, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT, DWRITE_FONT_WEIGHT_NORMAL,
    DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_TEXT_METRICS,
    DWRITE_WORD_WRAPPING_NO_WRAP, IDWriteFactory, IDWriteTextFormat,
};
use windows::core::{PCWSTR, Result, w};
use windows_numerics::Vector2;

/// Cor no formato 0xRRGGBB.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color(pub u32);

impl Color {
    fn d2d(self) -> D2D1_COLOR_F {
        let channel = |shift: u32| ((self.0 >> shift) & 0xFF) as f32 / 255.0;
        D2D1_COLOR_F { r: channel(16), g: channel(8), b: channel(0), a: 1.0 }
    }
}

/// Paleta do tema escuro (inspirada nas Configurações do Windows 11).
pub mod theme {
    use super::Color;

    pub const BG: Color = Color(0x202020);
    pub const CARD: Color = Color(0x2B2B2B);
    pub const CARD_BORDER: Color = Color(0x1A1A1A);
    pub const TEXT: Color = Color(0xFFFFFF);
    pub const TEXT_2: Color = Color(0xC5C5C5);
    pub const TEXT_3: Color = Color(0x9A9A9A);
    pub const TEXT_DISABLED: Color = Color(0x6E6E6E);
    pub const ACCENT: Color = Color(0x4CC2FF);
    pub const ACCENT_HOVER: Color = Color(0x47B1E8);
    pub const ACCENT_PRESSED: Color = Color(0x42A1D2);
    pub const ON_ACCENT: Color = Color(0x000000);
    pub const CONTROL: Color = Color(0x373737);
    pub const CONTROL_HOVER: Color = Color(0x3E3E3E);
    pub const CONTROL_PRESSED: Color = Color(0x323232);
    pub const CONTROL_BORDER: Color = Color(0x444444);
    pub const CONTROL_DISABLED: Color = Color(0x2F2F2F);
    pub const INPUT: Color = Color(0x1C1C1C);
    pub const INPUT_BORDER: Color = Color(0x484848);
    pub const INPUT_BORDER_HOVER: Color = Color(0x6A6A6A);
    pub const SUBTLE_HOVER: Color = Color(0x2E2E2E);
    pub const KEYCAP: Color = Color(0x3F3F3F);
    pub const KEYCAP_BORDER: Color = Color(0x575757);
    pub const TRACK: Color = Color(0x5A5A5A);
    pub const THUMB: Color = Color(0x454545);
    pub const THUMB_RING: Color = Color(0x5F5F5F);
    pub const ROW_SELECTED: Color = Color(0x383838);
    pub const ROW_HOVER: Color = Color(0x2F2F2F);
    pub const SCROLL: Color = Color(0x6A6A6A);
    pub const SUCCESS: Color = Color(0x6CCB5F);
    pub const ERROR: Color = Color(0xFF99A4);
    pub const FOCUS: Color = Color(0xFFFFFF);
}

/// Ícones da fonte Segoe MDL2 Assets (vem com o Windows 10 e 11).
pub mod glyph {
    pub const CHECK: &str = "\u{E73E}";
    pub const ERROR: &str = "\u{E783}";
    pub const INFO: &str = "\u{E946}";
    pub const ADD: &str = "\u{E710}";
    pub const SEARCH: &str = "\u{E721}";
    pub const DELETE: &str = "\u{E74D}";
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.right() && py >= self.y && py < self.bottom()
    }

    /// Encolhe (valores positivos) ou expande (negativos) a partir das bordas.
    pub fn inset(&self, dx: f32, dy: f32) -> Self {
        Self::new(self.x + dx, self.y + dy, self.w - 2.0 * dx, self.h - 2.0 * dy)
    }

    pub fn union(&self, other: Rect) -> Self {
        let (x, y) = (self.x.min(other.x), self.y.min(other.y));
        Self::new(x, y, self.right().max(other.right()) - x, self.bottom().max(other.bottom()) - y)
    }

    fn d2d(&self) -> D2D_RECT_F {
        D2D_RECT_F { left: self.x, top: self.y, right: self.right(), bottom: self.bottom() }
    }
}

#[derive(Clone, Copy)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// Formatos de texto usados na janela.
pub struct Fonts {
    pub body: IDWriteTextFormat,
    pub strong: IDWriteTextFormat,
    pub caption: IDWriteTextFormat,
    pub key: IDWriteTextFormat,
    pub icon: IDWriteTextFormat,
    pub icon_small: IDWriteTextFormat,
}

impl Fonts {
    pub fn new(dw: &IDWriteFactory) -> Result<Self> {
        let ui = w!("Segoe UI");
        let icons = w!("Segoe MDL2 Assets");
        Ok(Self {
            body: format(dw, ui, DWRITE_FONT_WEIGHT_NORMAL, 14.0)?,
            strong: format(dw, ui, DWRITE_FONT_WEIGHT_SEMI_BOLD, 14.0)?,
            caption: format(dw, ui, DWRITE_FONT_WEIGHT_NORMAL, 12.0)?,
            key: format(dw, ui, DWRITE_FONT_WEIGHT_SEMI_BOLD, 13.0)?,
            icon: format(dw, icons, DWRITE_FONT_WEIGHT_NORMAL, 14.0)?,
            icon_small: format(dw, icons, DWRITE_FONT_WEIGHT_NORMAL, 12.0)?,
        })
    }
}

fn format(dw: &IDWriteFactory, family: PCWSTR, weight: DWRITE_FONT_WEIGHT, size: f32) -> Result<IDWriteTextFormat> {
    unsafe {
        let format = dw.CreateTextFormat(
            family,
            None,
            weight,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            size,
            w!("pt-br"),
        )?;
        format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        Ok(format)
    }
}

/// Desenha numa área de desenho Direct2D (entre BeginDraw e EndDraw).
pub struct Painter<'a> {
    target: &'a ID2D1HwndRenderTarget,
    dw: &'a IDWriteFactory,
    brush: ID2D1SolidColorBrush,
}

impl<'a> Painter<'a> {
    pub fn new(target: &'a ID2D1HwndRenderTarget, dw: &'a IDWriteFactory) -> Result<Self> {
        let brush = unsafe { target.CreateSolidColorBrush(&Color(0).d2d(), None)? };
        Ok(Self { target, dw, brush })
    }

    fn set(&self, color: Color) {
        unsafe { self.brush.SetColor(&color.d2d()) };
    }

    pub fn clear(&self, color: Color) {
        unsafe { self.target.Clear(Some(&color.d2d())) };
    }

    pub fn fill(&self, r: Rect, radius: f32, color: Color) {
        self.set(color);
        let rounded = D2D1_ROUNDED_RECT { rect: r.d2d(), radiusX: radius, radiusY: radius };
        unsafe { self.target.FillRoundedRectangle(&rounded, &self.brush) };
    }

    /// Contorno por dentro do retângulo (a linha não vaza para fora).
    pub fn stroke(&self, r: Rect, radius: f32, color: Color, width: f32) {
        self.set(color);
        let half = width / 2.0;
        let rounded = D2D1_ROUNDED_RECT { rect: r.inset(half, half).d2d(), radiusX: radius, radiusY: radius };
        unsafe { self.target.DrawRoundedRectangle(&rounded, &self.brush, width, None) };
    }

    pub fn circle(&self, cx: f32, cy: f32, radius: f32, color: Color) {
        self.set(color);
        let ellipse = D2D1_ELLIPSE { point: Vector2 { X: cx, Y: cy }, radiusX: radius, radiusY: radius };
        unsafe { self.target.FillEllipse(&ellipse, &self.brush) };
    }

    /// Texto numa linha, centralizado na vertical dentro de `r`.
    pub fn text(&self, text: &str, font: &IDWriteTextFormat, r: Rect, color: Color, align: Align) {
        self.set(color);
        let wide: Vec<u16> = text.encode_utf16().collect();
        let alignment = match align {
            Align::Left => DWRITE_TEXT_ALIGNMENT_LEADING,
            Align::Center => DWRITE_TEXT_ALIGNMENT_CENTER,
            Align::Right => DWRITE_TEXT_ALIGNMENT_TRAILING,
        };
        unsafe {
            let _ = font.SetTextAlignment(alignment);
            self.target.DrawText(
                &wide,
                font,
                &r.d2d(),
                &self.brush,
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }
    }

    /// Largura do texto, em DIPs.
    pub fn measure(&self, text: &str, font: &IDWriteTextFormat) -> f32 {
        let wide: Vec<u16> = text.encode_utf16().collect();
        unsafe {
            let Ok(layout) = self.dw.CreateTextLayout(&wide, font, 10_000.0, 100.0) else { return 0.0 };
            let mut metrics = DWRITE_TEXT_METRICS::default();
            if layout.GetMetrics(&mut metrics).is_ok() { metrics.widthIncludingTrailingWhitespace } else { 0.0 }
        }
    }

    /// Desenha só dentro de `r` (o que passar é cortado).
    pub fn clipped(&self, r: Rect, draw: impl FnOnce()) {
        unsafe { self.target.PushAxisAlignedClip(&r.d2d(), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE) };
        draw();
        unsafe { self.target.PopAxisAlignedClip() };
    }
}
