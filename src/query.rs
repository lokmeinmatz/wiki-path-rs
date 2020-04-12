use crate::WikiUrl;
use std::collections::{BinaryHeap, HashSet};
use std::cmp::Ordering;
use regex::Regex;
use reqwest::{StatusCode, Url};
use std::sync::atomic::AtomicBool;

struct QueueRequest(Vec<WikiUrl>);

impl QueueRequest {
    fn depth(&self) -> usize { self.0.len() }
}

impl Ord for QueueRequest {
    fn cmp(&self, other: &Self) -> Ordering {
        self.depth().cmp(&other.depth()).reverse()
    }
}

impl PartialOrd for QueueRequest {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for QueueRequest {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq(&other.0.as_slice())
    }
}

impl Eq for QueueRequest {}

const WIKI_BASE_URL: &str = "https://de.wikipedia.org/wiki/";
pub static STOP_QUERIES: AtomicBool = AtomicBool::new(false);

pub fn fetch_or_lookup(page: &WikiUrl) -> Result<Vec<WikiUrl>, &'static str> {
    STOP_QUERIES.store(false, std::sync::atomic::Ordering::Release);
    //log::info!("looking on {}", page.0);
    // first test if is in database
    if let Some(links) = crate::cache::fetch_from_cache(page) {
        //log::info!("Found in cache");
        return Ok(links);
    }

    // fetch from net
    let mut base = String::from(WIKI_BASE_URL);
    base.push_str(&page.0);
    let res: reqwest::blocking::Response = reqwest::blocking::get(&base).unwrap();
    if res.status() != StatusCode::OK {
        return Err("Bad status code");
    }
    let html: String = res.text().map_err(|_| "Failed to get text")?;
    let re = Regex::new(r#"<a href="/wiki/[\w\d_()]*""#).unwrap();

    let mut links = Vec::new();

    let mut allready_stored = HashSet::new();
    allready_stored.insert(page.0.as_str());

    for link in re.find_iter(&html) {
        let link_ref = link.as_str();
        let link_ref = &link_ref[15..(link_ref.len() - 1)];
        if allready_stored.contains(link_ref) { continue; }
        allready_stored.insert(link_ref);
        let owned = link_ref.to_string();
        links.push(WikiUrl(owned));
    }

    crate::cache::insert_into_cache(page, &links);

    Ok(links)
}

pub (crate) fn query(from: WikiUrl, to: WikiUrl, depth: u8) -> Result<(u64, Vec<WikiUrl>), String> {

    let mut pages_to_query: BinaryHeap<QueueRequest> = BinaryHeap::new();
    let mut page_counter = 0;
    pages_to_query.push(QueueRequest(vec![from]));

    let mut allready_visited = HashSet::new();

    while let Some(qreq) = pages_to_query.pop() {
        if qreq.depth() > depth as usize { continue };
        if STOP_QUERIES.compare_and_swap(true, false, std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        page_counter += 1;
        for link in fetch_or_lookup(qreq.0.last().unwrap())? {
            if allready_visited.contains(&link) { continue; }
            let mut d = qreq.0.clone();
            d.push(link.clone());
            if link == to {
                // found res
                log::info!(">>  Found target with {} indirections", qreq.depth());
                return Ok((page_counter, d));
            }
            allready_visited.insert(link);

            pages_to_query.push(QueueRequest(d));
        }
    }


    Err(format!("No path found ({} pages checked)", page_counter))
}


pub fn get_random_url() -> Result<WikiUrl, &'static str> {
    let client = reqwest::blocking::Client::new().head("https://de.wikipedia\
    .org/wiki/Spezial:Random#/random");
    let res: reqwest::blocking::Response = client.send().unwrap();
    if res.status() != StatusCode::OK {
        eprintln!("errorcode: {}", res.status());
        return Err("Bad status code");
    }
    let mut url: std::str::Split<char> = res.url().path_segments().unwrap();
    let end = url.next_back().unwrap();
    log::info!("Random addr: {:?}", end);
    Ok(WikiUrl(end.to_string()))
}
