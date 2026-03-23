// stintlab - Frontend application
// Loads WASM module, wires API calls, handles DOM events.
//
// Note: innerHTML usage below renders data from our own API only (trusted).
// No user-supplied content is injected.

const API_BASE = window.location.origin;

// ---------------------------------------------------------------------------
// WASM Module Loading
// ---------------------------------------------------------------------------

let wasm = null;

async function loadWasm() {
    if (typeof WebAssembly === 'undefined') {
        showWasmFallback();
        return false;
    }

    try {
        const module = await import('./pkg/stintlab_viz.js');
        await module.default();
        wasm = module;
        return true;
    } catch (err) {
        console.error('Failed to load WASM module:', err);
        showWasmFallback();
        return false;
    }
}

function showWasmFallback() {
    const el = document.getElementById('wasm-fallback');
    if (el) el.classList.remove('hidden');
}

// ---------------------------------------------------------------------------
// API Client
// ---------------------------------------------------------------------------

async function fetchJson(path) {
    const response = await fetch(`${API_BASE}${path}`);
    if (!response.ok) {
        const body = await response.text();
        throw new Error(`API error ${response.status}: ${body}`);
    }
    return response.json();
}

async function postJson(path, body) {
    const response = await fetch(`${API_BASE}${path}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
    });
    if (!response.ok) {
        const text = await response.text();
        throw new Error(`API error ${response.status}: ${text}`);
    }
    return response.json();
}

// ---------------------------------------------------------------------------
// Safe DOM helpers
// ---------------------------------------------------------------------------

function clearChildren(el) {
    while (el.firstChild) el.removeChild(el.firstChild);
}

function createTextEl(tag, text, className) {
    const el = document.createElement(tag);
    el.textContent = text;
    if (className) el.className = className;
    return el;
}

// ---------------------------------------------------------------------------
// Race List Page (index.html)
// ---------------------------------------------------------------------------

async function loadRaces(season) {
    const container = document.getElementById('race-list');
    if (!container) return;

    clearChildren(container);
    container.appendChild(createTextEl('p', 'Loading races...', 'loading'));

    try {
        const races = await fetchJson(`/api/races?season=${encodeURIComponent(season)}`);

        clearChildren(container);

        if (races.length === 0) {
            container.appendChild(createTextEl('p',
                'No races found for this season. Run stintlab-ingest first.', 'loading'));
            return;
        }

        for (const race of races) {
            const card = document.createElement('div');
            card.className = 'race-card';

            const nameEl = createTextEl('div', race.name, 'race-name');
            const metaEl = createTextEl('div',
                `Round ${race.round} \u00B7 ${race.date} \u00B7 ${race.laps_total} laps`,
                'race-meta');

            card.appendChild(nameEl);
            card.appendChild(metaEl);
            card.addEventListener('click', () => {
                window.location.href = `analysis.html?id=${encodeURIComponent(race.id)}`;
            });
            container.appendChild(card);
        }
    } catch (err) {
        clearChildren(container);
        container.appendChild(createTextEl('p', `Error: ${err.message}`, 'loading'));
    }
}

// ---------------------------------------------------------------------------
// Analysis Page (analysis.html)
// ---------------------------------------------------------------------------

let currentRace = null;
let currentStints = null;
let currentLaps = null;

async function loadAnalysis(raceId) {
    const titleEl = document.getElementById('race-title');
    if (!titleEl) return;

    try {
        const races = await fetchJson('/api/races');
        currentRace = races.find(r => r.id === raceId);
        if (currentRace) {
            titleEl.textContent =
                `${currentRace.name} - ${currentRace.season} Round ${currentRace.round}`;
        }

        const [stintsResp, lapsResp] = await Promise.all([
            fetchJson(`/api/races/${encodeURIComponent(raceId)}/stints`),
            fetchJson(`/api/races/${encodeURIComponent(raceId)}/laps`),
        ]);

        currentStints = stintsResp;
        currentLaps = lapsResp;

        populateDriverSelectors(Object.keys(lapsResp.drivers || {}));

        if (wasm) {
            initWasm();
            renderStrategyTimeline();
            renderLapChart();
        }
    } catch (err) {
        console.error('Failed to load analysis:', err);
        titleEl.textContent = `Error loading race: ${err.message}`;
    }
}

function initWasm() {
    if (!wasm) return;
    try {
        wasm.init({
            strategy: 'canvas-strategy',
            lap_chart: 'canvas-laps',
        });
    } catch (err) {
        console.error('WASM init failed:', err);
    }
}

function renderStrategyTimeline() {
    if (!wasm || !currentStints || !currentRace) return;

    const drivers = Object.entries(currentStints.drivers || {}).map(([driver, stints]) => ({
        driver,
        stints,
    }));

    const showLabels = document.getElementById('show-labels');
    const options = {
        highlight_driver: null,
        show_compound_labels: showLabels ? showLabels.checked : true,
    };

    try {
        wasm.render_strategy_timeline(drivers, currentRace.laps_total, options);
    } catch (err) {
        console.error('Strategy timeline render failed:', err);
    }
}

function renderLapChart() {
    if (!wasm || !currentLaps) return;

    const filterSelect = document.getElementById('driver-filter');
    const selectedDrivers = filterSelect
        ? Array.from(filterSelect.selectedOptions).map(o => o.value)
        : [];

    const driversToShow = selectedDrivers.length > 0
        ? selectedDrivers
        : Object.keys(currentLaps.drivers || {}).slice(0, 5);

    const driversData = driversToShow.map(driver => ({
        driver,
        laps: (currentLaps.drivers[driver] || []).map(l => ({
            lap_number: l.lap_number,
            lap_time_ms: l.lap_time_ms,
            pit_in: l.pit_in,
            pit_out: l.pit_out,
        })),
        color: null,
    }));

    const showPitMarkers = document.getElementById('show-pit-markers');
    const options = {
        show_pit_markers: showPitMarkers ? showPitMarkers.checked : true,
        y_min_ms: null,
        y_max_ms: null,
    };

    try {
        wasm.render_lap_chart(driversData, null, options);
    } catch (err) {
        console.error('Lap chart render failed:', err);
    }
}

function populateDriverSelectors(drivers) {
    const filterSelect = document.getElementById('driver-filter');
    const predSelect = document.getElementById('pred-driver');

    for (const el of [filterSelect, predSelect]) {
        if (!el) continue;
        clearChildren(el);
        for (const d of drivers) {
            const option = document.createElement('option');
            option.value = d;
            option.textContent = d;
            el.appendChild(option);
        }
    }
}

// ---------------------------------------------------------------------------
// Pit Window Prediction
// ---------------------------------------------------------------------------

async function predictPitWindow() {
    const resultEl = document.getElementById('prediction-result');
    if (!resultEl || !currentRace) return;

    const driver = document.getElementById('pred-driver')?.value;
    const currentLap = parseInt(document.getElementById('pred-lap')?.value || '10', 10);
    const compound = document.getElementById('pred-compound')?.value || 'Medium';
    const tireAge = parseInt(document.getElementById('pred-age')?.value || '10', 10);

    try {
        const result = await postJson('/api/predict/pit-window', {
            race_id: currentRace.id,
            driver,
            current_lap: currentLap,
            current_compound: compound,
            tire_age: tireAge,
        });

        clearChildren(resultEl);
        resultEl.classList.remove('hidden');

        resultEl.appendChild(createTextEl('div',
            `Optimal pit: Lap ${result.optimal_lap} \u2192 ${result.next_compound}`,
            'optimal'));
        resultEl.appendChild(createTextEl('div',
            `Window: Lap ${result.window_start} - ${result.window_end}`,
            'window'));
        resultEl.appendChild(createTextEl('div',
            `Confidence: ${(result.confidence * 100).toFixed(0)}% \u00B7 ` +
            `Predicted loss: ${result.predicted_time_loss_s.toFixed(1)}s`,
            'confidence'));
    } catch (err) {
        clearChildren(resultEl);
        resultEl.classList.remove('hidden');
        resultEl.appendChild(createTextEl('p', `Prediction error: ${err.message}`));
    }
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

function exportCanvas(vizType) {
    if (!wasm) return;
    try {
        const dataUrl = wasm.export_png(vizType);
        const link = document.createElement('a');
        link.download = `stintlab-${vizType}.png`;
        link.href = dataUrl;
        link.click();
    } catch (err) {
        console.error('Export failed:', err);
    }
}

// ---------------------------------------------------------------------------
// Event Wiring
// ---------------------------------------------------------------------------

document.addEventListener('DOMContentLoaded', async () => {
    const isAnalysis = window.location.pathname.includes('analysis');

    if (isAnalysis) {
        await loadWasm();
        const params = new URLSearchParams(window.location.search);
        const raceId = parseInt(params.get('id'), 10);
        if (raceId) {
            await loadAnalysis(raceId);
        }

        document.getElementById('show-labels')?.addEventListener('change', renderStrategyTimeline);
        document.getElementById('show-pit-markers')?.addEventListener('change', renderLapChart);
        document.getElementById('driver-filter')?.addEventListener('change', renderLapChart);
        document.getElementById('predict-btn')?.addEventListener('click', predictPitWindow);
        document.getElementById('export-strategy')?.addEventListener('click',
            () => exportCanvas('strategy'));
        document.getElementById('export-laps')?.addEventListener('click',
            () => exportCanvas('lap_chart'));
    } else {
        const seasonSelect = document.getElementById('season-select');
        if (seasonSelect) {
            await loadRaces(seasonSelect.value);
            seasonSelect.addEventListener('change', () => loadRaces(seasonSelect.value));
        }
    }
});
