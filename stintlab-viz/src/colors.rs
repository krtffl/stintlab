use stintlab_core::models::Compound;

/// RGB color representation.
#[derive(Debug, Clone, Copy)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    #[must_use]
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Convert to a CSS-compatible `rgb(r, g, b)` string.
    #[must_use]
    pub fn to_css(&self) -> String {
        format!("rgb({}, {}, {})", self.r, self.g, self.b)
    }

    /// Convert to a CSS-compatible rgba string with alpha.
    #[must_use]
    pub fn to_css_alpha(&self, alpha: f64) -> String {
        format!("rgba({}, {}, {}, {alpha})", self.r, self.g, self.b)
    }
}

// F1 compound colors (official-ish palette)
pub const SOFT: Color = Color::new(0xFF, 0x33, 0x33);
pub const MEDIUM: Color = Color::new(0xFF, 0xC9, 0x33);
pub const HARD: Color = Color::new(0xFF, 0xFF, 0xFF);
pub const INTERMEDIATE: Color = Color::new(0x33, 0xCC, 0x33);
pub const WET: Color = Color::new(0x33, 0x66, 0xFF);

// UI colors (dark theme)
pub const BACKGROUND: Color = Color::new(0x1A, 0x1A, 0x2E);
pub const SURFACE: Color = Color::new(0x24, 0x24, 0x3E);
pub const TEXT_PRIMARY: Color = Color::new(0xE0, 0xE0, 0xE0);
pub const TEXT_SECONDARY: Color = Color::new(0x99, 0x99, 0xAA);
pub const GRID_LINE: Color = Color::new(0x33, 0x33, 0x4D);
pub const PIT_MARKER: Color = Color::new(0xFF, 0x66, 0x00);

/// Get the display color for a tire compound.
#[must_use]
pub fn compound_color(compound: Compound) -> Color {
    match compound {
        Compound::Soft => SOFT,
        Compound::Medium => MEDIUM,
        Compound::Hard => HARD,
        Compound::Intermediate => INTERMEDIATE,
        Compound::Wet => WET,
    }
}
