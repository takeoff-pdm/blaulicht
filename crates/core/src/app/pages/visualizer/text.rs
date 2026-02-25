use super::constants::{
    FONT_ATLAS_COLS, FONT_ATLAS_H, FONT_ATLAS_W, FONT_GLYPH_H, FONT_GLYPH_SPACING, FONT_GLYPH_W,
    FONT_LINE_GAP,
};

pub(super) fn build_font_atlas_rgba() -> Vec<u8> {
    let mut data = vec![0u8; FONT_ATLAS_W * FONT_ATLAS_H * 4];
    for glyph_index in 0..128 {
        let glyph = font8x8_glyph(glyph_index as u8);
        let col = glyph_index % FONT_ATLAS_COLS;
        let row = glyph_index / FONT_ATLAS_COLS;
        for y in 0..FONT_GLYPH_H {
            let row_bits = glyph[y];
            for x in 0..FONT_GLYPH_W {
                let on = (row_bits >> (7 - x)) & 1 == 1;
                let dst_x = col * FONT_GLYPH_W + x;
                let dst_y = row * FONT_GLYPH_H + y;
                let idx = (dst_y * FONT_ATLAS_W + dst_x) * 4;
                data[idx] = 255;
                data[idx + 1] = 255;
                data[idx + 2] = 255;
                data[idx + 3] = if on { 255 } else { 0 };
            }
        }
    }
    data
}

fn font8x8_glyph(code: u8) -> [u8; 8] {
    match code {
        b' ' => [0x00; 8],
        b':' => [0x00, 0x10, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00],
        b'?' => [0x38, 0x44, 0x04, 0x18, 0x10, 0x00, 0x10, 0x00],
        b'T' => [0x7c, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x00],
        b'a' => [0x00, 0x00, 0x38, 0x04, 0x3c, 0x44, 0x3c, 0x00],
        b'f' => [0x18, 0x24, 0x20, 0x70, 0x20, 0x20, 0x20, 0x00],
        b'k' => [0x40, 0x40, 0x48, 0x50, 0x60, 0x50, 0x48, 0x00],
        b'o' => [0x00, 0x00, 0x38, 0x44, 0x44, 0x44, 0x38, 0x00],
        b'0' => [0x38, 0x44, 0x44, 0x44, 0x44, 0x44, 0x38, 0x00],
        b'1' => [0x10, 0x30, 0x10, 0x10, 0x10, 0x10, 0x38, 0x00],
        b'2' => [0x38, 0x44, 0x04, 0x18, 0x20, 0x40, 0x7c, 0x00],
        b'3' => [0x78, 0x04, 0x04, 0x38, 0x04, 0x04, 0x78, 0x00],
        b'4' => [0x48, 0x48, 0x48, 0x7c, 0x08, 0x08, 0x08, 0x00],
        b'5' => [0x7c, 0x40, 0x40, 0x78, 0x04, 0x04, 0x78, 0x00],
        b'6' => [0x38, 0x40, 0x40, 0x78, 0x44, 0x44, 0x38, 0x00],
        b'7' => [0x7c, 0x04, 0x08, 0x10, 0x20, 0x20, 0x20, 0x00],
        b'8' => [0x38, 0x44, 0x44, 0x38, 0x44, 0x44, 0x38, 0x00],
        b'9' => [0x38, 0x44, 0x44, 0x3c, 0x04, 0x04, 0x38, 0x00],
        b'F' => [0x7c, 0x40, 0x40, 0x78, 0x40, 0x40, 0x40, 0x00],
        b'e' => [0x00, 0x38, 0x44, 0x7c, 0x40, 0x38, 0x00, 0x00],
        b'i' => [0x00, 0x10, 0x00, 0x10, 0x10, 0x10, 0x10, 0x00],
        b'r' => [0x00, 0x58, 0x64, 0x40, 0x40, 0x40, 0x00, 0x00],
        b's' => [0x00, 0x3c, 0x40, 0x38, 0x04, 0x78, 0x00, 0x00],
        b't' => [0x10, 0x78, 0x10, 0x10, 0x10, 0x18, 0x00, 0x00],
        b'u' => [0x00, 0x44, 0x44, 0x44, 0x44, 0x3c, 0x00, 0x00],
        b'x' => [0x00, 0x44, 0x28, 0x10, 0x28, 0x44, 0x00, 0x00],
        _ => [0x00; 8],
    }
}

pub(super) fn build_text_vertices(text: &str, origin_x: f32, origin_y: f32, scale: f32) -> Vec<f32> {
    let mut verts = Vec::with_capacity(text.len() * 6 * 5);
    let mut cursor_x = origin_x;
    let mut cursor_y = origin_y;
    let cell_w = FONT_GLYPH_W as f32 * scale;
    let cell_h = FONT_GLYPH_H as f32 * scale;
    let advance = cell_w + FONT_GLYPH_SPACING * scale;
    for ch in text.chars() {
        if ch == '\n' {
            cursor_x = origin_x;
            cursor_y += cell_h + FONT_LINE_GAP * scale;
            continue;
        }
        let glyph_index = if ch as u32 >= 128 {
            '?' as usize
        } else {
            ch as usize
        };
        let col = glyph_index % FONT_ATLAS_COLS;
        let row = glyph_index / FONT_ATLAS_COLS;
        let u0 = col as f32 * FONT_GLYPH_W as f32 / FONT_ATLAS_W as f32;
        let u1 = (col + 1) as f32 * FONT_GLYPH_W as f32 / FONT_ATLAS_W as f32;
        let v0 = 1.0 - (row as f32 * FONT_GLYPH_H as f32) / FONT_ATLAS_H as f32;
        let v1 = 1.0 - ((row + 1) as f32 * FONT_GLYPH_H as f32) / FONT_ATLAS_H as f32;
        let x0 = cursor_x;
        let y0 = cursor_y;
        let x1 = cursor_x + cell_w;
        let y1 = cursor_y + cell_h;
        verts.extend_from_slice(&[
            x0, y0, 0.0, u0, v0, x1, y0, 0.0, u1, v0, x1, y1, 0.0, u1, v1, x0, y0, 0.0, u0,
            v0, x1, y1, 0.0, u1, v1, x0, y1, 0.0, u0, v1,
        ]);
        cursor_x += advance;
    }
    verts
}

pub(super) fn text_line_width(text: &str, scale: f32) -> f32 {
    let cell_w = FONT_GLYPH_W as f32 * scale;
    let mut max_width: f32 = 0.0;
    let mut line_width: f32 = 0.0;
    for ch in text.chars() {
        if ch == '\n' {
            max_width = max_width.max(line_width);
            line_width = 0.0;
            continue;
        }
        if line_width > 0.0 {
            line_width += FONT_GLYPH_SPACING * scale;
        }
        line_width += cell_w;
    }
    max_width.max(line_width)
}
