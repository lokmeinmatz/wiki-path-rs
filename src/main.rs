#![feature(proc_macro_hygiene, decl_macro)]
//#![deny(warnings)]

#[macro_use]
extern crate rocket;
use serde::Deserialize;
use rocket::http::RawStr;
use std::time::Duration;
use rocket::response::Responder;

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct WikiUrl(String);

mod query;
mod cache;

fn main() {
    use simplelog::*;
    TermLogger::init(LevelFilter::Info, Config::default(), TerminalMode::Stdout).unwrap();

    // load templates
    log::info!("Start loading templates");

    log::info!("Finished loading templates");

    std::thread::spawn(|| {
        const interval: f64 = 10.0;
        log::info!("Starting random page query thread with interval {}", interval);

        loop {
            std::thread::sleep(Duration::from_secs_f64(interval));
            match query::get_random_url().and_then(|url| {
                log::info!("[Random Page Indexer] Random indexing of {:?}", url);
                query::fetch_or_lookup(&url)
            }) {
                Ok(_) => {},
                Err(e) => log::warn!("[Random Page Indexer] {}", e)
            }
        }
    });

    rocket::ignite().mount("/", routes![
        query,
        index,
        clear_mem_cache,
        clear_cache_for_page,
        stop_query
    ]).launch();
}


impl<'v> rocket::request::FromFormValue<'v> for WikiUrl {
    type Error = String;

    fn from_form_value(form_value: &'v RawStr) -> Result<Self, Self::Error> {
        //let decoded = form_value.to_string();
        Ok(WikiUrl(form_value.to_string()))
    }
}

impl<'v> rocket::request::FromParam<'v> for WikiUrl {
    type Error = String;

    fn from_param(form_value: &'v RawStr) -> Result<Self, Self::Error> {
        //let decoded = form_value.to_string();
        Ok(WikiUrl(form_value.to_string()))
    }
}

#[get("/?<from>&<to>&<depth>")]
fn query(from: WikiUrl, to: WikiUrl, depth: Option<u8>) -> String {
    let depth = depth.unwrap_or(2);
    log::info!("From {:?} to {:?} with depth: {:?}", from, to, depth);

    match query::query(from, to, depth) {
        Ok((pages_checked, path)) => {
            format!("Found path with {} jumps: {:?} ({} pages checked)", path.len() - 1, path, pages_checked)
        },
        Err(e) => e
    }
}

use rocket::response::content::Html;
use std::sync::atomic::Ordering;

#[get("/", rank = 2)]
fn index() -> Html<String> {
    let file = std::fs::read_to_string("./page/index.html").unwrap();
    Html(file)
}


#[get("/clear_mem")]
fn clear_mem_cache() -> &'static str {
    cache::clear_mem();
    "In memory cache cleared"
}

#[get("/stop")]
fn stop_query() -> &'static str {
    query::STOP_QUERIES.store(true, Ordering::Relaxed);
    "stopped at max one running query"
}

#[get("/clear/<page>")]
fn clear_cache_for_page(page: WikiUrl) -> &'static str {
    cache::clear_for_page(&page);
    "requested cache cleared"
}

#[derive(Debug, Deserialize)]
pub struct QueryOptions {
    pub from: String,
    pub to: String,
    pub depth: Option<u8>,
}

