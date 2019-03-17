//! Web server for Pickletrack.
//!
//! The server has two simple behaviors. It servers a couple of static pages, along with an API
//! endpoint to find a nearby bar given a customers location. The list of bars is loaded from disk,
//! and reloaded once a day. A separate process updates the list of bars. Note that this should be
//! done atomically (via a symbol link) to avoid partial read or write issues.
//!
//! The server server over HTTP on port 80. Because the web geolocation API requires HTTPS to run,
//! we place the server behind an SSL terminator on AWS. If the user attempts to load via HTTP, we
//! see this in the X-Forwarded-Proto header and redirect them to HTTPS.
mod barlisting;
use barlisting::BarListing;

use actix_web::fs::NamedFile;
use actix_web::http::header::LOCATION;
use actix_web::http::Method;
use actix_web::middleware::Started::{Done, Response};
use actix_web::middleware::{Logger, Middleware, Started};
use actix_web::{server, App, HttpRequest, HttpResponse, Json, Query, Result, State};
use serde::{Deserialize, Serialize};
use tokio::prelude::*;
use tokio::timer::Interval;

use std::sync::Arc;
use std::thread;
use std::time::Duration;

const INDEX_HTML_PATH: &str = "static/index.html";
const ABOUT_HTML_PATH: &str = "static/about.html";

/// This middleware rewrites all requests to be HTTPS and against "www" (AWS cannot terminate
/// SSL for apex domains due to DNS limitations).
struct AWSHTTPSWWWOnlyMiddleware;
impl<S> Middleware<S> for AWSHTTPSWWWOnlyMiddleware {
    fn start(&self, req: &HttpRequest<S>) -> Result<Started> {
        if !req.headers().contains_key("x-forwarded-proto") {
            // Not running behind AWS a HTTPS proxy. Just allow
            // everything without redirection.
            return Ok(Done);
        }

        let host: &str = req.headers().get("host").unwrap().to_str().unwrap();
        if req.headers().get("x-forwarded-proto").unwrap() == "https" && host.starts_with("www.") {
            return Ok(Done);
        }

        let mut www_uri: String = "https://".into();
        if !host.starts_with("www.") {
            www_uri.push_str("www.");
            www_uri.push_str(host);
        } else {
            www_uri.push_str(host);
            www_uri.push_str(req.uri().path_and_query().unwrap().as_str());
        }

        Ok(Response(
            HttpResponse::PermanentRedirect()
                .header(LOCATION, www_uri)
                .finish(),
        ))
    }
}

/// Request the index page.
fn index(_: &HttpRequest<Arc<BarListing>>) -> Result<NamedFile> {
    Ok(NamedFile::open(INDEX_HTML_PATH)?)
}

/// Request the about page.
fn about(_: &HttpRequest<Arc<BarListing>>) -> Result<NamedFile> {
    Ok(NamedFile::open(ABOUT_HTML_PATH)?)
}

#[derive(Serialize)]
struct LocateQueryResult {
    id: String,
    name: String,
    comment: String,
}

#[derive(Deserialize)]
struct LatLng {
    lat: f64,
    lng: f64,
}

fn locate(state: State<Arc<BarListing>>, latlng: Query<LatLng>) -> Json<LocateQueryResult> {
    if let Some((id, name, comment)) = state.locate_pickleback(latlng.lat, latlng.lng) {
        Json(LocateQueryResult {
            id,
            name,
            comment,
        })
    } else {
        Json(LocateQueryResult {
            id: "".into(),
            name: "".into(),
            comment: "".into(),
        })
    }
}

fn main() {
    env_logger::init();

    let state = Arc::new(BarListing::new());
    let cloned = state.clone();

    thread::spawn(move || {
        let task = Interval::new_interval(Duration::from_secs(60 * 60 * 24))
            .for_each(move |_| {
                cloned.reload_bars();
                Ok(())
            })
            .map_err(|e| panic!("{:?}", e));

        tokio::run(task);
    });

    server::new(move || {
        let cloned = state.clone();
        App::with_state(cloned)
            .middleware(AWSHTTPSWWWOnlyMiddleware)
            .middleware(Logger::default())
            .resource("/", |r| r.method(Method::GET).f(index))
            // Oops. We used to have a bad permanent redirect to // so we need to preserve this
            // for long enough until client caches expire.
            .resource("//", |r| r.method(Method::GET).f(index))
            .resource("/about", |r| r.method(Method::GET).f(about))
            .resource("/locate", |r| r.method(Method::GET).with(locate))
            .finish()
    })
    .bind("0.0.0.0:1025")
    .unwrap()
    .run();
}
