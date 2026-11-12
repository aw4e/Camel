//! Everything that touches Windows APIs or the network. Most services run a worker thread and expose a cheap
//! `state()`/`status()` snapshot the UI reads every frame.

pub(crate) mod loopback;
pub(crate) mod lyrics;
pub(crate) mod media;
pub(crate) mod mixer;
pub(crate) mod monitor;
pub(crate) mod platform;
pub(crate) mod radios;
pub(crate) mod rates;
pub(crate) mod router;
pub(crate) mod system;
pub(crate) mod update;
pub(crate) mod weather;
