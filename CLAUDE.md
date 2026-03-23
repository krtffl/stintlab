# stintlab

Data-driven F1 strategy analytics with WASM-rendered visualizations.
Fourth project (P4) in a 7-product portfolio. WASM learning vehicle and content marketing platform.

## Commands

| Command | Description |
|---------|-------------|
| `cargo build` | Build all workspace crates |
| `cargo test` | Run all tests |
| `cargo clippy --all-targets -- -D warnings` | Lint with pedantic warnings as errors |
| `cargo fmt --check` | Check formatting |
| `wasm-pack build stintlab-viz --target web --release` | Build WASM visualization crate |

## Architecture

```
stintlab/
├── stintlab-core/     # Library crate (domain types, degradation model, strategy engine)
│   └── src/
│       ├── lib.rs           # Public API surface
│       ├── models.rs        # Race, Lap, Stint, Compound, DegradationModel, PitWindowPrediction
│       ├── degradation.rs   # OLS linear regression, predict lap times
│       ├── strategy.rs      # Pit window predictor
│       └── error.rs         # StintlabError (thiserror)
├── stintlab-ingest/   # Data pipeline binary (OpenF1 → SQLite)
│   └── src/
│       ├── main.rs          # CLI entry point
│       ├── openf1.rs        # OpenF1 REST API client
│       ├── storage.rs       # SQLite reads/writes
│       └── normalize.rs     # Raw API data → domain types
├── stintlab-web/      # Web server binary (axum REST API + static files)
│   └── src/
│       ├── main.rs          # axum server setup
│       ├── api.rs           # REST API handlers
│       └── state.rs         # AppState with SQLite connection
├── stintlab-viz/      # WASM visualization library (excluded from workspace, built with wasm-pack)
│   └── src/
│       ├── lib.rs           # wasm_bindgen entry points
│       ├── canvas.rs        # Canvas2D abstraction
│       ├── strategy_timeline.rs
│       ├── lap_chart.rs
│       └── colors.rs        # F1 compound color palettes
└── web/               # Static frontend assets
    ├── index.html
    ├── analysis.html
    ├── style.css
    └── app.js
```

## Key Patterns

- **Error handling**: `thiserror` for stintlab-core errors, `anyhow` for binary errors
- **Logging**: `tracing` with structured fields (not `println!` or `log`)
- **Testing**: Unit tests in `#[cfg(test)]` modules, integration tests in `tests/`
- **No `unwrap()` or `expect()`** in stintlab-core — use `?` operator
- **Edition 2024** with `clippy::pedantic` as baseline
- **Conventional commits**: `feat:`, `fix:`, `refactor:`, `test:`, `docs:`, `ci:`

## Cross-references

- Product definition: /home/krtffl/Documents/product-portfolio/products/P4-stintlab.md
- Tech specification: /home/krtffl/Documents/product-portfolio/products/P4-stintlab-tech.md
- Portfolio plan: /home/krtffl/Documents/product-portfolio/04-execution-plan.md
