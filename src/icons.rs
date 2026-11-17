//! Ícones da bandeja, desenhados por código (nítidos em qualquer escala de tela).
//!
//! Cada estado é um quadrado arredondado colorido com um alto-falante branco: azul com duas
//! ondas (volume alto), cinza com uma onda (volume baixo), cinza riscado (atalho fora do ar)
//! e vermelho com X (nada encontrado). O fundo colorido evita confusão com o ícone de volume
//! do próprio Windows.

use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateBitmap, CreateDIBSection, DIB_RGB_COLORS, DeleteObject,
};
use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, HICON, ICONINFO};
use windows::core::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconState {
    /// Jogo no máximo (ou ainda nada alternado).
    Normal,
    /// Jogo no mínimo.
    Lowered,
    /// A bind não achou áudio do alvo (piscada curta).
    NotFound,
    /// Atalho fora do ar: pausado, configurações abertas ou em uso por outro programa.
    Unavailable,
}

pub const ALL: [IconState; 4] = [IconState::Normal, IconState::Lowered, IconState::NotFound, IconState::Unavailable];

type Rgb = (u8, u8, u8);

const WHITE: Rgb = (255, 255, 255);
const BLUE: Rgb = (37, 99, 235);
const GRAY: Rgb = (107, 114, 128);
const RED: Rgb = (220, 38, 38);

/// (fundo, desenho). Abaixado e fora do ar usam o mesmo cinza; o que muda é o desenho
/// (uma onda só × alto-falante riscado).
fn colors(state: IconState) -> (Rgb, Rgb) {
    match state {
        IconState::Normal => (BLUE, WHITE),
        IconState::Lowered | IconState::Unavailable => (GRAY, WHITE),
        IconState::NotFound => (RED, WHITE),
    }
}

// Geometria numa grade de 16×16 unidades.
const SPEAKER: [(f32, f32); 6] = [(2.5, 6.2), (5.0, 6.2), (8.3, 3.0), (8.3, 13.0), (5.0, 9.8), (2.5, 9.8)];
const WAVE_CENTER: (f32, f32) = (8.3, 8.0);
const WAVE_SMALL: (f32, f32) = (2.0, 3.3);
const WAVE_BIG: (f32, f32) = (4.3, 5.6);

/// Subamostras por eixo (suavização das bordas).
const SS: u32 = 4;

/// Pixels ARGB (alfa não pré-multiplicado), linha a linha de cima para baixo.
pub fn render(state: IconState, size: u32) -> Vec<u32> {
    let (bg, fg) = colors(state);
    let scale = 16.0 / size as f32;
    let samples = (SS * SS) as f32;
    let mut pixels = vec![0u32; (size * size) as usize];

    for y in 0..size {
        for x in 0..size {
            let (mut badge, mut glyph) = (0u32, 0u32);
            for sy in 0..SS {
                for sx in 0..SS {
                    let u = (x as f32 + (sx as f32 + 0.5) / SS as f32) * scale;
                    let v = (y as f32 + (sy as f32 + 0.5) / SS as f32) * scale;
                    if in_rounded_rect(u, v, 0.5, 0.5, 15.5, 15.5, 3.5) {
                        badge += 1;
                        if in_glyph(state, u, v) {
                            glyph += 1;
                        }
                    }
                }
            }
            if badge == 0 {
                continue;
            }
            let alpha = (badge as f32 / samples * 255.0).round() as u32;
            let t = glyph as f32 / badge as f32;
            let mix = |b: u8, f: u8| (b as f32 + (f as f32 - b as f32) * t).round() as u32;
            let (r, g, b) = (mix(bg.0, fg.0), mix(bg.1, fg.1), mix(bg.2, fg.2));
            pixels[(y * size + x) as usize] = alpha << 24 | r << 16 | g << 8 | b;
        }
    }
    pixels
}

fn in_glyph(state: IconState, u: f32, v: f32) -> bool {
    let speaker = in_polygon(u, v, &SPEAKER);
    let small = in_wave(u, v, WAVE_SMALL);
    let big = in_wave(u, v, WAVE_BIG);
    match state {
        IconState::Normal => speaker || small || big,
        IconState::Lowered => speaker || small,
        IconState::NotFound => {
            let cross = dist_to_segment(u, v, (10.2, 6.2), (13.8, 9.8)).min(dist_to_segment(u, v, (10.2, 9.8), (13.8, 6.2)));
            speaker || cross <= 0.75
        }
        IconState::Unavailable => {
            let slash = dist_to_segment(u, v, (3.0, 13.0), (13.0, 3.0));
            ((speaker || small || big) && slash > 1.6) || slash <= 0.8
        }
    }
}

fn in_wave(u: f32, v: f32, (r_in, r_out): (f32, f32)) -> bool {
    let (dx, dy) = (u - WAVE_CENTER.0, v - WAVE_CENTER.1);
    let d = (dx * dx + dy * dy).sqrt();
    dx > 0.0 && dy.abs() <= dx * 1.15 && d >= r_in && d <= r_out
}

fn in_rounded_rect(u: f32, v: f32, x0: f32, y0: f32, x1: f32, y1: f32, r: f32) -> bool {
    if u < x0 || u > x1 || v < y0 || v > y1 {
        return false;
    }
    let (cx, cy) = (u.clamp(x0 + r, x1 - r), v.clamp(y0 + r, y1 - r));
    (u - cx).powi(2) + (v - cy).powi(2) <= r * r
}

fn in_polygon(u: f32, v: f32, poly: &[(f32, f32)]) -> bool {
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let ((xi, yi), (xj, yj)) = (poly[i], poly[j]);
        if (yi > v) != (yj > v) && u < (xj - xi) * (v - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn dist_to_segment(u: f32, v: f32, (ax, ay): (f32, f32), (bx, by): (f32, f32)) -> f32 {
    let (dx, dy) = (bx - ax, by - ay);
    let t = (((u - ax) * dx + (v - ay) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    ((u - ax - t * dx).powi(2) + (v - ay - t * dy).powi(2)).sqrt()
}

/// Cria o ícone do Windows para um estado, no tamanho pedido (em pixels).
pub fn create_hicon(state: IconState, size: u32) -> Result<HICON> {
    let pixels = render(state, size);
    let header = BITMAPINFOHEADER {
        biSize: size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: size as i32,
        biHeight: -(size as i32), // negativo = de cima para baixo
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        ..Default::default()
    };
    let info = BITMAPINFO { bmiHeader: header, ..Default::default() };
    // Máscara monocromática zerada: com cor de 32 bits, quem manda é o alfa.
    let mask_bits = vec![0u8; (size.div_ceil(16) * 2 * size) as usize];

    // SAFETY: os bitmaps são criados, copiados e apagados aqui; o buffer da seção DIB tem
    // exatamente size × size pixels de 32 bits.
    unsafe {
        let mut bits = std::ptr::null_mut();
        let color = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0)?;
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits.cast::<u32>(), pixels.len());
        let mask = CreateBitmap(size as i32, size as i32, 1, 1, Some(mask_bits.as_ptr().cast()));
        let icon = CreateIconIndirect(&ICONINFO {
            fIcon: true.into(),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        });
        let _ = DeleteObject(color.into());
        let _ = DeleteObject(mask.into());
        icon
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn todos_os_estados_tem_desenho_e_fundo() {
        for state in ALL {
            let px = render(state, 32);
            let (bg, fg) = colors(state);
            let has = |c: Rgb| px.iter().any(|&p| p >> 24 == 255 && (p >> 16 & 255, p >> 8 & 255, p & 255) == (c.0 as u32, c.1 as u32, c.2 as u32));
            assert!(has(bg), "{state:?} sem fundo");
            assert!(has(fg), "{state:?} sem desenho");
        }
    }

    #[test]
    fn cantos_sao_transparentes() {
        let px = render(IconState::Normal, 16);
        assert_eq!(px[0] >> 24, 0);
        assert_eq!(px[15] >> 24, 0);
    }

    /// Arquivo .ico do app (o ícone azul) com vários tamanhos, imagens de 32 bits com alfa.
    fn ico_file(sizes: &[u32]) -> Vec<u8> {
        let images: Vec<Vec<u8>> = sizes
            .iter()
            .map(|&size| {
                let px = render(IconState::Normal, size);
                let and_row = size.div_ceil(32) * 4;
                let mut img = Vec::new();
                // BITMAPINFOHEADER; a altura é dobrada (cor + máscara), como o formato .ico exige.
                img.extend(40u32.to_le_bytes());
                img.extend((size as i32).to_le_bytes());
                img.extend((2 * size as i32).to_le_bytes());
                img.extend(1u16.to_le_bytes());
                img.extend(32u16.to_le_bytes());
                img.extend(0u32.to_le_bytes());
                img.extend((size * size * 4 + and_row * size).to_le_bytes());
                img.extend([0u8; 16]);
                // Cor (BGRA), de baixo para cima.
                for y in (0..size).rev() {
                    for x in 0..size {
                        let p = px[(y * size + x) as usize];
                        img.extend([p as u8, (p >> 8) as u8, (p >> 16) as u8, (p >> 24) as u8]);
                    }
                }
                // Máscara: bit 1 = transparente (para quem não entende o alfa).
                for y in (0..size).rev() {
                    let mut row = vec![0u8; and_row as usize];
                    for x in 0..size {
                        if px[(y * size + x) as usize] >> 24 == 0 {
                            row[(x / 8) as usize] |= 0x80 >> (x % 8);
                        }
                    }
                    img.extend(row);
                }
                img
            })
            .collect();

        let mut out = Vec::new();
        out.extend([0u16, 1, sizes.len() as u16].iter().flat_map(|v| v.to_le_bytes()));
        let mut offset = 6 + 16 * sizes.len() as u32;
        for (&size, img) in sizes.iter().zip(&images) {
            let dim = if size >= 256 { 0 } else { size as u8 }; // 0 = 256
            out.extend([dim, dim, 0, 0]);
            out.extend(1u16.to_le_bytes());
            out.extend(32u16.to_le_bytes());
            out.extend((img.len() as u32).to_le_bytes());
            out.extend(offset.to_le_bytes());
            offset += img.len() as u32;
        }
        for img in images {
            out.extend(img);
        }
        out
    }

    /// BMP de 24 bits (w × h) com a cor de cada pixel dada por `pixel(x, y)`.
    fn bmp24(w: u32, h: u32, pixel: impl Fn(u32, u32) -> Rgb) -> Vec<u8> {
        let row_bytes = (w * 3).div_ceil(4) * 4;
        let data_len = row_bytes * h;
        let mut bmp = Vec::with_capacity(54 + data_len as usize);
        bmp.extend_from_slice(b"BM");
        for v in [54 + data_len, 0, 54, 40] {
            bmp.extend_from_slice(&v.to_le_bytes());
        }
        bmp.extend_from_slice(&(w as i32).to_le_bytes());
        bmp.extend_from_slice(&(h as i32).to_le_bytes());
        bmp.extend_from_slice(&1u16.to_le_bytes());
        bmp.extend_from_slice(&24u16.to_le_bytes());
        for v in [0u32, data_len, 2835, 2835, 0, 0] {
            bmp.extend_from_slice(&v.to_le_bytes());
        }
        for y in (0..h).rev() {
            let start = bmp.len();
            for x in 0..w {
                let (r, g, b) = pixel(x, y);
                bmp.extend_from_slice(&[b, g, r]);
            }
            bmp.resize(start + row_bytes as usize, 0);
        }
        bmp
    }

    /// O ícone azul (`icon` px) centralizado num quadrado de `size` px, já pintado sobre um
    /// fundo sólido (a imagem do canto do instalador, que é sempre escuro).
    fn icon_on_background(size: u32, icon: u32, background: Rgb) -> Vec<u8> {
        let px = render(IconState::Normal, icon);
        let offset = (size - icon) / 2;
        bmp24(size, size, |x, y| {
            let (ix, iy) = (x.wrapping_sub(offset), y.wrapping_sub(offset));
            if ix >= icon || iy >= icon {
                return background;
            }
            let p = px[(iy * icon + ix) as usize];
            let a = (p >> 24) as f32 / 255.0;
            let mix = |bg: u8, fg: u32| (bg as f32 * (1.0 - a) + fg as f32 * a).round() as u8;
            (mix(background.0, p >> 16 & 255), mix(background.1, p >> 8 & 255), mix(background.2, p & 255))
        })
    }

    #[test]
    fn ico_tem_cabecalho_e_todos_os_tamanhos() {
        let ico = ico_file(&[16, 32, 256]);
        assert_eq!(&ico[..6], &[0, 0, 1, 0, 3, 0]);
        assert_eq!((ico[6], ico[6 + 16], ico[6 + 32]), (16, 32, 0)); // 0 = 256
        let first_offset = u32::from_le_bytes(ico[18..22].try_into().unwrap());
        assert_eq!(first_offset, 6 + 16 * 3);
    }

    /// Gera as imagens do app em `res/`: o ícone (`vholumin.ico`, usado no .exe e no
    /// instalador) e as imagens do canto do instalador.
    /// Rodar com: `cargo test exportar_imagens_do_app -- --ignored`
    #[test]
    #[ignore]
    fn exportar_imagens_do_app() {
        let res = concat!(env!("CARGO_MANIFEST_DIR"), "/res");
        std::fs::write(format!("{res}/vholumin.ico"), ico_file(&[16, 20, 24, 32, 40, 48, 64, 256])).unwrap();
        // Fundo do assistente escuro do Inno Setup (estilo "modern dark windows11").
        let wizard_dark = (0x2B, 0x2B, 0x2B);
        std::fs::write(format!("{res}/instalador-55.bmp"), icon_on_background(55, 48, wizard_dark)).unwrap();
        std::fs::write(format!("{res}/instalador-110.bmp"), icon_on_background(110, 96, wizard_dark)).unwrap();
    }

    /// Gera `target/icones-previa.bmp` para olhar os ícones ampliados.
    /// Rodar com: `cargo test exportar_previa -- --ignored`
    #[test]
    #[ignore]
    fn exportar_previa() {
        const ZOOM: u32 = 4;
        const CELL: u32 = 32 * ZOOM + 16;
        let sizes = [16u32, 20, 24, 32];
        let backgrounds: [Rgb; 2] = [(243, 243, 243), (32, 32, 32)];
        let (w, h) = (CELL * ALL.len() as u32, CELL * (sizes.len() * backgrounds.len()) as u32);
        let mut img = vec![(0u8, 0u8, 0u8); (w * h) as usize];

        for (bi, &bgc) in backgrounds.iter().enumerate() {
            for (si, &size) in sizes.iter().enumerate() {
                let row = (bi * sizes.len() + si) as u32;
                for (ci, &state) in ALL.iter().enumerate() {
                    let px = render(state, size);
                    for y in 0..CELL {
                        for x in 0..CELL {
                            let mut c = bgc;
                            let (ix, iy) = ((x as i32 - 8) / ZOOM as i32, (y as i32 - 8) / ZOOM as i32);
                            if x >= 8 && y >= 8 && (ix as u32) < size && (iy as u32) < size {
                                let p = px[(iy as u32 * size + ix as u32) as usize];
                                let a = (p >> 24) as f32 / 255.0;
                                let blend = |b: u8, f: u32| (b as f32 * (1.0 - a) + f as f32 * a).round() as u8;
                                c = (blend(bgc.0, p >> 16 & 255), blend(bgc.1, p >> 8 & 255), blend(bgc.2, p & 255));
                            }
                            img[((row * CELL + y) * w + ci as u32 * CELL + x) as usize] = c;
                        }
                    }
                }
            }
        }

        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/target/icones-previa.bmp");
        std::fs::write(path, bmp24(w, h, |x, y| img[(y * w + x) as usize])).unwrap();
    }
}
