//! One file per tab of the open notch. Each adds a `tab_*` method to `Notch` and draws in absolute coordinates from
//! the body origin `o`, fading with alpha `a`.

mod apps;
mod clipboard;
pub(crate) mod clock;
mod controls;
mod media;
mod mixer;
mod settings;
mod shelf;
mod tasks;
mod timer;
