//! Imports every page file needs.

pub(crate) use crate::layout::*;
pub(crate) use crate::ui::{self, BLUE, DIM, GRAY, GREEN, ORANGE, RED, bold, fade, fill, font};
pub(crate) use crate::widgets::*;
pub(crate) use eframe::egui::{self, Align2, Color32, Key, Modifiers, Rect, Response, Sense, Stroke, StrokeKind, Vec2, pos2, vec2};
pub(crate) use std::time::{Duration, Instant};
