//! In-memory clipboard history of text and images, with pins. Never written to disk.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Cursor;

const CAP: usize = 8;
/// Pinned items survive new copies, so only a few can be pinned and the list always has room for fresh ones.
const MAX_PINNED: usize = 4;
/// A copied novel must not sit in RAM eight times over.
const MAX_CHARS: usize = 2000;
/// Images are kept as compressed PNG, so a screenshot costs hundreds of KB, not the 8 MB of its raw pixels.
/// All images together stay under this; the oldest go first.
const IMAGE_BUDGET: usize = 24 * 1024 * 1024;
/// A bigger picture than 4096 x 4096 is not worth keeping.
pub(crate) const MAX_PIXELS: u64 = 4096 * 4096;

#[derive(Clone)]
pub struct Pic {
    /// Hash of the PNG bytes: the same picture copied twice is one entry, and the key of its thumbnail texture.
    pub id: u64,
    pub w: u32,
    pub h: u32,
    png: Vec<u8>,
    /// Thumbnail size and RGBA pixels (at most 64 x 40).
    pub thumb: ([usize; 2], Vec<u8>),
}

impl Pic {
    pub fn kb(&self) -> usize {
        self.png.len().div_ceil(1024)
    }

    /// The full picture again, to put it back on the clipboard: (width, height, RGBA).
    pub fn to_rgba(&self) -> Option<(u32, u32, Vec<u8>)> {
        let img = image::load_from_memory_with_format(&self.png, image::ImageFormat::Png).ok()?.to_rgba8();
        Some((img.width(), img.height(), img.into_raw()))
    }
}

/// Builds an entry from the RGBA pixels of a clipboard image. None when the buffer does not match the size, or the
/// picture is empty, absurdly large or does not compress below the budget.
pub fn make_pic(w: usize, h: usize, rgba: Vec<u8>) -> Option<Pic> {
    if w == 0 || h == 0 || (w as u64) * (h as u64) > MAX_PIXELS || rgba.len() != w * h * 4 {
        return None;
    }
    let img = image::RgbaImage::from_raw(w as u32, h as u32, rgba)?;
    // fast deflate with the Sub filter: cheap enough to run while the toast shows, and smooth areas (gradients,
    // screenshots) shrink a lot, unlike with no filter at all
    let mut png = Vec::new();
    let enc =
        image::codecs::png::PngEncoder::new_with_quality(Cursor::new(&mut png), image::codecs::png::CompressionType::Fast, image::codecs::png::FilterType::Sub);
    img.write_with_encoder(enc).ok()?;
    if png.len() > IMAGE_BUDGET {
        return None;
    }
    let scale = (64.0 / w as f32).min(40.0 / h as f32).min(1.0);
    let (tw, th) = (((w as f32 * scale).round() as u32).max(1), ((h as f32 * scale).round() as u32).max(1));
    let thumb = image::imageops::thumbnail(&img, tw, th);
    let mut hasher = DefaultHasher::new();
    png.hash(&mut hasher);
    Some(Pic { id: hasher.finish(), w: w as u32, h: h as u32, png, thumb: ([tw as usize, th as usize], thumb.into_raw()) })
}

#[derive(Clone)]
pub enum Item {
    Text(String),
    Image(Pic),
}

impl Item {
    fn same(&self, other: &Item) -> bool {
        match (self, other) {
            (Item::Text(a), Item::Text(b)) => a == b,
            (Item::Image(a), Item::Image(b)) => a.id == b.id,
            _ => false,
        }
    }
}

pub struct Slot {
    pub item: Item,
    pub pinned: bool,
}

#[derive(Default)]
pub struct History(Vec<Slot>);

impl History {
    fn put(&mut self, item: Item) {
        // copying a pinned item again keeps its pin
        let pinned = self.0.iter().any(|s| s.pinned && s.item.same(&item));
        self.0.retain(|s| !s.item.same(&item));
        let newest = item.clone();
        self.0.insert(0, Slot { item, pinned });
        self.pinned_first();
        // trimmed oldest first; pinned items never go
        while self.0.len() > CAP {
            let Some(i) = self.0.iter().rposition(|s| !s.pinned) else { break };
            self.0.remove(i);
        }
        // keep the compressed pictures inside the budget, dropping the oldest first but never the newest
        let bytes = |h: &History| h.0.iter().map(|s| if let Item::Image(p) = &s.item { p.png.len() } else { 0 }).sum::<usize>();
        while bytes(self) > IMAGE_BUDGET {
            let Some(i) = self.0.iter().rposition(|s| !s.pinned && matches!(s.item, Item::Image(_)) && !s.item.same(&newest)) else { break };
            self.0.remove(i);
        }
    }

    /// Pinned items first, each group keeping its newest-first order.
    fn pinned_first(&mut self) {
        self.0.sort_by_key(|s| !s.pinned);
    }

    /// Newest first; copying the same text again moves it to the top instead of duplicating it.
    pub fn push_text(&mut self, text: &str) {
        let t: String = text.trim().chars().take(MAX_CHARS).collect();
        if !t.is_empty() {
            self.put(Item::Text(t));
        }
    }

    pub fn push_image(&mut self, pic: Pic) {
        self.put(Item::Image(pic));
    }

    pub fn items(&self) -> &[Slot] {
        &self.0
    }

    /// Pins or unpins entry `i`. False when it cannot be pinned: no such entry, or the pin limit is reached.
    pub fn toggle_pin(&mut self, i: usize) -> bool {
        let pinned_now = self.0.iter().filter(|s| s.pinned).count();
        let Some(s) = self.0.get_mut(i) else { return false };
        if !s.pinned && pinned_now >= MAX_PINNED {
            return false;
        }
        s.pinned = !s.pinned;
        self.pinned_first();
        true
    }

    pub fn remove(&mut self, i: usize) {
        if i < self.0.len() {
            self.0.remove(i);
        }
    }

    /// Clears everything except the pinned items.
    pub fn clear(&mut self) {
        self.0.retain(|s| s.pinned);
    }
}

/// One line of at most `max` characters for a list row.
pub fn one_line(s: &str, max: usize) -> String {
    crate::widgets::clean_text(&s.chars().take(max * 2).collect::<String>()).split_whitespace().collect::<Vec<_>>().join(" ").chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(h: &History) -> Vec<&str> {
        h.items().iter().filter_map(|s| if let Item::Text(t) = &s.item { Some(t.as_str()) } else { None }).collect()
    }

    /// A w x h picture where pixel (x, y) is (x, y, seed, 255).
    fn pixels(w: usize, h: usize, seed: u8) -> Vec<u8> {
        (0..w * h).flat_map(|i| [(i % w) as u8, (i / w) as u8, seed, 255]).collect()
    }

    #[test]
    fn dedupes_caps_and_trims_text() {
        let mut h = History::default();
        h.push_text("  a  ");
        h.push_text("b");
        h.push_text("a");
        assert_eq!(texts(&h), ["a", "b"]);
        h.push_text("   ");
        assert_eq!(h.items().len(), 2);
        for i in 0..20 {
            h.push_text(&i.to_string());
        }
        assert_eq!(h.items().len(), CAP);
        assert_eq!(texts(&h)[0], "19");
        h.push_text(&"x".repeat(MAX_CHARS * 2));
        assert_eq!(texts(&h)[0].chars().count(), MAX_CHARS);
        h.remove(0);
        h.remove(99);
        assert_eq!(h.items().len(), CAP - 1);
        h.clear();
        assert!(h.items().is_empty());
    }

    #[test]
    fn one_line_flattens_whitespace() {
        assert_eq!(one_line("a\n  b\tc", 10), "a b c");
        assert_eq!(one_line("abcdef", 3), "abc");
    }

    #[test]
    fn a_picture_survives_the_round_trip() {
        let p = make_pic(8, 6, pixels(8, 6, 9)).unwrap();
        assert_eq!((p.w, p.h), (8, 6));
        assert_eq!(p.thumb.0, [8, 6]); // small pictures are not scaled up
        assert_eq!(p.thumb.1.len(), 8 * 6 * 4);
        assert!(p.kb() >= 1);
        let (w, h, rgba) = p.to_rgba().unwrap();
        assert_eq!((w, h), (8, 6));
        assert_eq!(rgba, pixels(8, 6, 9));
    }

    #[test]
    fn smooth_pictures_compress_well() {
        let (w, h) = (400usize, 200usize);
        let raw: Vec<u8> = (0..w * h).flat_map(|i| [((i % w) * 255 / w) as u8, ((i / w) * 255 / h) as u8, 128, 255]).collect();
        let p = make_pic(w, h, raw.clone()).unwrap();
        assert!(p.png.len() * 4 < raw.len(), "{} bytes for {} raw", p.png.len(), raw.len());
    }

    #[test]
    fn thumbnails_keep_the_aspect_ratio_inside_64_by_40() {
        let wide = make_pic(640, 100, pixels(640, 100, 1)).unwrap();
        assert_eq!(wide.thumb.0, [64, 10]);
        let tall = make_pic(100, 400, pixels(100, 400, 1)).unwrap();
        assert_eq!(tall.thumb.0, [10, 40]);
    }

    /// Puts a stored picture back on the real clipboard the way clicking its row does, and reads it again.
    /// Run it by hand: `cargo test --bin camel clipboard_round_trip -- --ignored`
    #[test]
    #[ignore = "writes to the real clipboard"]
    fn clipboard_round_trip() {
        let p = make_pic(32, 16, pixels(32, 16, 7)).unwrap();
        let (w, h, rgba) = p.to_rgba().unwrap();
        let mut cb = arboard::Clipboard::new().unwrap();
        cb.set_image(arboard::ImageData { width: w as usize, height: h as usize, bytes: std::borrow::Cow::Owned(rgba) }).unwrap();
        let back = cb.get_image().unwrap();
        assert_eq!((back.width, back.height), (32, 16));
        assert_eq!(back.bytes.into_owned(), pixels(32, 16, 7));
    }

    #[test]
    fn rejects_bad_pictures() {
        assert!(make_pic(0, 5, vec![]).is_none());
        assert!(make_pic(4, 4, vec![0; 10]).is_none(), "buffer does not match the size");
        assert!(make_pic(5000, 5000, vec![]).is_none(), "too many pixels");
    }

    #[test]
    fn the_same_picture_is_one_entry_and_text_does_not_clash() {
        let mut h = History::default();
        h.push_image(make_pic(4, 4, pixels(4, 4, 1)).unwrap());
        h.push_text("note");
        h.push_image(make_pic(4, 4, pixels(4, 4, 1)).unwrap());
        h.push_image(make_pic(4, 4, pixels(4, 4, 2)).unwrap());
        assert_eq!(h.items().len(), 3);
        assert!(matches!(h.items()[0].item, Item::Image(_)));
        assert_eq!(texts(&h), ["note"]);
    }

    #[test]
    fn old_pictures_go_first_when_the_budget_is_exceeded() {
        let mut h = History::default();
        let fake = |id: u64, bytes: usize| Pic { id, w: 1, h: 1, png: vec![0; bytes], thumb: ([1, 1], vec![0; 4]) };
        h.push_image(fake(1, IMAGE_BUDGET / 2));
        h.push_text("keep me");
        h.push_image(fake(2, IMAGE_BUDGET / 2));
        h.push_image(fake(3, IMAGE_BUDGET / 2));
        let ids: Vec<u64> = h.items().iter().filter_map(|s| if let Item::Image(p) = &s.item { Some(p.id) } else { None }).collect();
        assert_eq!(ids, [3, 2], "the oldest picture was dropped");
        assert_eq!(texts(&h), ["keep me"], "text is never dropped for space");
    }

    #[test]
    fn pinned_items_stay_on_top_and_survive_new_copies_and_clear() {
        let mut h = History::default();
        for t in ["a", "b", "c"] {
            h.push_text(t);
        }
        assert!(h.toggle_pin(2)); // "a", the oldest
        assert_eq!(texts(&h), ["a", "c", "b"], "pinned first, the rest newest first");
        for i in 0..20 {
            h.push_text(&format!("n{i}"));
        }
        assert_eq!(h.items().len(), CAP);
        assert_eq!(texts(&h)[0], "a", "a flood of copies never pushes a pin out");
        h.push_text("a");
        assert!(h.items()[0].pinned, "copying a pinned item again keeps the pin");
        assert_eq!(texts(&h).iter().filter(|t| **t == "a").count(), 1);
        h.clear();
        assert_eq!(texts(&h), ["a"], "Clear keeps pins");
        assert!(h.toggle_pin(0), "unpin");
        assert!(!h.items()[0].pinned);
        h.remove(0);
        assert!(h.items().is_empty(), "explicit removal still works");
    }

    #[test]
    fn only_a_few_items_can_be_pinned() {
        let mut h = History::default();
        for i in 0..6 {
            h.push_text(&i.to_string());
        }
        for _ in 0..MAX_PINNED {
            assert!(h.toggle_pin(MAX_PINNED)); // the first unpinned one each time
        }
        assert!(!h.toggle_pin(MAX_PINNED), "the limit is reached");
        assert_eq!(h.items().iter().filter(|s| s.pinned).count(), MAX_PINNED);
        assert!(!h.toggle_pin(99), "no such entry");
    }
}
