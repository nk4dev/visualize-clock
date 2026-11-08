use crate::dsp::NUM_BARS;
use crate::theme::{Color, Theme};
use ab_glyph::{Font, FontArc, PxScale, ScaleFont};
use std::cell::RefCell;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VisualizerMode {
    Bars,
    Mirror,
    Waveform,
}

impl VisualizerMode {
    pub fn next(self) -> Self {
        match self {
            VisualizerMode::Bars => VisualizerMode::Mirror,
            VisualizerMode::Mirror => VisualizerMode::Waveform,
            VisualizerMode::Waveform => VisualizerMode::Bars,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            VisualizerMode::Bars => "BARS",
            VisualizerMode::Mirror => "MIRROR",
            VisualizerMode::Waveform => "WAVE",
        }
    }
}

#[derive(Clone)]
pub struct CachedGlyph {
    pub width: u32,
    pub height: u32,
    pub x_offset: i32,
    pub y_offset: i32,
    pub advance: f32,
    pub bitmap: Vec<u8>,
}

#[derive(Clone, Copy)]
enum GlyphScale {
    Large,
    Medium,
    Small,
}

impl GlyphScale {
    fn px_scale(&self) -> PxScale {
        match self {
            GlyphScale::Large => PxScale::from(38.0),
            GlyphScale::Medium => PxScale::from(17.0),
            GlyphScale::Small => PxScale::from(13.0),
        }
    }
}

pub struct FontRenderer {
    font: Option<FontArc>,
    cjk_font: Option<FontArc>,
    cache_large: RefCell<HashMap<char, CachedGlyph>>,
    cache_medium: RefCell<HashMap<char, CachedGlyph>>,
    cache_small: RefCell<HashMap<char, CachedGlyph>>,
}

impl FontRenderer {
    pub fn new() -> Self {
        let latin_candidates = [
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf",
            "/usr/share/fonts/truetype/ubuntu/UbuntuMono-B.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
            "/usr/share/fonts/truetype/ubuntu/Ubuntu-B.ttf",
            "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
        ];

        let cjk_candidates = [
            "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        ];

        let mut font = None;
        for path in &latin_candidates {
            if let Ok(data) = std::fs::read(path) {
                if let Ok(f) = FontArc::try_from_vec(data) {
                    font = Some(f);
                    break;
                }
            }
        }

        let mut cjk_font = None;
        for path in &cjk_candidates {
            if let Ok(data) = std::fs::read(path) {
                if let Ok(f) = FontArc::try_from_vec(data) {
                    cjk_font = Some(f);
                    break;
                }
            }
        }

        let renderer = Self {
            font,
            cjk_font,
            cache_large: RefCell::new(HashMap::new()),
            cache_medium: RefCell::new(HashMap::new()),
            cache_small: RefCell::new(HashMap::new()),
        };

        renderer.preload_glyphs();
        renderer
    }

    fn preload_glyphs(&self) {
        if self.font.is_none() && self.cjk_font.is_none() {
            return;
        }

        let chars_to_cache = "0123456789:.-/ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz[]()+| %▾▴♪";
        for c in chars_to_cache.chars() {
            let _ = self.get_glyph(c, GlyphScale::Large);
            let _ = self.get_glyph(c, GlyphScale::Medium);
            let _ = self.get_glyph(c, GlyphScale::Small);
        }
    }

    fn rasterize_char(&self, c: char, scale: PxScale) -> Option<CachedGlyph> {
        let primary = self.font.as_ref();
        let cjk = self.cjk_font.as_ref();

        // Choose font: if primary font has glyph (glyph_id.0 != 0), use it; otherwise fallback to CJK
        let font = if let Some(p) = primary {
            if p.glyph_id(c).0 != 0 {
                p
            } else if let Some(fb) = cjk {
                fb
            } else {
                p
            }
        } else if let Some(fb) = cjk {
            fb
        } else {
            return None;
        };

        let glyph_id = font.glyph_id(c);
        let advance = font.as_scaled(scale).h_advance(glyph_id);

        let glyph = glyph_id.with_scale_and_position(scale, ab_glyph::point(0.0, 0.0));
        if let Some(outlined) = font.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            let width = bounds.width().ceil() as u32;
            let height = bounds.height().ceil() as u32;
            let mut bitmap = vec![0u8; (width * height) as usize];

            outlined.draw(|x, y, coverage| {
                if x < width && y < height {
                    let idx = (y * width + x) as usize;
                    let alpha = (coverage * 255.0).clamp(0.0, 255.0) as u8;
                    bitmap[idx] = alpha;
                }
            });

            Some(CachedGlyph {
                width,
                height,
                x_offset: bounds.min.x.round() as i32,
                y_offset: bounds.min.y.round() as i32,
                advance,
                bitmap,
            })
        } else {
            // Space or non-outline glyph
            Some(CachedGlyph {
                width: 0,
                height: 0,
                x_offset: 0,
                y_offset: 0,
                advance,
                bitmap: Vec::new(),
            })
        }
    }

    fn get_glyph(&self, c: char, scale_type: GlyphScale) -> Option<CachedGlyph> {
        let cache_ref = match scale_type {
            GlyphScale::Large => &self.cache_large,
            GlyphScale::Medium => &self.cache_medium,
            GlyphScale::Small => &self.cache_small,
        };

        if let Some(g) = cache_ref.borrow().get(&c) {
            return Some(g.clone());
        }

        // On-demand rasterize & cache
        let g = self.rasterize_char(c, scale_type.px_scale())?;
        cache_ref.borrow_mut().insert(c, g.clone());
        Some(g)
    }

    pub fn draw_text_large(
        &self,
        buf: &mut [u32],
        w: u32,
        h: u32,
        x: i32,
        y: i32,
        text: &str,
        color: Color,
    ) {
        self.draw_text_generic(buf, w, h, x, y, text, color, GlyphScale::Large);
    }

    pub fn draw_text_medium(
        &self,
        buf: &mut [u32],
        w: u32,
        h: u32,
        x: i32,
        y: i32,
        text: &str,
        color: Color,
    ) {
        self.draw_text_generic(buf, w, h, x, y, text, color, GlyphScale::Medium);
    }

    pub fn draw_text_small(
        &self,
        buf: &mut [u32],
        w: u32,
        h: u32,
        x: i32,
        y: i32,
        text: &str,
        color: Color,
    ) {
        self.draw_text_generic(buf, w, h, x, y, text, color, GlyphScale::Small);
    }

    fn draw_text_generic(
        &self,
        buf: &mut [u32],
        w: u32,
        h: u32,
        mut cur_x: i32,
        base_y: i32,
        text: &str,
        color: Color,
        scale_type: GlyphScale,
    ) {
        for c in text.chars() {
            if let Some(glyph) = self.get_glyph(c, scale_type) {
                let gx = cur_x + glyph.x_offset;
                let gy = base_y + glyph.y_offset;

                for row in 0..glyph.height {
                    for col in 0..glyph.width {
                        let px = gx + col as i32;
                        let py = gy + row as i32;

                        if px >= 0 && px < w as i32 && py >= 0 && py < h as i32 {
                            let alpha_cov = glyph.bitmap[(row * glyph.width + col) as usize];
                            if alpha_cov > 0 {
                                let mut tint = color;
                                tint.a = ((color.a as u32 * alpha_cov as u32) / 255) as u8;
                                blend_pixel(buf, w, px as u32, py as u32, tint);
                            }
                        }
                    }
                }
                cur_x += glyph.advance.round() as i32;
            } else {
                cur_x += 12;
            }
        }
    }
}

pub fn blend_pixel(buf: &mut [u32], w: u32, x: u32, y: u32, c: Color) {
    if c.a == 0 {
        return;
    }
    let idx = (y * w + x) as usize;
    if idx >= buf.len() {
        return;
    }

    let existing = buf[idx];
    let ea = ((existing >> 24) & 0xff) as u32;
    let er = ((existing >> 16) & 0xff) as u32;
    let eg = ((existing >> 8) & 0xff) as u32;
    let eb = (existing & 0xff) as u32;

    let ca = c.a as u32;
    let cr = c.r as u32;
    let cg = c.g as u32;
    let cb = c.b as u32;

    let inv_a = 255 - ca;

    // Standard alpha compositing
    let out_a = ca + (ea * inv_a) / 255;
    if out_a == 0 {
        buf[idx] = 0;
        return;
    }

    let out_r = (cr * ca + er * inv_a) / 255;
    let out_g = (cg * ca + eg * inv_a) / 255;
    let out_b = (cb * ca + eb * inv_a) / 255;

    buf[idx] = (out_a << 24) | (out_r << 16) | (out_g << 8) | out_b;
}

pub fn draw_mini_visualizer(
    buf: &mut [u32],
    w: u32,
    _h: u32,
    left: u32,
    top: u32,
    width: u32,
    height: u32,
    bars: &[f32; NUM_BARS],
    theme: &Theme,
) {
    let count = 14u32;
    let gap = 2u32;
    if width <= (count - 1) * gap {
        return;
    }
    let bar_width = ((width - (count - 1) * gap) / count).max(2);
    let bottom_y = top + height;

    for i in 0..count {
        let bar_idx = (i * 2) as usize;
        let x = left + i * (bar_width + gap);
        let val = bars[bar_idx].clamp(0.0, 1.0);
        let bar_h = ((val * (height as f32)) as u32).min(height).max(1);
        let bar_top_y = bottom_y.saturating_sub(bar_h);

        for by in bar_top_y..bottom_y {
            let ratio = if height > 0 {
                (bottom_y - by) as f32 / height as f32
            } else {
                0.0
            };
            let color = theme.get_bar_color(bar_idx, NUM_BARS, ratio);
            for bx in x..(x + bar_width) {
                blend_pixel(buf, w, bx, by, color);
            }
        }
    }
}

pub fn draw_card_background(
    buf: &mut [u32],
    w: u32,
    h: u32,
    radius: f32,
    bg_color: Color,
    border_color: Color,
) {
    let border_thickness = 1.2f32;

    for y in 0..h {
        let yf = y as f32;
        let dy = if yf < radius {
            radius - yf
        } else if yf > (h as f32 - 1.0 - radius) {
            yf - (h as f32 - 1.0 - radius)
        } else {
            0.0
        };

        for x in 0..w {
            let xf = x as f32;
            let dx = if xf < radius {
                radius - xf
            } else if xf > (w as f32 - 1.0 - radius) {
                xf - (w as f32 - 1.0 - radius)
            } else {
                0.0
            };

            let dist = (dx * dx + dy * dy).sqrt();

            if dist <= radius {
                let border_dist = radius - dist;
                if border_dist < border_thickness {
                    // Border area with smooth anti-aliased edge
                    let edge_alpha = (radius - dist).clamp(0.0, 1.0);
                    let mut b_col = border_color;
                    b_col.a = (border_color.a as f32 * edge_alpha) as u8;
                    blend_pixel(buf, w, x, y, b_col);
                } else {
                    blend_pixel(buf, w, x, y, bg_color);
                }
            } else if dist < radius + 1.0 {
                // Anti-aliased outer edge
                let edge_alpha = (radius + 1.0 - dist).clamp(0.0, 1.0);
                let mut b_col = border_color;
                b_col.a = (border_color.a as f32 * edge_alpha * 0.5) as u8;
                blend_pixel(buf, w, x, y, b_col);
            }
        }
    }
}

pub fn draw_visualizer_bars(
    buf: &mut [u32],
    w: u32,
    _h: u32,
    left: u32,
    top: u32,
    width: u32,
    height: u32,
    bars: &[f32; NUM_BARS],
    peaks: &[f32; NUM_BARS],
    theme: &Theme,
) {
    let num_bars = NUM_BARS as u32;
    let gap = 3u32;
    let total_gaps = (num_bars - 1) * gap;
    if width <= total_gaps {
        return;
    }

    let bar_width = ((width - total_gaps) / num_bars).max(2);
    let effective_width = num_bars * bar_width + total_gaps;
    let start_x = left + (width.saturating_sub(effective_width)) / 2;

    for i in 0..NUM_BARS {
        let x = start_x + (i as u32) * (bar_width + gap);
        let val = bars[i].clamp(0.0, 1.0);
        let bar_h = ((val * (height as f32)) as u32).min(height);

        let bottom_y = top + height;
        let bar_top_y = bottom_y.saturating_sub(bar_h);

        // Draw bar column
        for by in bar_top_y..bottom_y {
            let ratio = if height > 0 {
                (bottom_y - by) as f32 / height as f32
            } else {
                0.0
            };
            let color = theme.get_bar_color(i, NUM_BARS, ratio);

            for bx in x..(x + bar_width) {
                blend_pixel(buf, w, bx, by, color);
            }
        }

        // Draw peak hold dot
        let peak_val = peaks[i].clamp(0.0, 1.0);
        let peak_h = (peak_val * (height as f32)) as u32;
        let peak_y = bottom_y.saturating_sub(peak_h).saturating_sub(3);

        if peak_y >= top && peak_y + 2 < bottom_y {
            for py in peak_y..(peak_y + 2) {
                for px in x..(x + bar_width) {
                    blend_pixel(buf, w, px, py, theme.peak);
                }
            }
        }
    }
}

pub fn draw_mirror_visualizer(
    buf: &mut [u32],
    w: u32,
    _h: u32,
    left: u32,
    top: u32,
    width: u32,
    height: u32,
    bars: &[f32; NUM_BARS],
    theme: &Theme,
) {
    let num_bars = NUM_BARS as u32;
    let gap = 3u32;
    let total_gaps = (num_bars - 1) * gap;
    let bar_width = ((width - total_gaps) / num_bars).max(2);
    let start_x = left + (width.saturating_sub(num_bars * bar_width + total_gaps)) / 2;

    let center_y = top + height / 2;
    let half_h = (height / 2) as f32;

    for i in 0..NUM_BARS {
        let x = start_x + (i as u32) * (bar_width + gap);
        let val = bars[i].clamp(0.0, 1.0);
        let bar_h = (val * half_h) as u32;

        let y1 = center_y.saturating_sub(bar_h);
        let y2 = (center_y + bar_h).min(top + height);

        for by in y1..=y2 {
            let ratio = (by as f32 - center_y as f32).abs() / half_h;
            let color = theme.get_bar_color(i, NUM_BARS, ratio);
            for bx in x..(x + bar_width) {
                blend_pixel(buf, w, bx, by, color);
            }
        }
    }
}

pub fn draw_waveform(
    buf: &mut [u32],
    w: u32,
    _h: u32,
    left: u32,
    top: u32,
    width: u32,
    height: u32,
    samples: &[f32],
    theme: &Theme,
) {
    if samples.is_empty() || width < 2 {
        return;
    }

    let center_y = (top + height / 2) as f32;
    let amp = height as f32 * 0.45;
    let step = (samples.len() as f32) / (width as f32);

    let mut prev_y = center_y;

    for x_offset in 0..width {
        let sample_idx = ((x_offset as f32) * step) as usize;
        let s = if sample_idx < samples.len() {
            samples[sample_idx]
        } else {
            0.0
        };

        let curr_y = (center_y - s * amp).clamp(top as f32, (top + height - 1) as f32);
        let px = left + x_offset;

        let y_min = prev_y.min(curr_y).floor() as u32;
        let y_max = prev_y.max(curr_y).ceil() as u32;

        for py in y_min..=y_max {
            let ratio = (py as f32 - center_y).abs() / amp;
            let c = theme.get_bar_color((x_offset % NUM_BARS as u32) as usize, NUM_BARS, ratio);
            blend_pixel(buf, w, px, py, c);
        }

        prev_y = curr_y;
    }
}
