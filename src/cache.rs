use std::sync::{Mutex, MutexGuard};
use std::sync::mpsc;
use std::collections::HashMap;
use crate::WikiUrl;
use std::hash::Hasher;
use rusqlite::Connection;
use once_cell::sync::Lazy;
use std::sync::mpsc::Sender;

static CACHE: Lazy<Mutex<DbCache>> = Lazy::new(|| {
    Mutex::new(DbCache::create())
});

pub enum DbOffloadCommand {
    WriteLinks{ from: WikiUrl, links: Vec<WikiUrl> }
}

pub fn init_offload_thread() -> Sender<DbOffloadCommand> {
    let (tx, rx) = mpsc::channel::<DbOffloadCommand>();

    std::thread::spawn(move || {
        while let Ok(command) = rx.recv() {
            match command {
                DbOffloadCommand::WriteLinks {from, links} => {
                    log::info!("Writing links to db..");
                    let mut cache = CACHE.lock().unwrap();
                    // insert into db
                    let transaction = cache.db.transaction().unwrap();
                    let mut stmt = transaction.prepare("INSERT INTO cached (page, link) VALUES (?, ?)").unwrap();

                    for link in links {
                        stmt.execute(&[&from.0, &link.0]).unwrap();
                    }

                    drop(stmt);

                    transaction.commit().unwrap();
                    log::info!("Finished writing links to db..");
                }
            }
        }

        log::error!("Db Offload Thread terminated");
    });

    tx
}


pub struct FnvHasher(u64);

impl Default for FnvHasher {
    #[inline]
    fn default() -> FnvHasher {
        FnvHasher(0xcbf29ce484222325)
    }
}

impl Hasher for FnvHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        let FnvHasher(mut hash) = *self;

        for byte in bytes.iter() {
            hash = hash ^ (*byte as u64);
            hash = hash.wrapping_mul(0x100000001b3);
        }

        *self = FnvHasher(hash);
    }
}

pub fn fetch_from_cache(page: &WikiUrl) -> Option<Vec<WikiUrl>> {
    let cache = CACHE.lock().unwrap();
    cache.fetch_from_cache(page)
}


pub fn insert_into_cache(page: &WikiUrl, links: &Vec<WikiUrl>) {

    let mut cache = CACHE.lock().unwrap();

    log::info!("Inserting {} links to {} into cache", links.len(), &page.0);
    cache.insert(page, links);
    log::info!("Inserted links");
}

pub fn clear_mem() {
    let mut cache = CACHE.lock().unwrap();
    cache.in_mem.clear();
}

pub fn clear_for_page(page: &WikiUrl) {
    let mut cache = CACHE.lock().unwrap();

    cache.in_mem.remove(page);

    cache.db.prepare("DELETE FROM cached WHERE page = ?")
        .unwrap().execute(&[&page.0]).unwrap();
}

struct DbCache {
    in_mem: HashMap<WikiUrl, Vec<WikiUrl>>,
    db: Connection,
    offload_sender: Sender<DbOffloadCommand>
}

impl DbCache {

    pub fn create() -> Self {
        let s = init_offload_thread();
        DbCache {
            in_mem: HashMap::new(),
            db: Connection::open("./page-cache.sqlite").unwrap(),
            offload_sender: s
        }
    }

    pub fn insert(&mut self, page: &WikiUrl, links: &Vec<WikiUrl>) {
        // insert into mem
        self.in_mem.insert(page.clone(), links.clone());

        // send to offloader thread
        self.offload_sender.send(DbOffloadCommand::WriteLinks {
            from: page.clone(),
            links: links.clone()
        }).unwrap();
    }

    pub fn fetch_from_cache(&self, page: &WikiUrl) -> Option<Vec<WikiUrl>> {
        // check memory cache
        if let Some(links) = self.in_mem.get(page) {
            return Some(links.to_vec())
        }

        // check database
        let mut stmt = self.db.prepare("SELECT link FROM cached WHERE page = ?").unwrap();
        let links: Vec<WikiUrl> = stmt.query_map(
            &[&page.0],
            |row| Ok(WikiUrl(row.get(0)?))).unwrap().filter_map(Result::ok).collect();
        if links.len() == 0 { None }
        else { Some(links) }

    }
}