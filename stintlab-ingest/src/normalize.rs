use std::collections::HashMap;

use chrono::NaiveDate;
use tracing::warn;

use stintlab_core::models::{Compound, Lap, Race, Stint};

use crate::openf1::{RawDriver, RawLap, RawSession, RawStint};

/// Build a driver number to 3-letter acronym lookup from raw driver data.
pub fn build_driver_map(drivers: &[RawDriver]) -> HashMap<u16, String> {
    let mut map = HashMap::new();
    for d in drivers {
        if let Some(ref acr) = d.name_acronym {
            map.insert(d.driver_number, acr.clone());
        }
    }
    map
}

/// Convert a raw `OpenF1` session into a domain Race.
///
/// Returns `None` if essential fields are missing.
pub fn session_to_race(session: &RawSession, round: u8) -> Option<Race> {
    let year = session.year?;
    let date_str = session.date_start.as_deref()?;

    // date_start is ISO 8601 datetime; we only need the date part
    let date = NaiveDate::parse_from_str(&date_str[..10], "%Y-%m-%d").ok()?;

    let circuit_key = session
        .circuit_short_name
        .as_deref()
        .unwrap_or("unknown")
        .to_lowercase()
        .replace(' ', "_");

    let name = session
        .country_name
        .clone()
        .unwrap_or_else(|| "Unknown GP".into());

    Some(Race {
        id: None,
        season: year,
        round,
        name: format!("{name} Grand Prix"),
        circuit_key,
        date,
        laps_total: 0, // Will be filled from lap data
    })
}

/// Convert raw `OpenF1` laps into domain Laps.
///
/// Requires a driver number to acronym map and a compound map from stints.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn normalize_laps(
    raw_laps: &[RawLap],
    race_id: i64,
    driver_map: &HashMap<u16, String>,
    stint_compounds: &HashMap<(u16, u16), (Compound, u16)>,
) -> Vec<Lap> {
    raw_laps
        .iter()
        .filter_map(|raw| {
            let driver = driver_map.get(&raw.driver_number)?.clone();
            let lap_number = raw.lap_number?;

            // Look up compound and tire age from stint data
            let (compound, tire_age) = stint_compounds
                .get(&(raw.driver_number, lap_number))
                .copied()
                .unwrap_or((Compound::Medium, 1));

            let to_ms = |d: f64| (d * 1000.0).round() as u32;
            let lap_time_ms = raw.lap_duration.map(to_ms);
            let sector1_ms = raw.duration_sector_1.map(to_ms);
            let sector2_ms = raw.duration_sector_2.map(to_ms);
            let sector3_ms = raw.duration_sector_3.map(to_ms);

            let pit_out = raw.is_pit_out_lap.unwrap_or(false);

            Some(Lap {
                race_id,
                driver,
                lap_number,
                lap_time_ms,
                sector1_ms,
                sector2_ms,
                sector3_ms,
                compound,
                tire_age,
                position: 0,   // OpenF1 laps endpoint doesn't provide position directly
                pit_in: false, // Will be derived from stint boundaries
                pit_out,
            })
        })
        .collect()
}

/// Build a lookup: `(driver_number, lap_number)` to `(Compound, tire_age)`.
///
/// Uses stint data to determine what compound and tire age each lap has.
pub fn build_stint_compound_map(raw_stints: &[RawStint]) -> HashMap<(u16, u16), (Compound, u16)> {
    let mut map = HashMap::new();

    for stint in raw_stints {
        let compound = stint
            .compound
            .as_deref()
            .and_then(parse_compound)
            .unwrap_or(Compound::Medium);

        let start = stint.lap_start.unwrap_or(1);
        let end = stint.lap_end.unwrap_or(start);
        let age_offset = stint.tyre_age_at_start.unwrap_or(0);

        for lap in start..=end {
            let tire_age = age_offset + (lap - start) + 1;
            map.insert((stint.driver_number, lap), (compound, tire_age));
        }
    }

    map
}

/// Convert raw `OpenF1` stints into domain Stints.
pub fn normalize_stints(
    raw_stints: &[RawStint],
    race_id: i64,
    driver_map: &HashMap<u16, String>,
) -> Vec<Stint> {
    raw_stints
        .iter()
        .filter_map(|raw| {
            let driver = driver_map.get(&raw.driver_number)?.clone();
            let stint_number = raw.stint_number?;
            let compound = raw
                .compound
                .as_deref()
                .and_then(parse_compound)
                .unwrap_or(Compound::Medium);

            Some(Stint {
                race_id,
                driver,
                stint_number,
                compound,
                start_lap: raw.lap_start.unwrap_or(1),
                end_lap: raw.lap_end.unwrap_or(1),
            })
        })
        .collect()
}

/// Parse an `OpenF1` compound string into our domain enum.
fn parse_compound(s: &str) -> Option<Compound> {
    match s.to_uppercase().as_str() {
        "SOFT" => Some(Compound::Soft),
        "MEDIUM" => Some(Compound::Medium),
        "HARD" => Some(Compound::Hard),
        "INTERMEDIATE" => Some(Compound::Intermediate),
        "WET" => Some(Compound::Wet),
        other => {
            warn!(compound = other, "unknown compound from OpenF1");
            None
        }
    }
}

/// Derive the total number of laps from lap data.
pub fn derive_laps_total(raw_laps: &[RawLap]) -> u16 {
    raw_laps
        .iter()
        .filter_map(|l| l.lap_number)
        .max()
        .unwrap_or(0)
}

/// Mark pit-in laps based on stint boundaries.
///
/// The last lap of each stint (except the final stint) is a pit-in lap.
pub fn mark_pit_in_laps(laps: &mut [Lap], stints: &[Stint]) {
    for stint in stints {
        // If this stint is not the last one for this driver, the end_lap is a pit-in lap
        let is_last = !stints
            .iter()
            .any(|s| s.driver == stint.driver && s.stint_number == stint.stint_number + 1);

        if !is_last {
            for lap in laps.iter_mut() {
                if lap.driver == stint.driver && lap.lap_number == stint.end_lap {
                    lap.pit_in = true;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_compound_variants() {
        assert_eq!(parse_compound("SOFT"), Some(Compound::Soft));
        assert_eq!(parse_compound("soft"), Some(Compound::Soft));
        assert_eq!(parse_compound("MEDIUM"), Some(Compound::Medium));
        assert_eq!(parse_compound("HARD"), Some(Compound::Hard));
        assert_eq!(parse_compound("INTERMEDIATE"), Some(Compound::Intermediate));
        assert_eq!(parse_compound("WET"), Some(Compound::Wet));
        assert_eq!(parse_compound("HYPERSOFT"), None);
    }

    #[test]
    fn build_driver_map_basic() {
        let drivers = vec![RawDriver {
            session_key: 1,
            driver_number: 1,
            broadcast_name: Some("M VERSTAPPEN".into()),
            name_acronym: Some("VER".into()),
            team_name: Some("Red Bull Racing".into()),
        }];
        let map = build_driver_map(&drivers);
        assert_eq!(map.get(&1), Some(&"VER".to_string()));
    }

    #[test]
    fn stint_compound_map_covers_range() {
        let stints = vec![RawStint {
            session_key: 1,
            driver_number: 1,
            stint_number: Some(1),
            compound: Some("MEDIUM".into()),
            tyre_age_at_start: Some(0),
            lap_start: Some(1),
            lap_end: Some(5),
        }];
        let map = build_stint_compound_map(&stints);
        for lap in 1..=5 {
            let (compound, age) = map[&(1, lap)];
            assert_eq!(compound, Compound::Medium);
            assert_eq!(age, lap);
        }
    }
}
