#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ThemeMode {
    NeonCyberpunk,
    MatrixEmerald,
    SunsetGlow,
    IceMinimalist,
    Rainbow,
}

#[derive(Clone, Copy)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    #[allow(dead_code)]
    pub fn to_u32(self) -> u32 {
        ((self.a as u32) << 24) | ((self.r as u32) << 16) | ((self.g as u32) << 8) | (self.b as u32)
    }

    pub fn lerp(c1: Color, c2: Color, t: f32) -> Color {
        let t = t.clamp(0.0, 1.0);
        let r = (c1.r as f32 + (c2.r as f32 - c1.r as f32) * t) as u8;
        let g = (c1.g as f32 + (c2.g as f32 - c1.g as f32) * t) as u8;
        let b = (c1.b as f32 + (c2.b as f32 - c1.b as f32) * t) as u8;
        let a = (c1.a as f32 + (c2.a as f32 - c1.a as f32) * t) as u8;
        Color { r, g, b, a }
    }
}

pub struct Theme {
    pub mode: ThemeMode,
    pub name: &'static str,
    pub bg: Color,
    pub border: Color,
    pub time_text: Color,
    pub date_text: Color,
    pub bar_bottom: Color,
    pub bar_top: Color,
    pub peak: Color,
    pub accent: Color,
}

impl Theme {
    pub fn get(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::NeonCyberpunk => Theme {
                mode,
                name: "Cyberpunk",
                bg: Color::rgba(18, 20, 32, 215),
                border: Color::rgba(60, 75, 120, 140),
                time_text: Color::rgb(0, 240, 255),
                date_text: Color::rgba(160, 180, 220, 220),
                bar_bottom: Color::rgba(0, 240, 255, 230),
                bar_top: Color::rgba(255, 45, 140, 240),
                peak: Color::rgba(255, 255, 255, 255),
                accent: Color::rgb(255, 45, 140),
            },
            ThemeMode::MatrixEmerald => Theme {
                mode,
                name: "Matrix",
                bg: Color::rgba(12, 22, 16, 220),
                border: Color::rgba(35, 90, 50, 150),
                time_text: Color::rgb(45, 255, 135),
                date_text: Color::rgba(140, 225, 170, 220),
                bar_bottom: Color::rgba(20, 180, 90, 230),
                bar_top: Color::rgba(180, 255, 100, 240),
                peak: Color::rgba(255, 255, 255, 255),
                accent: Color::rgb(45, 255, 135),
            },
            ThemeMode::SunsetGlow => Theme {
                mode,
                name: "Sunset",
                bg: Color::rgba(26, 16, 24, 220),
                border: Color::rgba(110, 50, 70, 140),
                time_text: Color::rgb(255, 175, 60),
                date_text: Color::rgba(240, 180, 190, 220),
                bar_bottom: Color::rgba(255, 80, 50, 230),
                bar_top: Color::rgba(255, 215, 60, 240),
                peak: Color::rgba(255, 255, 240, 255),
                accent: Color::rgb(255, 80, 50),
            },
            ThemeMode::IceMinimalist => Theme {
                mode,
                name: "Ice Blue",
                bg: Color::rgba(20, 24, 30, 210),
                border: Color::rgba(80, 100, 130, 120),
                time_text: Color::rgb(240, 245, 255),
                date_text: Color::rgba(140, 170, 210, 220),
                bar_bottom: Color::rgba(70, 160, 245, 230),
                bar_top: Color::rgba(180, 225, 255, 240),
                peak: Color::rgba(255, 255, 255, 255),
                accent: Color::rgb(70, 160, 245),
            },
            ThemeMode::Rainbow => Theme {
                mode,
                name: "Rainbow",
                bg: Color::rgba(18, 18, 22, 220),
                border: Color::rgba(70, 70, 95, 140),
                time_text: Color::rgb(255, 255, 255),
                date_text: Color::rgba(200, 200, 220, 220),
                bar_bottom: Color::rgba(255, 60, 100, 230),
                bar_top: Color::rgba(100, 220, 255, 240),
                peak: Color::rgba(255, 255, 255, 255),
                accent: Color::rgb(255, 200, 80),
            },
        }
    }

    pub fn next(&self) -> Theme {
        let next_mode = match self.mode {
            ThemeMode::NeonCyberpunk => ThemeMode::MatrixEmerald,
            ThemeMode::MatrixEmerald => ThemeMode::SunsetGlow,
            ThemeMode::SunsetGlow => ThemeMode::IceMinimalist,
            ThemeMode::IceMinimalist => ThemeMode::Rainbow,
            ThemeMode::Rainbow => ThemeMode::NeonCyberpunk,
        };
        Theme::get(next_mode)
    }

    pub fn get_bar_color(&self, bar_idx: usize, total_bars: usize, height_ratio: f32) -> Color {
        if self.mode == ThemeMode::Rainbow {
            let hue = (bar_idx as f32 / total_bars as f32) * 320.0;
            let (r, g, b) = hsv_to_rgb(hue, 0.85, 0.95);
            let c_base = Color::rgba(r, g, b, 230);
            let c_light = Color::rgba(
                (r as f32 * 0.7 + 76.0) as u8,
                (g as f32 * 0.7 + 76.0) as u8,
                (b as f32 * 0.7 + 76.0) as u8,
                245,
            );
            Color::lerp(c_base, c_light, height_ratio)
        } else {
            Color::lerp(self.bar_bottom, self.bar_top, height_ratio)
        }
    }
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r1, g1, b1) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    (
        ((r1 + m) * 255.0) as u8,
        ((g1 + m) * 255.0) as u8,
        ((b1 + m) * 255.0) as u8,
    )
}
