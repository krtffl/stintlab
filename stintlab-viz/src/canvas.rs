use wasm_bindgen::JsCast;
use web_sys::CanvasRenderingContext2d;

use crate::colors::Color;

/// Thin abstraction over the Canvas2D rendering context.
///
/// Provides typed helpers for common drawing operations,
/// keeping the rendering code clean and readable.
pub struct Canvas {
    ctx: CanvasRenderingContext2d,
    pub width: f64,
    pub height: f64,
}

impl Canvas {
    /// Create a `Canvas` from an HTML canvas element ID.
    ///
    /// Returns `Err` if the element is not found or is not a canvas.
    pub fn from_id(id: &str) -> Result<Self, String> {
        let document = web_sys::window()
            .ok_or("no window")?
            .document()
            .ok_or("no document")?;

        let element = document
            .get_element_by_id(id)
            .ok_or_else(|| format!("element not found: {id}"))?;

        let canvas: web_sys::HtmlCanvasElement = element
            .dyn_into()
            .map_err(|_| format!("{id} is not a canvas element"))?;

        let ctx = canvas
            .get_context("2d")
            .map_err(|_| "failed to get 2d context".to_string())?
            .ok_or("no 2d context")?
            .dyn_into::<CanvasRenderingContext2d>()
            .map_err(|_| "context is not CanvasRenderingContext2d".to_string())?;

        let width = f64::from(canvas.width());
        let height = f64::from(canvas.height());

        Ok(Self { ctx, width, height })
    }

    /// Clear the entire canvas.
    pub fn clear(&self) {
        self.ctx.clear_rect(0.0, 0.0, self.width, self.height);
    }

    /// Fill the canvas with a solid color.
    pub fn fill_background(&self, color: &Color) {
        self.set_fill_color(color);
        self.ctx.fill_rect(0.0, 0.0, self.width, self.height);
    }

    /// Draw a filled rectangle.
    pub fn draw_rect(&self, x: f64, y: f64, w: f64, h: f64, color: &Color) {
        self.set_fill_color(color);
        self.ctx.fill_rect(x, y, w, h);
    }

    /// Draw a rectangle outline.
    pub fn draw_rect_outline(&self, x: f64, y: f64, w: f64, h: f64, color: &Color, line_width: f64) {
        self.set_stroke_color(color);
        self.ctx.set_line_width(line_width);
        self.ctx.stroke_rect(x, y, w, h);
    }

    /// Draw a line from (x1, y1) to (x2, y2).
    pub fn draw_line(&self, x1: f64, y1: f64, x2: f64, y2: f64, color: &Color, line_width: f64) {
        self.set_stroke_color(color);
        self.ctx.set_line_width(line_width);
        self.ctx.begin_path();
        self.ctx.move_to(x1, y1);
        self.ctx.line_to(x2, y2);
        self.ctx.stroke();
    }

    /// Draw text at (x, y).
    pub fn draw_text(&self, text: &str, x: f64, y: f64, color: &Color, font: &str) {
        self.set_fill_color(color);
        self.ctx.set_font(font);
        let _ = self.ctx.fill_text(text, x, y);
    }

    /// Draw text aligned to the right of x.
    pub fn draw_text_right(&self, text: &str, x: f64, y: f64, color: &Color, font: &str) {
        self.set_fill_color(color);
        self.ctx.set_font(font);
        self.ctx.set_text_align("right");
        let _ = self.ctx.fill_text(text, x, y);
        self.ctx.set_text_align("left");
    }

    /// Draw a filled circle.
    pub fn draw_circle(&self, cx: f64, cy: f64, radius: f64, color: &Color) {
        self.set_fill_color(color);
        self.ctx.begin_path();
        let _ = self.ctx.arc(
            cx,
            cy,
            radius,
            0.0,
            std::f64::consts::TAU,
        );
        self.ctx.fill();
    }

    /// Draw a polyline (series of connected line segments).
    pub fn draw_polyline(&self, points: &[(f64, f64)], color: &Color, line_width: f64) {
        if points.len() < 2 {
            return;
        }
        self.set_stroke_color(color);
        self.ctx.set_line_width(line_width);
        self.ctx.begin_path();
        self.ctx.move_to(points[0].0, points[0].1);
        for &(x, y) in &points[1..] {
            self.ctx.line_to(x, y);
        }
        self.ctx.stroke();
    }

    /// Export the canvas contents as a base64-encoded PNG data URL.
    pub fn to_data_url(&self) -> Result<String, String> {
        // We need to get back to the canvas element to call toDataURL
        let document = web_sys::window()
            .ok_or("no window")?
            .document()
            .ok_or("no document")?;

        // This is a workaround — in practice we'd store the canvas reference
        // For now, the caller should use the JS-side toDataURL
        Err("use JavaScript canvas.toDataURL() for export".to_string())
    }

    fn set_fill_color(&self, color: &Color) {
        self.ctx.set_fill_style_str(&color.to_css());
    }

    fn set_stroke_color(&self, color: &Color) {
        self.ctx.set_stroke_style_str(&color.to_css());
    }
}
