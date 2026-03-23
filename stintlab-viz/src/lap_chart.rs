use serde::Deserialize;

use crate::canvas::Canvas;
use crate::colors::{self, Color};

/// Lap data for a single driver to render on the chart.
#[derive(Debug, Deserialize)]
pub struct DriverLaps {
    pub driver: String,
    pub laps: Vec<LapPoint>,
    pub color: Option<[u8; 3]>,
}

/// A single lap time data point.
#[derive(Debug, Deserialize)]
pub struct LapPoint {
    pub lap_number: u16,
    pub lap_time_ms: Option<u32>,
    pub pit_in: bool,
    pub pit_out: bool,
}

/// Degradation model curve to overlay on the chart.
#[derive(Debug, Deserialize)]
pub struct ModelCurve {
    pub label: String,
    pub points: Vec<(u16, u32)>,
    pub color: [u8; 3],
    pub dashed: bool,
}

/// Rendering options for the lap chart.
#[derive(Debug, Deserialize)]
pub struct LapChartOptions {
    #[serde(default = "default_true")]
    pub show_pit_markers: bool,
    #[serde(default)]
    pub y_min_ms: Option<u32>,
    #[serde(default)]
    pub y_max_ms: Option<u32>,
}

fn default_true() -> bool {
    true
}

/// Layout constants.
const MARGIN_LEFT: f64 = 70.0;
const MARGIN_RIGHT: f64 = 20.0;
const MARGIN_TOP: f64 = 40.0;
const MARGIN_BOTTOM: f64 = 40.0;

/// A set of distinguishable colors for drivers.
const DRIVER_COLORS: &[Color] = &[
    Color::new(0x00, 0xD2, 0xBE), // teal
    Color::new(0xFF, 0x87, 0x00), // orange
    Color::new(0x00, 0x90, 0xFF), // blue
    Color::new(0xDC, 0x00, 0x00), // red
    Color::new(0x27, 0xF4, 0xD2), // aqua
    Color::new(0xFF, 0x00, 0x87), // pink
    Color::new(0xB6, 0xBA, 0x00), // yellow-green
    Color::new(0x64, 0xC4, 0xFF), // light blue
    Color::new(0xFE, 0x6E, 0x38), // coral
    Color::new(0x9B, 0x00, 0x00), // dark red
];

/// Render a lap time evolution chart with optional degradation curves.
pub fn render(
    canvas: &Canvas,
    drivers_data: &[DriverLaps],
    model_curves: &[ModelCurve],
    options: &LapChartOptions,
) {
    canvas.fill_background(&colors::BACKGROUND);

    if drivers_data.is_empty() {
        canvas.draw_text(
            "No lap data available",
            canvas.width / 2.0 - 80.0,
            canvas.height / 2.0,
            &colors::TEXT_SECONDARY,
            "14px monospace",
        );
        return;
    }

    // Determine axis ranges
    let (x_min, x_max) = compute_x_range(drivers_data);
    let (y_min, y_max) = compute_y_range(drivers_data, options);

    if x_min >= x_max || y_min >= y_max {
        return;
    }

    let plot_width = canvas.width - MARGIN_LEFT - MARGIN_RIGHT;
    let plot_height = canvas.height - MARGIN_TOP - MARGIN_BOTTOM;

    // Title
    canvas.draw_text(
        "Lap Times",
        MARGIN_LEFT,
        24.0,
        &colors::TEXT_PRIMARY,
        "bold 16px monospace",
    );

    // Draw grid
    draw_grid(canvas, x_min, x_max, y_min, y_max, plot_width, plot_height);

    // Draw each driver's lap time line
    for (i, driver) in drivers_data.iter().enumerate() {
        let color = driver
            .color
            .map(|c| Color::new(c[0], c[1], c[2]))
            .unwrap_or_else(|| DRIVER_COLORS[i % DRIVER_COLORS.len()]);

        let points: Vec<(f64, f64)> = driver
            .laps
            .iter()
            .filter(|l| l.lap_time_ms.is_some() && !l.pit_in && !l.pit_out)
            .filter_map(|l| {
                let time = l.lap_time_ms?;
                if time < y_min || time > y_max {
                    return None;
                }
                let x = MARGIN_LEFT
                    + (f64::from(l.lap_number) - f64::from(x_min)) / f64::from(x_max - x_min)
                        * plot_width;
                let y = MARGIN_TOP + plot_height
                    - (f64::from(time) - f64::from(y_min)) / f64::from(y_max - y_min)
                        * plot_height;
                Some((x, y))
            })
            .collect();

        canvas.draw_polyline(&points, &color, 1.5);

        // Draw data point dots
        for &(x, y) in &points {
            canvas.draw_circle(x, y, 2.0, &color);
        }

        // Pit markers
        if options.show_pit_markers {
            for lap in &driver.laps {
                if lap.pit_in {
                    if let Some(time) = lap.lap_time_ms {
                        if time >= y_min && time <= y_max {
                            let x = MARGIN_LEFT
                                + (f64::from(lap.lap_number) - f64::from(x_min))
                                    / f64::from(x_max - x_min)
                                    * plot_width;
                            let y = MARGIN_TOP + plot_height
                                - (f64::from(time) - f64::from(y_min))
                                    / f64::from(y_max - y_min)
                                    * plot_height;
                            canvas.draw_circle(x, y, 5.0, &colors::PIT_MARKER);
                        }
                    }
                }
            }
        }

        // Driver label at end of line
        if let Some(&(x, y)) = points.last() {
            canvas.draw_text(
                &driver.driver,
                x + 5.0,
                y + 4.0,
                &color,
                "10px monospace",
            );
        }
    }

    // Draw degradation model curves
    for curve in model_curves {
        let color = Color::new(curve.color[0], curve.color[1], curve.color[2]);
        let points: Vec<(f64, f64)> = curve
            .points
            .iter()
            .filter_map(|&(lap, time)| {
                if time < y_min || time > y_max {
                    return None;
                }
                let x = MARGIN_LEFT
                    + (f64::from(lap) - f64::from(x_min)) / f64::from(x_max - x_min)
                        * plot_width;
                let y = MARGIN_TOP + plot_height
                    - (f64::from(time) - f64::from(y_min)) / f64::from(y_max - y_min)
                        * plot_height;
                Some((x, y))
            })
            .collect();

        // Dashed lines are drawn the same way for now (Canvas2D setLineDash
        // is not available through draw_polyline; could be added later)
        canvas.draw_polyline(&points, &color, 1.0);
    }
}

fn compute_x_range(drivers: &[DriverLaps]) -> (u16, u16) {
    let mut min = u16::MAX;
    let mut max = 0_u16;
    for d in drivers {
        for l in &d.laps {
            if l.lap_number < min {
                min = l.lap_number;
            }
            if l.lap_number > max {
                max = l.lap_number;
            }
        }
    }
    if min == u16::MAX {
        min = 0;
    }
    (min, max)
}

fn compute_y_range(drivers: &[DriverLaps], options: &LapChartOptions) -> (u32, u32) {
    if let (Some(min), Some(max)) = (options.y_min_ms, options.y_max_ms) {
        return (min, max);
    }

    let mut times: Vec<u32> = drivers
        .iter()
        .flat_map(|d| &d.laps)
        .filter(|l| !l.pit_in && !l.pit_out)
        .filter_map(|l| l.lap_time_ms)
        .collect();

    if times.is_empty() {
        return (80_000, 100_000);
    }

    times.sort_unstable();

    // Use P5 to P95 range to exclude outliers
    let p5 = times[times.len() * 5 / 100];
    let p95 = times[times.len() * 95 / 100];
    let margin = (p95 - p5) / 10;

    (
        options.y_min_ms.unwrap_or(p5.saturating_sub(margin)),
        options.y_max_ms.unwrap_or(p95 + margin),
    )
}

fn draw_grid(
    canvas: &Canvas,
    x_min: u16,
    x_max: u16,
    y_min: u32,
    y_max: u32,
    plot_width: f64,
    plot_height: f64,
) {
    // X axis ticks (laps)
    let x_range = x_max - x_min;
    let x_tick = if x_range > 40 { 10 } else { 5 };
    let first_x = (u32::from(x_min) / u32::from(x_tick) + 1) * u32::from(x_tick);
    let mut lap = first_x as u16;
    while lap <= x_max {
        let x = MARGIN_LEFT
            + (f64::from(lap) - f64::from(x_min)) / f64::from(x_range) * plot_width;
        canvas.draw_line(
            x,
            MARGIN_TOP,
            x,
            MARGIN_TOP + plot_height,
            &colors::GRID_LINE,
            0.5,
        );
        canvas.draw_text(
            &lap.to_string(),
            x - 6.0,
            MARGIN_TOP + plot_height + 16.0,
            &colors::TEXT_SECONDARY,
            "10px monospace",
        );
        lap += x_tick;
    }

    // X axis label
    canvas.draw_text(
        "Lap",
        MARGIN_LEFT + plot_width / 2.0 - 10.0,
        canvas.height - 4.0,
        &colors::TEXT_SECONDARY,
        "12px monospace",
    );

    // Y axis ticks (lap time in seconds)
    let y_range = y_max - y_min;
    let y_tick_ms = if y_range > 20_000 { 5000 } else if y_range > 10_000 { 2000 } else { 1000 };
    let first_y = (y_min / y_tick_ms + 1) * y_tick_ms;
    let mut time = first_y;
    while time <= y_max {
        let y = MARGIN_TOP + plot_height
            - (f64::from(time) - f64::from(y_min)) / f64::from(y_range) * plot_height;
        canvas.draw_line(
            MARGIN_LEFT,
            y,
            MARGIN_LEFT + plot_width,
            y,
            &colors::GRID_LINE,
            0.5,
        );
        let secs = f64::from(time) / 1000.0;
        canvas.draw_text_right(
            &format!("{secs:.1}s"),
            MARGIN_LEFT - 6.0,
            y + 4.0,
            &colors::TEXT_SECONDARY,
            "10px monospace",
        );
        time += y_tick_ms;
    }
}
