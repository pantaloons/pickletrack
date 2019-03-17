//! A library for scraping bars that mention picklebacks from Foursquare.
//!
//! The library writes the result of a scrape to the file ~/static/data/%Y%m%d.json and then updates
//! the symlink ~/static/data/current.json to point to this new file. The web server will
//! periodically reload the list of bars from the symlinked JSON file.
extern crate chrono;
extern crate reqwest;
extern crate serde;
extern crate serde_derive;
extern crate serde_json;

use std::collections::HashSet;
use std::f64::consts::PI;
use std::fs::File;

use chrono::{Date, Utc};
use serde::{Deserialize, Serialize};

const TIP_SEARCH_PHRASES: &[&'static str] = &[
    "pickle back",
    "pickleback",
    "pickel back",
    "pickelback",
    "pickle-back",
    "pickle-back",
    "pickle shot",
    "pickel shot",
    "pickle-shot",
    "pickel-shot",
    "shot of pickle",
    "shot of pickel",
    "shot pickle",
    "shot pickel",
    "pickle juice",
    "pickel juice",
    "pickle-juice",
    "pickel-juice",
];

const MANHATTAN_BOUNDING_BOX_TOPLEFT: LatLong = LatLong {
    latitude: 40.934688,
    longitude: -74.061693,
};
const MANHATTAN_BOUNDING_BOX_HEIGHT_METERS: i32 = 48000;
const MANHATTAN_BOUNDING_BOX_WIDTH_METERS: i32 = 33000;

/// When querying the Foursquare API for places, this is the default bounding box search size we
/// restrict to. If there are too many results, the bounding box will be choppped in half repeatedly
/// until they are all found. Note that the API has a limit of 10 square kilometers per query, so we
/// sneak in a little under this.
const DEFAULT_SEARCH_SIZE_METERS: i32 = 3000;

/// Foursquare API ID for the "Bar" category.
const FOURSQUARE_BAR_CATEGORY_IDENTIFIER: &'static str = "4bf58dd8d48988d116941735";

/// Foursquare API version tested against. Format YYYYMMDD.
const FOURSQUARE_API_VERSION_TARGETED: &'static str = "20170911";

/// Foursquare maximum results returned per query.
const FOURSQUARE_MAX_VENUES_PER_QUERY: usize = 50;

#[derive(Debug, Clone)]
struct LatLong {
    latitude: f64,
    longitude: f64,
}

#[derive(Clone)]
struct BoundingBox {
    sw: LatLong,
    ne: LatLong,
}

#[derive(Deserialize, Debug)]
struct FoursquareBarLocation {
    lat: f64,
    lng: f64,

    state: Option<String>,
}

#[derive(Deserialize, Debug)]
struct FoursquareBar {
    id: String,
    name: String,
    location: FoursquareBarLocation,
}

#[derive(Serialize, Debug)]
pub struct Bar {
    id: String,
    name: String,
    lat: f64,
    lng: f64,
    tips: Vec<String>,
}

#[derive(Deserialize, Debug)]
struct FoursquareTip {
    text: String,
}

#[derive(Deserialize, Debug)]
struct FoursquareVenueQueryResponse {
    venues: Vec<FoursquareBar>,
}

#[derive(Deserialize, Debug)]
struct FoursquareTipsItems {
    items: Vec<FoursquareTip>,
}

#[derive(Deserialize, Debug)]
struct FoursquareTipsQueryResponse {
    tips: FoursquareTipsItems,
}

#[derive(Deserialize, Debug)]
struct FoursquareVenueQueryResult {
    response: FoursquareVenueQueryResponse,
}

#[derive(Deserialize, Debug)]
struct FoursquareTipsQueryResult {
    response: FoursquareTipsQueryResponse,
}

/// Given a source lat/long point, and distances in meters to travel from that point, produce a new
/// lat/long point at the resulting location. This is not hyper accurate, but good enough for our
/// purposes.
fn offset_latlong(source: &LatLong, dn: i32, de: i32) -> LatLong {
    let d_lat: f64 = dn as f64 / 6378137f64;
    let d_lon: f64 = de as f64 / (6378137f64 * (PI * source.latitude / 180.0f64).cos());

    LatLong {
        latitude: source.latitude + d_lat * (180.0f64 / PI),
        longitude: source.longitude + d_lon * (180.0f64 / PI),
    }
}

/// Given a bounding box, split it into four equally distributed sub quadrants.
///
/// This is used for fine grained search within the limits of the Foursquare API. Foursquare will
/// return at most 50 results for any given bounding box, so when we encounter a box that has 50
/// items, we subdivide it and keep trying until all results are known comprehensively.
fn split_to_quadrants(source: &BoundingBox) -> [BoundingBox; 4] {
    let midpoint_lat: f64 = (source.sw.latitude + source.ne.latitude) / 2.0f64;
    let midpoint_lon: f64 = (source.sw.longitude + source.ne.longitude) / 2.0f64;

    [
        // Top left
        BoundingBox {
            sw: LatLong {
                latitude: midpoint_lat,
                longitude: source.sw.longitude,
            },
            ne: LatLong {
                latitude: source.ne.latitude,
                longitude: midpoint_lon,
            },
        },
        // Top right
        BoundingBox {
            sw: LatLong {
                latitude: midpoint_lat,
                longitude: midpoint_lon,
            },
            ne: LatLong {
                latitude: source.ne.latitude,
                longitude: source.ne.longitude,
            },
        },
        // Bottom left
        BoundingBox {
            sw: LatLong {
                latitude: source.sw.latitude,
                longitude: source.sw.longitude,
            },
            ne: LatLong {
                latitude: midpoint_lat,
                longitude: midpoint_lon,
            },
        },
        // Bottom right
        BoundingBox {
            sw: LatLong {
                latitude: source.sw.latitude,
                longitude: midpoint_lon,
            },
            ne: LatLong {
                latitude: midpoint_lat,
                longitude: source.ne.longitude,
            },
        },
    ]
}

fn get_bars(client_id: &String, client_secret: &String) -> Vec<FoursquareBar> {
    // Subdivide the region bounding box into a collection of smaller grid squares. We will explore
    // these one by one to build the place database.
    let mut unexplored: Vec<BoundingBox> = Vec::new();
    for de in 0..MANHATTAN_BOUNDING_BOX_WIDTH_METERS / DEFAULT_SEARCH_SIZE_METERS {
        for dn in 0..MANHATTAN_BOUNDING_BOX_HEIGHT_METERS / DEFAULT_SEARCH_SIZE_METERS {
            // We push the edges of the sub boxes to overlap a little bit, to account for
            // potential GIS issues and missing places in the lat/long cracks.
            unexplored.push(BoundingBox {
                sw: offset_latlong(
                    &MANHATTAN_BOUNDING_BOX_TOPLEFT,
                    -((dn + 1) * DEFAULT_SEARCH_SIZE_METERS + 10),
                    de * DEFAULT_SEARCH_SIZE_METERS - 10,
                ),
                ne: offset_latlong(
                    &MANHATTAN_BOUNDING_BOX_TOPLEFT,
                    -(dn * DEFAULT_SEARCH_SIZE_METERS - 10),
                    (de + 1) * DEFAULT_SEARCH_SIZE_METERS + 10,
                ),
            });
        }
    }

    let total_large: usize = unexplored.len();
    let mut total_large_handled: usize = 0;
    let mut bars: Vec<FoursquareBar> = Vec::new();

    while !unexplored.is_empty() {
        let next: BoundingBox = unexplored.pop().unwrap();

        let uri = format!(
            "https://api.foursquare.com/v2/venues/search?\
             sw={},{}&\
             ne={},{}&\
             intent=browse&\
             categoryId={}&\
             client_id={}&\
             client_secret={}&\
             v={}&\
             m=foursquare&\
             limit={}",
            next.sw.latitude,
            next.sw.longitude,
            next.ne.latitude,
            next.ne.longitude,
            FOURSQUARE_BAR_CATEGORY_IDENTIFIER,
            client_id,
            client_secret,
            FOURSQUARE_API_VERSION_TARGETED,
            FOURSQUARE_MAX_VENUES_PER_QUERY
        );
        let mut response = reqwest::get(&uri).unwrap();
        assert!(response.status().is_success());
        let body = response.text().unwrap();

        let mut results: FoursquareVenueQueryResult = serde_json::from_str(&body).unwrap();

        if results.response.venues.len() == FOURSQUARE_MAX_VENUES_PER_QUERY {
            // We got 50 venue results, which is the maximum. This means there are more in this
            // geographic quadrant and we need to break it down further to retrieve them fully.
            unexplored.extend_from_slice(&split_to_quadrants(&next));
            continue;
        }

        bars.append(&mut results.response.venues);
        if unexplored.len() < total_large - total_large_handled {
            total_large_handled += 1;
            println!(
                "Processed {}/{} large quadrants. Found {} bars.",
                total_large_handled,
                total_large,
                bars.len()
            );
        }
    }

    bars
}

pub fn scrape_pickleback_bars(client_id: &String, client_secret: &String) -> Vec<Bar> {
    assert!(MANHATTAN_BOUNDING_BOX_WIDTH_METERS % DEFAULT_SEARCH_SIZE_METERS == 0);
    assert!(MANHATTAN_BOUNDING_BOX_HEIGHT_METERS % DEFAULT_SEARCH_SIZE_METERS == 0);

    let bars: Vec<FoursquareBar> = get_bars(client_id, client_secret);
    let mut pickle_bars: Vec<Bar> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();

    let bars_100: usize = bars.len() / 100;

    let mut processed = 0;
    for bar in bars {
        if processed % bars_100 == 0 {
            println!("Fetching details {}% complete.", processed / bars_100);
        }
        processed += 1;

        if visited.contains(&bar.id.clone()) {
            continue;
        }
        visited.insert(bar.id.clone());

        if bar.location.state.is_none() || bar.location.state.unwrap() != "NY" {
            continue;
        }

        let uri = format!(
            "https://api.foursquare.com/v2/venues/{}/tips?\
             limit=500&\
             client_id={}&\
             client_secret={}&\
             v={}",
            bar.id, client_id, client_secret, FOURSQUARE_API_VERSION_TARGETED
        );

        let body;
        loop {
            let mut response = reqwest::get(&uri).unwrap();
            if response.status().is_success() {
                body = response.text().unwrap();
                break;
            }

            // Wait ten minutes and try again.
            println!("Error fetching details. Waiting ten minutes.");
            ::std::thread::sleep(::std::time::Duration::from_secs(60 * 10));
        }

        let results: FoursquareTipsQueryResult = serde_json::from_str(&body).unwrap();
        let mut tips: Vec<String> = Vec::new();
        for tip in results.response.tips.items {
            for search_phrase in TIP_SEARCH_PHRASES {
                if tip.text.to_lowercase().contains(search_phrase) {
                    if !tips.contains(&tip.text) {
                        tips.push(tip.text.clone());
                    }
                }
            }
        }

        if tips.len() > 0 {
            pickle_bars.push(Bar {
                id: bar.id,
                name: bar.name,
                lat: bar.location.lat,
                lng: bar.location.lng,
                tips: tips,
            });
        }
    }

    pickle_bars
}

fn main() {
    let now: Date<Utc> = Utc::today();

	let client_id: String = ::std::env::var("CLIENT_ID").unwrap();
	let client_secret: String = ::std::env::var("CLIENT_SECRET").unwrap();

    let date_path = format!("static/data/{}.json", now.format("%Y%m%d"));
    let symlink_path = format!("static/data/current.json");

    let mut file = File::create(&date_path).unwrap();
    serde_json::to_writer_pretty(&mut file,
		&scrape_pickleback_bars(&client_id, &client_secret)).unwrap();
    drop(file);

    ::std::os::unix::fs::symlink(date_path, symlink_path).unwrap();
}
