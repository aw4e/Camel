//! Money questions in Spotlight: "100 usd to idr", "$50 in rupiah", "rp 1.500.000 to usd", "2jt idr usd".
//! Parsing and formatting are pure and tested; the rates come from `services::rates`.

use crate::services::rates::{self, State, Table};

/// The currencies the rates cover, and the words people use for them. Anything else is not money, so no request is made.
const ALIASES: &[(&str, &str)] = &[
    ("rupiah", "IDR"),
    ("rp", "IDR"),
    ("dollar", "USD"),
    ("dollars", "USD"),
    ("dolar", "USD"),
    ("euro", "EUR"),
    ("euros", "EUR"),
    ("yen", "JPY"),
    ("pound", "GBP"),
    ("pounds", "GBP"),
    ("sterling", "GBP"),
    ("yuan", "CNY"),
    ("rmb", "CNY"),
    ("won", "KRW"),
    ("baht", "THB"),
    ("ringgit", "MYR"),
    ("peso", "PHP"),
    ("rupee", "INR"),
    ("franc", "CHF"),
];

const CODES: &[&str] = &[
    "AUD", "BRL", "CAD", "CHF", "CNY", "CZK", "DKK", "EUR", "GBP", "HKD", "HUF", "IDR", "ILS", "INR", "ISK", "JPY", "KRW", "MXN", "MYR", "NOK", "NZD", "PHP",
    "PLN", "RON", "SEK", "SGD", "THB", "TRY", "USD", "ZAR",
];

/// Amounts written with a word after them: "2jt" is two million, "50k" fifty thousand, "500rb" five hundred thousand.
const MULTIPLIERS: &[(&str, f64)] = &[("k", 1e3), ("rb", 1e3), ("ribu", 1e3), ("jt", 1e6), ("juta", 1e6)];

fn currency(word: &str) -> Option<&'static str> {
    let w = word.to_lowercase();
    ALIASES.iter().find(|(a, _)| *a == w).map(|(_, c)| *c).or_else(|| CODES.iter().find(|c| c.eq_ignore_ascii_case(&w)).copied())
}

/// "15.000", "1.5", "1,5", "1.234,56", "1,234.56", "1,500": a lone separator followed by exactly three digits is a
/// thousands separator, any other lone one is the decimal point, and when both appear the last one is the decimal point.
pub fn parse_amount(s: &str) -> Option<f64> {
    if s.is_empty() || !s.chars().all(|c| c.is_ascii_digit() || c == '.' || c == ',') || !s.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let seps: Vec<(usize, char)> = s.char_indices().filter(|(_, c)| *c == '.' || *c == ',').collect();
    let decimal_at = match seps.as_slice() {
        [] => None,
        [(i, _)] if s.len() - i - 1 == 3 => None,
        [(i, _)] => Some(*i),
        many if many.iter().any(|(_, c)| *c != many[0].1) => many.last().map(|(i, _)| *i),
        _ => None,
    };
    let mut clean = String::new();
    for (i, c) in s.char_indices() {
        match c {
            '.' | ',' if Some(i) == decimal_at => clean.push('.'),
            '.' | ',' => {}
            d => clean.push(d),
        }
    }
    clean.parse().ok().filter(|v: &f64| v.is_finite())
}

#[derive(Debug, PartialEq)]
pub struct Query {
    pub amount: f64,
    pub from: &'static str,
    pub to: Option<&'static str>,
}

/// Splits into numbers (with their separators) and words; symbols become the words they stand for.
fn tokens(q: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let flush = |cur: &mut String, out: &mut Vec<String>| {
        if !cur.is_empty() {
            out.push(std::mem::take(cur));
        }
    };
    for c in q.to_lowercase().chars() {
        let symbol = match c {
            '$' => Some("usd"),
            '€' => Some("eur"),
            '£' => Some("gbp"),
            '¥' => Some("jpy"),
            '₹' => Some("inr"),
            '₩' => Some("krw"),
            _ => None,
        };
        let kind = |ch: char| ch.is_ascii_digit() || ch == '.' || ch == ',';
        if let Some(word) = symbol {
            flush(&mut cur, &mut out);
            out.push(word.to_owned());
        } else if c.is_whitespace() || c == '>' || c == '-' {
            flush(&mut cur, &mut out);
        } else if cur.chars().next().is_some_and(|f| kind(f) != kind(c)) {
            flush(&mut cur, &mut out);
            cur.push(c);
        } else {
            cur.push(c);
        }
    }
    flush(&mut cur, &mut out);
    out
}

/// "100 usd to idr", "$100 in idr", "usd 100 idr", "2 jt rupiah usd", "100 usd" (to your own currency).
pub fn parse(query: &str) -> Option<Query> {
    let toks = tokens(query);
    let mut iter = toks.iter().map(String::as_str).peekable();
    let mut amount = None;
    let mut from = None;
    let mut to = None;
    let mut explicit_to = false;
    while let Some(t) = iter.next() {
        if let Some(v) = parse_amount(t).filter(|_| amount.is_none()) {
            let mult = iter.peek().and_then(|w| MULTIPLIERS.iter().find(|(m, _)| m == w)).map(|(_, f)| *f);
            if mult.is_some() {
                iter.next();
            }
            amount = Some(v * mult.unwrap_or(1.0));
        } else if matches!(t, "to" | "in" | "into" | "ke" | "jadi" | "as") {
            explicit_to = true;
        } else if let Some(c) = currency(t) {
            if from.is_none() && !explicit_to {
                from = Some(c);
            } else if to.is_none() {
                to = Some(c);
            } else {
                return None;
            }
        } else {
            return None;
        }
    }
    // "usd idr 100" and "100 usd idr" both mean from the first currency to the second
    let (amount, from) = (amount?, from?);
    (to != Some(from)).then_some(Query { amount, from, to })
}

pub fn convert(amount: f64, from: &str, to: &str, t: &Table) -> Option<f64> {
    Some(amount / t.per_eur.get(from)? * t.per_eur.get(to)?)
}

/// Groups digits with `thousands` and puts `decimal` before the fraction.
pub fn format_number(v: f64, decimals: usize, (decimal, thousands): (char, char)) -> String {
    let fixed = format!("{v:.decimals$}");
    let (int, frac) = fixed.split_once('.').unwrap_or((&fixed, ""));
    let (sign, digits) = int.strip_prefix('-').map_or(("", int), |d| ("-", d));
    let mut grouped = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            grouped.push(thousands);
        }
        grouped.push(ch);
    }
    if frac.is_empty() { format!("{sign}{grouped}") } else { format!("{sign}{grouped}{decimal}{frac}") }
}

/// Currencies that are quoted in whole units.
fn decimals(code: &str, v: f64) -> usize {
    match code {
        "IDR" | "JPY" | "KRW" | "HUF" | "ISK" => 0,
        _ if v.abs() < 1.0 => 4,
        _ => 2,
    }
}

fn money(v: f64, code: &str, style: (char, char)) -> String {
    format!("{} {code}", format_number(v, decimals(code, v), style))
}

/// (title, detail) for a money question; None for anything that is not one.
pub fn answer(query: &str) -> Option<(String, String)> {
    let q = parse(query)?;
    let style = crate::services::system::number_format();
    let to = q.to.or_else(|| crate::services::system::local_currency().and_then(|c| currency(&c)).filter(|c| *c != q.from)).unwrap_or(if q.from == "USD" {
        "EUR"
    } else {
        "USD"
    });
    match rates::state() {
        State::Ready(t) => {
            let result = convert(q.amount, q.from, to, &t)?;
            let one = convert(1.0, q.from, to, &t)?;
            Some((money(result, to, style), format!("{} · 1 {} = {} · ECB {}", money(q.amount, q.from, style), q.from, money(one, to, style), t.date)))
        }
        State::Loading => Some(("…".to_owned(), "Getting exchange rates".to_owned())),
        State::Failed => Some(("Offline".to_owned(), "Could not get exchange rates, try again in a moment".to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::time::Duration;

    fn table() -> Table {
        let per_eur = HashMap::from([("EUR".to_owned(), 1.0), ("USD".to_owned(), 1.25), ("IDR".to_owned(), 20_000.0), ("JPY".to_owned(), 150.0)]);
        Table { per_eur, date: "2026-11-11".to_owned() }
    }

    #[test]
    fn reads_amounts_written_either_way() {
        for (s, v) in [
            ("100", 100.0),
            ("15.000", 15000.0),
            ("1.500.000", 1_500_000.0),
            ("1,500", 1500.0),
            ("1.5", 1.5),
            ("1,5", 1.5),
            ("0,5", 0.5),
            ("1.234,56", 1234.56),
            ("1,234.56", 1234.56),
            ("2,50", 2.5),
        ] {
            assert_eq!(parse_amount(s), Some(v), "{s}");
        }
        for bad in ["", "abc", ".5", ",", "1..2x", "-5", "1e5"] {
            assert_eq!(parse_amount(bad), None, "{bad}");
        }
    }

    #[test]
    fn reads_the_forms_people_type() {
        let q = |s: &str| parse(s).map(|q| (q.amount, q.from, q.to));
        assert_eq!(q("100 usd to idr"), Some((100.0, "USD", Some("IDR"))));
        assert_eq!(q("$50 in rupiah"), Some((50.0, "USD", Some("IDR"))));
        assert_eq!(q("rp 1.500.000 to usd"), Some((1_500_000.0, "IDR", Some("USD"))));
        assert_eq!(q("Rp15.000 ke dolar"), Some((15_000.0, "IDR", Some("USD"))));
        assert_eq!(q("2jt idr usd"), Some((2_000_000.0, "IDR", Some("USD"))));
        assert_eq!(q("50k jpy to idr"), Some((50_000.0, "JPY", Some("IDR"))));
        assert_eq!(q("€20"), Some((20.0, "EUR", None)));
        assert_eq!(q("100 usd"), Some((100.0, "USD", None)));
        assert_eq!(q("usd 100 -> idr"), Some((100.0, "USD", Some("IDR"))));
    }

    #[test]
    fn leaves_everything_else_alone_so_no_request_is_made() {
        for q in ["5 km in miles", "100 f to c", "5 min", "2 kg", "hello", "100", "usd", "usd to idr", "100 usd to usd", "1 abc to idr", "2+3"] {
            assert_eq!(parse(q), None, "{q}");
        }
    }

    #[test]
    fn converts_through_the_euro() {
        let t = table();
        assert_eq!(convert(100.0, "USD", "IDR", &t), Some(1_600_000.0));
        assert_eq!(convert(2_000_000.0, "IDR", "USD", &t), Some(125.0));
        assert_eq!(convert(1.0, "EUR", "JPY", &t), Some(150.0));
        assert_eq!(convert(1.0, "USD", "CHF", &t), None, "a currency with no rate");
    }

    #[test]
    fn formats_in_the_users_style() {
        let id = (',', '.');
        let us = ('.', ',');
        assert_eq!(format_number(1_589_000.0, 0, id), "1.589.000");
        assert_eq!(format_number(1234.5, 2, id), "1.234,50");
        assert_eq!(format_number(1234.5, 2, us), "1,234.50");
        assert_eq!(format_number(-999.0, 0, us), "-999");
        assert_eq!(format_number(0.0063, 4, us), "0.0063");
        assert_eq!(money(1_600_000.0, "IDR", id), "1.600.000 IDR");
        assert_eq!(money(0.5, "EUR", us), "0.5000 EUR");
        assert_eq!(money(125.0, "USD", us), "125.00 USD");
    }

    /// Asks the real service through the whole path. Run it by hand:
    /// `cargo test --bin camel answers_with_real_rates -- --ignored --nocapture`
    #[test]
    #[ignore = "needs the network"]
    fn answers_with_real_rates() {
        let first = answer("100 usd to idr").unwrap();
        assert_eq!(first.0, "…", "the first answer waits for the rates");
        std::thread::sleep(Duration::from_secs(4));
        let (title, detail) = answer("100 usd to idr").unwrap();
        println!("{title} | {detail} | local {:?} {:?}", crate::services::system::local_currency(), crate::services::system::number_format());
        assert!(title.ends_with(" IDR") && detail.contains("ECB"), "{title} / {detail}");
        let (title, detail) = answer("rp 2jt").unwrap();
        println!("{title} | {detail}");
    }
}
