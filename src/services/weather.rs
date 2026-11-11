//! Temperature and humidity where you are. The place comes from Windows location (or a city you type), the weather from
//! Open-Meteo, which needs no account. Only rounded coordinates leave the PC, and only while the weather is switched on.

use serde::Deserialize;
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::time::Duration;
use windows::Devices::Geolocation::{Geolocator, PositionAccuracy};
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};

const REFRESH: Duration = Duration::from_secs(20 * 60);

#[derive(Clone, Debug, PartialEq)]
pub struct Weather {
    pub temp: f32,
    pub humidity: u8,
    pub code: u8,
    pub place: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Off,
    Loading,
    Ready(Weather),
    /// Windows gave no location and no city is set.
    NoLocation,
    Failed(String),
}

enum Msg {
    Enable(bool),
    City(String),
}

pub struct Service {
    shared: Arc<Mutex<Status>>,
    tx: mpsc::Sender<Msg>,
}

impl Service {
    pub fn spawn(enabled: bool, city: String) -> Self {
        let shared = Arc::new(Mutex::new(if enabled { Status::Loading } else { Status::Off }));
        let (tx, rx) = mpsc::channel();
        let state = shared.clone();
        std::thread::spawn(move || {
            unsafe { RoInitialize(RO_INIT_MULTITHREADED).ok() };
            let (mut on, mut city) = (enabled, city);
            let mut refresh = true;
            loop {
                if on && refresh {
                    let status = match locate(&city) {
                        None => Status::NoLocation,
                        Some(at) => fetch(at).map_or_else(Status::Failed, Status::Ready),
                    };
                    *state.lock().unwrap_or_else(PoisonError::into_inner) = status;
                }
                refresh = true;
                match rx.recv_timeout(REFRESH) {
                    Ok(Msg::Enable(v)) => {
                        on = v;
                        *state.lock().unwrap_or_else(PoisonError::into_inner) = if v { Status::Loading } else { Status::Off };
                    }
                    Ok(Msg::City(c)) => {
                        city = c;
                        *state.lock().unwrap_or_else(PoisonError::into_inner) = if on { Status::Loading } else { Status::Off };
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        });
        Self { shared, tx }
    }

    pub fn status(&self) -> Status {
        self.shared.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    pub fn set_enabled(&self, on: bool) {
        let _ = self.tx.send(Msg::Enable(on));
    }

    pub fn set_city(&self, city: &str) {
        let _ = self.tx.send(Msg::City(city.trim().to_owned()));
    }
}

/// Coordinates rounded to two decimals (about a kilometre): enough for the weather, too coarse to find a house.
fn round(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Latitude and longitude of the typed city, otherwise of the PC according to Windows.
fn locate(city: &str) -> Option<(f64, f64)> {
    if !city.is_empty() {
        return geocode(city);
    }
    let locator = Geolocator::new().ok()?;
    locator.SetDesiredAccuracy(PositionAccuracy::Default).ok()?;
    let p = locator.GetGeopositionAsync().ok()?.join().ok()?.Coordinate().ok()?.Point().ok()?.Position().ok()?;
    Some((round(p.Latitude), round(p.Longitude)))
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(10))).build().into()
}

#[derive(Deserialize)]
struct Geo {
    #[serde(default)]
    results: Vec<GeoHit>,
}

#[derive(Deserialize)]
struct GeoHit {
    latitude: f64,
    longitude: f64,
}

fn geocode(city: &str) -> Option<(f64, f64)> {
    let mut resp = agent()
        .get("https://geocoding-api.open-meteo.com/v1/search")
        .header("User-Agent", format!("Camel/{}", crate::update::VERSION))
        .query("name", city)
        .query("count", "1")
        .call()
        .ok()?;
    let hit = resp.body_mut().read_json::<Geo>().ok()?.results.into_iter().next()?;
    Some((round(hit.latitude), round(hit.longitude)))
}

fn fetch((lat, lon): (f64, f64)) -> Result<Weather, String> {
    let mut resp = agent()
        .get("https://api.open-meteo.com/v1/forecast")
        .header("User-Agent", format!("Camel/{}", crate::update::VERSION))
        .query("latitude", lat.to_string())
        .query("longitude", lon.to_string())
        .query("current", "temperature_2m,relative_humidity_2m,weather_code")
        .query("timezone", "auto")
        .call()
        .map_err(|e| format!("could not reach the weather service: {e}"))?;
    let body = resp.body_mut().read_to_string().map_err(|e| e.to_string())?;
    parse(&body).ok_or_else(|| "unreadable weather answer".to_owned())
}

#[derive(Deserialize)]
struct Forecast {
    timezone: Option<String>,
    current: Current,
}

#[derive(Deserialize)]
struct Current {
    temperature_2m: f32,
    relative_humidity_2m: f32,
    weather_code: u8,
}

pub fn parse(json: &str) -> Option<Weather> {
    let f: Forecast = serde_json::from_str(json).ok()?;
    let c = f.current;
    if !(-90.0..=70.0).contains(&c.temperature_2m) || !(0.0..=100.0).contains(&c.relative_humidity_2m) {
        return None;
    }
    Some(Weather {
        temp: c.temperature_2m,
        humidity: c.relative_humidity_2m.round() as u8,
        code: c.weather_code,
        place: f.timezone.as_deref().map(place_from_timezone).unwrap_or_default(),
    })
}

/// "Asia/Jakarta" is "Jakarta", "America/New_York" is "New York": the nearest thing to a place name in the answer.
pub fn place_from_timezone(zone: &str) -> String {
    zone.rsplit('/').next().unwrap_or(zone).replace('_', " ")
}

/// A short description of a WMO weather code.
pub fn describe(code: u8) -> &'static str {
    match code {
        0 => "Clear",
        1 => "Mostly clear",
        2 => "Partly cloudy",
        3 => "Overcast",
        45 | 48 => "Fog",
        51..=57 => "Drizzle",
        61..=67 => "Rain",
        71..=77 => "Snow",
        80..=82 => "Showers",
        85 | 86 => "Snow showers",
        95..=99 => "Thunderstorm",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_current_conditions() {
        let json = r#"{"latitude":-6.2,"longitude":106.8,"timezone":"Asia/Jakarta","current":{"time":"2026-11-11T19:30","temperature_2m":29.4,"relative_humidity_2m":74,"weather_code":3}}"#;
        let w = parse(json).unwrap();
        assert_eq!((w.temp, w.humidity, w.code, w.place.as_str()), (29.4, 74, 3, "Jakarta"));
        assert_eq!(describe(w.code), "Overcast");
    }

    #[test]
    fn rejects_answers_that_make_no_sense() {
        assert!(parse("{}").is_none());
        assert!(parse("not json").is_none());
        assert!(parse(r#"{"current":{"temperature_2m":400,"relative_humidity_2m":50,"weather_code":0}}"#).is_none());
        assert!(parse(r#"{"current":{"temperature_2m":20,"relative_humidity_2m":150,"weather_code":0}}"#).is_none());
        assert_eq!(parse(r#"{"current":{"temperature_2m":20,"relative_humidity_2m":50,"weather_code":0}}"#).unwrap().place, "");
    }

    #[test]
    fn names_places_and_codes() {
        assert_eq!(place_from_timezone("America/New_York"), "New York");
        assert_eq!(place_from_timezone("UTC"), "UTC");
        assert_eq!((describe(0), describe(63), describe(95), describe(200)), ("Clear", "Rain", "Thunderstorm", "Unknown"));
    }

    #[test]
    fn coordinates_are_rounded_to_about_a_kilometre() {
        assert_eq!(round(-6.200_049), -6.2);
        assert_eq!(round(106.845_67), 106.85);
    }

    /// Asks Windows for the location and prints only whether it worked, never the coordinates.
    /// Run it by hand: `cargo test --bin camel locates_this_pc -- --ignored --nocapture`
    #[test]
    #[ignore = "uses the Windows location service and the network"]
    fn locates_this_pc_and_gets_weather() {
        unsafe { RoInitialize(RO_INIT_MULTITHREADED).ok() };
        let at = locate("");
        println!("windows location: {}", if at.is_some() { "ok" } else { "none" });
        let at = at.or_else(|| locate("Jakarta")).expect("a place");
        let w = fetch(at).expect("weather");
        println!("weather ok: humidity {}%, {}, {}", w.humidity, describe(w.code), w.place);
    }
}
