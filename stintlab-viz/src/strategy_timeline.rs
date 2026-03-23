use serde::Deserialize;

use crate::canvas::Canvas;
use crate::colors::{self, compound_color, Color};
use stintlab_core::models::Compound;

/// Rendering options for the strategy timeline.
#[derive(Debug, Deserialize)]
pub struct TimelineOptions {
    pub highlight_driver: Option<String>,
    #[serde(default = "default_true")]
    pub show_compound_labels: bool,
}

fn default_true() -> bool {
    true
}

/// A driver's stint data for rendering.
#[derive(Debug, Deserialize)]
pub struct DriverStints {
    pub driver: String,
    pub stints: Vec<StintData>,
}

/// A single stint for rendering.
#[derive(Debug, Deserialize)]
pub struct StintData {
    pub compound: String,
    pub start_lap: u16,
    pub end_lap: u16,
    pub stint_number: u8,
}

/// Layout constants for the strategy timeline.
const MARGIN_LEFT: f64 = 60.0;
const MARGIN_RIGHT: f64 = 20.0;
const MARGIN_TOP: f64 = 40.0;
const MARGIN_BOTTOM: f64 = 30.0;
const ROW_HEIGHT: f64 = 24.0;
const ROW_GAP: f64 = 4.0;
const BAR_RADIUS: f64 = 3.0;

/// Render a strategy timeline showing stint bars for all drivers.
///
/// Each row represents a driver. Stint bars are colored by compound.
/// Pit stop boundaries are marked with vertical dividers.
pub fn render(
    canvas: &Canvas,
    drivers_data: &[DriverStints],
    laps_total: u16,
    options: &TimelineOptions,
) {
    canvas.fill_background(&colors::BACKGROUND);

    if drivers_data.is_empty() || laps_total == 0 {
        canvas.draw_text(
            "No stint data available",
            canvas.width / 2.0 - 80.0,
            canvas.height / 2.0,
            &colors::TEXT_SECONDARY,
            "14px monospace",
        );
        return;
    }

    let plot_width = canvas.width - MARGIN_LEFT - MARGIN_RIGHT;
    let lap_scale = plot_width / f64::from(laps_total);

    // Title
    canvas.draw_text(
        "Race Strategy",
        MARGIN_LEFT,
        24.0,
        &colors::TEXT_PRIMARY,
        "bold 16px monospace",
    );

    // Draw lap number axis
    let axis_y = MARGIN_TOP - 8.0;
    let tick_interval = if laps_total > 40 { 10 } else { 5 };
    for lap in (0..=laps_total).step_by(tick_interval as usize) {
        let x = MARGIN_LEFT + f64::from(lap) * lap_scale;
        canvas.draw_text(
            &lap.to_string(),
            x - 4.0,
            axis_y,
            &colors::TEXT_SECONDARY,
            "10px monospace",
        );
        // Vertical grid line
        let grid_bottom = MARGIN_TOP + (drivers_data.len() as f64) * (ROW_HEIGHT + ROW_GAP);
        canvas.draw_line(
            x,
            MARGIN_TOP,
            x,
            grid_bottom,
            &colors::GRID_LINE,
            0.5,
        );
    }

    // Draw each driver's stints
    for (i, driver) in drivers_data.iter().enumerate() {
        let y = MARGIN_TOP + (i as f64) * (ROW_HEIGHT + ROW_GAP);

        let is_highlighted = options
            .highlight_driver
            .as_ref()
            .is_none_or(|h| h == &driver.driver);

        let alpha = if is_highlighted { 1.0 } else { 0.3 };

        // Driver label
        let label_color = if is_highlighted {
            colors::TEXT_PRIMARY
        } else {
            colors::TEXT_SECONDARY
        };
        canvas.draw_text_right(
            &driver.driver,
            MARGIN_LEFT - 8.0,
            y + ROW_HEIGHT / 2.0 + 4.0,
            &label_color,
            "12px monospace",
        );

        // Draw stint bars
        for stint in &driver.stints {
            let compound = parse_compound(&stint.compound);
            let color = compound_color(compound);

            let x_start = MARGIN_LEFT + f64::from(stint.start_lap.saturating_sub(1)) * lap_scale;
            let bar_width = f64::from(stint.end_lap - stint.start_lap + 1) * lap_scale;

            // Draw bar with alpha
            let bar_color = if is_highlighted {
                color
            } else {
                Color::new(
                    ((f64::from(color.r) * alpha) as u8).max(30),
                    ((f64::from(color.g) * alpha) as u8).max(30),
                    ((f64::from(color.b) * alpha) as u8).max(30),
                )
            };
            canvas.draw_rect(x_start, y, bar_width, ROW_HEIGHT, &bar_color);

            // Compound label on the bar
            if options.show_compound_labels && bar_width > 30.0 {
                let label = compound_label(compound);
                let text_x = x_start + bar_width / 2.0 - 3.0;
                let text_color = match compound {
                    Compound::Hard => colors::BACKGROUND,
                    Compound::Medium => colors::BACKGROUND,
                    _ => colors::TEXT_PRIMARY,
                };
                canvas.draw_text(
                    label,
                    text_x,
                    y + ROW_HEIGHT / 2.0 + 4.0,
                    &text_color,
                    "bold 10px monospace",
                );
            }

            // Pit marker at the end of non-final stints
            if stint.end_lap < laps_total {
                let pit_x = x_start + bar_width;
                canvas.draw_line(
                    pit_x,
                    y,
                    pit_x,
                    y + ROW_HEIGHT,
                    &colors::PIT_MARKER,
                    2.0,
                );
            }
        }
    }
}

fn parse_compound(s: &str) -> Compound {
    s.parse().unwrap_or(Compound::Medium)
}

fn compound_label(compound: Compound) -> &'static str {
    match compound {
        Compound::Soft => "S",
        Compound::Medium => "M",
        Compound::Hard => "H",
        Compound::Intermediate => "I",
        Compound::Wet => "W",
    }
}
