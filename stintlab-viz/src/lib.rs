mod canvas;
pub mod colors;
mod lap_chart;
mod strategy_timeline;

use wasm_bindgen::prelude::*;

use canvas::Canvas;
use lap_chart::{DriverLaps, LapChartOptions, ModelCurve};
use strategy_timeline::{DriverStints, TimelineOptions};

/// Global state holding canvas references.
static mut STRATEGY_CANVAS: Option<Canvas> = None;
static mut LAP_CHART_CANVAS: Option<Canvas> = None;

/// Initialize the WASM module. Must be called once before any rendering.
///
/// `canvas_ids` should be a JS object: `{ strategy: "canvas-id-1", lap_chart: "canvas-id-2" }`
#[wasm_bindgen]
pub fn init(canvas_ids: JsValue) -> Result<(), JsValue> {
    // Set up panic hook for better error messages in console
    console_error_panic_hook_set();

    let ids: CanvasIds = serde_wasm_bindgen::from_value(canvas_ids)
        .map_err(|e| JsValue::from_str(&format!("invalid canvas_ids: {e}")))?;

    unsafe {
        if let Some(ref id) = ids.strategy {
            STRATEGY_CANVAS = Some(Canvas::from_id(id).map_err(JsValue::from)?);
        }
        if let Some(ref id) = ids.lap_chart {
            LAP_CHART_CANVAS = Some(Canvas::from_id(id).map_err(JsValue::from)?);
        }
    }

    web_sys::console::log_1(&"stintlab-viz initialized".into());
    Ok(())
}

/// Render the strategy timeline visualization.
///
/// `stints_json`: stint data from `/api/races/:id/stints` response
/// `laps_total`: total laps in the race
/// `options`: rendering options `{ highlight_driver?: string, show_compound_labels: bool }`
#[wasm_bindgen]
pub fn render_strategy_timeline(
    stints_json: JsValue,
    laps_total: u16,
    options: JsValue,
) -> Result<(), JsValue> {
    let canvas = unsafe {
        STRATEGY_CANVAS
            .as_ref()
            .ok_or_else(|| JsValue::from_str("strategy canvas not initialized"))?
    };

    let drivers: Vec<DriverStints> = serde_wasm_bindgen::from_value(stints_json)
        .map_err(|e| JsValue::from_str(&format!("invalid stint data: {e}")))?;

    let opts: TimelineOptions = serde_wasm_bindgen::from_value(options)
        .unwrap_or(TimelineOptions {
            highlight_driver: None,
            show_compound_labels: true,
        });

    strategy_timeline::render(canvas, &drivers, laps_total, &opts);
    Ok(())
}

/// Render the lap time evolution chart.
///
/// `laps_json`: driver lap data
/// `model_curves`: optional degradation model curves to overlay
/// `options`: rendering options
#[wasm_bindgen]
pub fn render_lap_chart(
    laps_json: JsValue,
    model_curves: JsValue,
    options: JsValue,
) -> Result<(), JsValue> {
    let canvas = unsafe {
        LAP_CHART_CANVAS
            .as_ref()
            .ok_or_else(|| JsValue::from_str("lap chart canvas not initialized"))?
    };

    let drivers: Vec<DriverLaps> = serde_wasm_bindgen::from_value(laps_json)
        .map_err(|e| JsValue::from_str(&format!("invalid lap data: {e}")))?;

    let curves: Vec<ModelCurve> = if model_curves.is_null() || model_curves.is_undefined() {
        Vec::new()
    } else {
        serde_wasm_bindgen::from_value(model_curves)
            .map_err(|e| JsValue::from_str(&format!("invalid model curves: {e}")))?
    };

    let opts: LapChartOptions = serde_wasm_bindgen::from_value(options)
        .unwrap_or(LapChartOptions {
            show_pit_markers: true,
            y_min_ms: None,
            y_max_ms: None,
        });

    lap_chart::render(canvas, &drivers, &curves, &opts);
    Ok(())
}

/// Export a canvas to PNG. Returns base64-encoded data URL.
///
/// `viz_type`: "strategy" or "lap_chart"
#[wasm_bindgen]
pub fn export_png(viz_type: String) -> Result<String, JsValue> {
    let canvas_id = match viz_type.as_str() {
        "strategy" => "canvas-strategy",
        "lap_chart" => "canvas-laps",
        other => return Err(JsValue::from_str(&format!("unknown viz type: {other}"))),
    };

    // Use JavaScript to get data URL from the canvas element
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let element = document
        .get_element_by_id(canvas_id)
        .ok_or_else(|| JsValue::from_str(&format!("canvas not found: {canvas_id}")))?;
    let canvas: web_sys::HtmlCanvasElement = element
        .dyn_into()
        .map_err(|_| JsValue::from_str("not a canvas"))?;

    canvas
        .to_data_url_with_type("image/png")
        .map_err(|e| JsValue::from_str(&format!("export failed: {e:?}")))
}

/// Free all allocated resources. Call on page unload.
#[wasm_bindgen]
pub fn dispose() {
    unsafe {
        STRATEGY_CANVAS = None;
        LAP_CHART_CANVAS = None;
    }
    web_sys::console::log_1(&"stintlab-viz disposed".into());
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct CanvasIds {
    strategy: Option<String>,
    lap_chart: Option<String>,
}

/// Set up `console_error_panic_hook` for better WASM error messages.
fn console_error_panic_hook_set() {
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
}
