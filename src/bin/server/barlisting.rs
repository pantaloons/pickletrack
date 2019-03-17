use std::fs::File;
use std::sync::RwLock;

use log::{error, info};
use rand::seq::SliceRandom;
use rand::{thread_rng, Rng};
use serde::Deserialize;

/// Maximum distance of a bar that will be suggested to the user based on their current location.
const MAXIMUM_DITANCE_MILES: f64 = 3.0;

/// Path to JSON file containing list of bars with reviews mentioning picklebacks.
const BARS_FILE_PATH: &str = "static/data/current.json";

#[derive(Deserialize)]
struct Bar {
    id: String,
    name: String,
    lat: f64,
    lng: f64,
    tips: Vec<String>,
}

/// A rough estimate for the "utility" score of a bar.
///
/// This is a linear scoring of the likelihood the user would want to choose this bar. If three
/// bars are available, with scores [1, 2, 3], the first bar would be picked 1 in 6 times.
fn utility_from_distance(distance_miles: f64) -> f64 {
    5000.0 / ((distance_miles.powf(4.0) * 40.0) + 0.96)
}

/// Compute the distance in miles between two (lat, lng) pairs.
fn distance_latlong(lat1: f64, lng1: f64, lat2: f64, lng2: f64) -> f64 {
    let d_lat: f64 = (lat2 - lat1).to_radians();
    let d_lon: f64 = (lng2 - lng1).to_radians();
    let a = (d_lat / 2.0).sin().powf(2.0)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (d_lon / 2.0).sin().powf(2.0);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    3959.0 * c
}

pub struct BarListing {
    bars: RwLock<Vec<Bar>>,
}

impl BarListing {
    /// Create a new directory of bars serving picklebacks.
    ///
    /// # Panics
    /// This panics if we can't load the initial listing from disk.
    pub fn new() -> Self {
        let mut f: File = File::open(BARS_FILE_PATH).unwrap();
        let bars: Vec<Bar> = serde_json::from_reader(&mut f).unwrap();
        Self {
            bars: RwLock::new(bars),
        }
    }

    /// Attempt to reload the directory of bars from disk.
    ///
    /// This can fail for various IO related reasons, including if the bar directory file is not
    /// present or is corrupt. In these cases, nothing is changed and we continue using the
    /// previously loaded listing.
    pub fn reload_bars(&self) {
        info!("Reloading bar listing");
        match File::open(BARS_FILE_PATH) {
            Ok(mut file) => match serde_json::from_reader(&mut file) {
                Ok(bars) => {
                    let mut listing = self.bars.write().unwrap();
                    *listing = bars;
                    info!("Successfully reloaded bar listing");
                }
                Err(err) => error!(
                    "Couldn't parse bar liting file {} {:?}",
                    BARS_FILE_PATH, err
                ),
            },
            Err(err) => error!(
                "Couldn't open bar listing file {} {:?}",
                BARS_FILE_PATH, err
            ),
        }
    }

    /// Given a location, locate a random bar nearby that serves picklebacks.
    ///
    /// The returned tuple has the form (ID, Name, Comment), where comment is a randomly selected
    /// comment for the bar mentioning picklebacks. This picks bars based on a crude weighting by
    /// distance, closer bars will be returned more often. If there are no bars nearby, None is
    /// returned.
    #[allow(clippy::blacklisted_name)]
    pub fn locate_pickleback(&self, lat: f64, lng: f64) -> Option<(String, String, String)> {
        let bars = self.bars.read().unwrap();
        let mut rng = thread_rng();

        let mut total_utility: f64 = 0f64;
        for bar in &*bars {
            let distance: f64 = distance_latlong(lat, lng, bar.lat, bar.lng);
            if distance > MAXIMUM_DITANCE_MILES {
                continue;
            }

            total_utility += utility_from_distance(distance);
        }

        if total_utility == 0.0 {
            return None;
        }

        let choice: f64 = rng.gen_range(0.0, total_utility);

        let mut sweep_utility: f64 = 0.0;
        for bar in &*bars {
            let distance: f64 = distance_latlong(lat, lng, bar.lat, bar.lng);
            if distance > MAXIMUM_DITANCE_MILES {
                continue;
            }

            let bar_utility = utility_from_distance(distance);
            if sweep_utility + bar_utility > choice {
                return Some((
                    bar.id.clone(),
                    bar.name.clone(),
                    bar.tips.choose(&mut rng).unwrap().clone(),
                ));
            }
            sweep_utility += bar_utility;
        }

        unreachable!();
    }
}
