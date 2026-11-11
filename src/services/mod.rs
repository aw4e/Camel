// the first block of a driverless mixer; the mixer page does not use it yet
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) mod loopback;
pub(crate) mod lyrics;
pub(crate) mod media;
pub(crate) mod mixer;
pub(crate) mod monitor;
pub(crate) mod platform;
pub(crate) mod radios;
pub(crate) mod rates;
pub(crate) mod system;
pub(crate) mod update;
pub(crate) mod weather;
