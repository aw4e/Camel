//! Exchange rates from the European Central Bank through Frankfurter, which needs no account. They are fetched the first
//! time a money question is typed in Spotlight and kept for six hours. Nothing about you is sent, only the request.

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

const FRESH: Duration = Duration::from_secs(6 * 3600);
/// After a failed request, wait this long before asking again, so typing does not hammer a dead connection.
const RETRY: Duration = Duration::from_secs(30);

/// Rates per one euro, and the day the bank published them.
#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub per_eur: HashMap<String, f64>,
    pub date: String,
}

#[derive(Clone)]
pub enum State {
    Ready(Table),
    /// The first request is still running.
    Loading,
    Failed,
}

struct Cache {
    table: Option<(Instant, Table)>,
    failed_at: Option<Instant>,
}

static CACHE: Mutex<Cache> = Mutex::new(Cache { table: None, failed_at: None });
static FETCHING: AtomicBool = AtomicBool::new(false);
/// Goes up whenever the answer to `state` may have changed, so Spotlight knows to look again.
static VERSION: AtomicU32 = AtomicU32::new(0);

pub fn version() -> u32 {
    VERSION.load(Ordering::Relaxed)
}

/// The rates if they are known, starting a fetch in the background when they are missing or old.
pub fn state() -> State {
    let (table, stale, failed_recently) = {
        let c = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
        let table = c.table.as_ref().map(|(_, t)| t.clone());
        let stale = c.table.as_ref().is_none_or(|(at, _)| at.elapsed() > FRESH);
        (table, stale, c.failed_at.is_some_and(|f| f.elapsed() < RETRY))
    };
    if stale && !failed_recently && !FETCHING.swap(true, Ordering::AcqRel) {
        std::thread::spawn(|| {
            let result = fetch();
            let mut c = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
            match result {
                Ok(t) => (c.table, c.failed_at) = (Some((Instant::now(), t)), None),
                Err(_) => c.failed_at = Some(Instant::now()),
            }
            drop(c);
            FETCHING.store(false, Ordering::Release);
            VERSION.fetch_add(1, Ordering::Relaxed);
        });
    }
    match table {
        Some(t) => State::Ready(t),
        None if failed_recently => State::Failed,
        None => State::Loading,
    }
}

#[derive(Deserialize)]
struct Answer {
    date: String,
    rates: HashMap<String, f64>,
}

pub fn parse(json: &str) -> Option<Table> {
    let a: Answer = serde_json::from_str(json).ok()?;
    let mut per_eur = a.rates;
    per_eur.retain(|_, r| r.is_finite() && *r > 0.0);
    per_eur.insert("EUR".to_owned(), 1.0);
    (per_eur.len() > 5).then_some(Table { per_eur, date: a.date })
}

fn fetch() -> Result<Table, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(10))).build().into();
    let mut resp = agent
        .get("https://api.frankfurter.dev/v1/latest")
        .header("User-Agent", format!("Camel/{}", crate::update::VERSION))
        .call()
        .map_err(|e| e.to_string())?;
    parse(&resp.body_mut().read_to_string().map_err(|e| e.to_string())?).ok_or_else(|| "unreadable rates".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_banks_answer_and_adds_the_euro() {
        let json = r#"{"amount":1.0,"base":"EUR","date":"2026-11-11","rates":{"USD":1.1,"IDR":18000.0,"JPY":170.0,"GBP":0.85,"CHF":0.93,"AUD":1.6}}"#;
        let t = parse(json).unwrap();
        assert_eq!((t.date.as_str(), t.per_eur["USD"], t.per_eur["EUR"]), ("2026-11-11", 1.1, 1.0));
    }

    #[test]
    fn refuses_answers_that_cannot_be_rates() {
        assert!(parse("{}").is_none());
        assert!(parse("nope").is_none());
        assert!(parse(r#"{"date":"x","rates":{"USD":1.1}}"#).is_none(), "too few currencies to be the real thing");
        let bad = r#"{"date":"x","rates":{"USD":0,"IDR":-1,"JPY":1,"GBP":1,"CHF":1,"AUD":1,"CAD":1}}"#;
        assert!(parse(bad).is_some_and(|t| !t.per_eur.contains_key("USD") && !t.per_eur.contains_key("IDR")), "zero and negative rates are dropped");
    }

    /// Asks the real service. Run it by hand: `cargo test --bin camel gets_real_rates -- --ignored --nocapture`
    #[test]
    #[ignore = "needs the network"]
    fn gets_real_rates() {
        let t = fetch().unwrap();
        println!("{} currencies on {}", t.per_eur.len(), t.date);
        assert!(t.per_eur.contains_key("IDR") && t.per_eur.contains_key("USD"));
    }
}
