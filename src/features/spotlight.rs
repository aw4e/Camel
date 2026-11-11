//! Spotlight search: calculator, apps (Start Menu) and files (user folders).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{HSTRING, w};

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Calc,
    App,
    File,
    Web,
    /// A system action, timer or similar; `Hit::action` says which.
    Action,
}

#[derive(Clone)]
pub struct Hit {
    pub action: Option<crate::actions::Action>,
    pub kind: Kind,
    pub title: String,
    pub detail: String,
    pub path: PathBuf,
}

#[derive(Clone)]
struct Entry {
    lower: String,
    name: String,
    path: PathBuf,
    app: bool,
}

#[derive(Clone, Default)]
pub struct Index(Arc<Mutex<Vec<Entry>>>);

pub fn build_index() -> Index {
    let index = Index::default();
    let out = index.0.clone();
    std::thread::spawn(move || {
        let mut all = Vec::new();
        for var in ["ProgramData", "APPDATA"] {
            if let Some(base) = std::env::var_os(var) {
                let dir = Path::new(&base).join(r"Microsoft\Windows\Start Menu\Programs");
                walk(&dir, 6, true, &mut all);
            }
        }
        *out.lock().unwrap_or_else(PoisonError::into_inner) = all.clone();
        if let Some(home) = std::env::var_os("USERPROFILE") {
            for d in ["Desktop", "Documents", "Downloads"] {
                walk(&Path::new(&home).join(d), 4, false, &mut all);
            }
        }
        *out.lock().unwrap_or_else(PoisonError::into_inner) = all;
    });
    index
}

fn walk(dir: &Path, depth: u32, apps: bool, out: &mut Vec<Entry>) {
    const CAP: usize = 60_000;
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        if out.len() >= CAP {
            return;
        }
        let path = e.path();
        let fname = e.file_name().to_string_lossy().into_owned();
        if fname.starts_with('.') || fname == "node_modules" || fname == "target" {
            continue;
        }
        // file_type() comes free with the directory listing; is_dir() on the path costs a stat per entry
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            if depth > 0 {
                walk(&path, depth - 1, apps, out);
            }
            continue;
        }
        let ext = path.extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
        if apps && !matches!(ext.as_str(), "lnk" | "exe" | "url") {
            continue;
        }
        let name = if apps { path.file_stem().unwrap_or_default().to_string_lossy().into_owned() } else { fname };
        out.push(Entry { lower: name.to_lowercase(), name, path, app: apps });
    }
}

/// `awake` is the keep-awake state, so its row can offer to turn it off.
pub fn search(index: &Index, query: &str, awake: bool, pending: &[crate::reminders::Reminder]) -> Vec<Hit> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    let mut hits = Vec::new();
    if let Some(v) = calc(&q) {
        hits.push(Hit { action: None, kind: Kind::Calc, title: fmt_num(v), detail: format!("{query} ="), path: PathBuf::new() });
    }
    if let Some((title, detail)) = crate::actions::convert(query).or_else(|| crate::worldtime::answer(query)).or_else(|| crate::currency::answer(query)) {
        hits.push(Hit { action: None, kind: Kind::Calc, title, detail, path: PathBuf::new() });
    }
    for s in crate::actions::suggest(query, awake, pending) {
        hits.push(Hit { action: Some(s.action), kind: Kind::Action, title: s.title, detail: s.detail, path: PathBuf::new() });
    }
    let mut scored: Vec<(u8, &Entry)> = Vec::new();
    let guard = index.0.lock().unwrap_or_else(PoisonError::into_inner);
    for e in guard.iter() {
        let rank = if e.lower.starts_with(&q) {
            0
        } else if e.lower.contains(&q) {
            1
        } else {
            continue;
        };
        scored.push((rank * 2 + u8::from(!e.app), e));
    }
    scored.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.lower.len().cmp(&b.1.lower.len())));
    scored.dedup_by(|a, b| a.1.lower == b.1.lower && a.1.app == b.1.app);
    for (_, e) in scored.into_iter().take(5usize.saturating_sub(hits.len())) {
        hits.push(Hit {
            action: None,
            kind: if e.app { Kind::App } else { Kind::File },
            title: e.name.clone(),
            detail: e.path.parent().map(|p| p.display().to_string()).unwrap_or_default(),
            path: e.path.clone(),
        });
    }
    hits.push(web_hit(query.trim()));
    hits
}

/// Last row of every search: open the text as a link if it is one, otherwise search the web for it.
fn web_hit(q: &str) -> Hit {
    let direct = q.starts_with("http://") || q.starts_with("https://") || q.starts_with("www.");
    let (title, detail, url) = if direct {
        let url = if q.starts_with("www.") { format!("https://{q}") } else { q.to_owned() };
        (format!("Open {q}"), "Default browser".to_owned(), url)
    } else {
        (format!("Search the web for \"{q}\""), "DuckDuckGo".to_owned(), format!("https://duckduckgo.com/?q={}", url_encode(q)))
    };
    Hit { action: None, kind: Kind::Web, title, detail, path: PathBuf::from(url) }
}

fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn launch(path: &Path) {
    unsafe {
        ShellExecuteW(None, w!("open"), &HSTRING::from(path.as_os_str()), None, None, SW_SHOWNORMAL);
    }
}

fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 { format!("{v:.0}") } else { format!("{v:.8}").trim_end_matches('0').trim_end_matches('.').to_owned() }
}

/// Expression calculator. Needs an operator or parenthesis so "42" alone is not a result.
pub fn calc(s: &str) -> Option<f64> {
    if !s.contains(['+', '-', '*', '/', '^', '(', '%']) {
        return None;
    }
    let chars: Vec<char> = s.chars().filter(|c| !c.is_whitespace()).collect();
    let mut p = Parser { c: &chars, i: 0 };
    let v = p.expr()?;
    (p.i == chars.len() && v.is_finite()).then_some(v)
}

struct Parser<'a> {
    c: &'a [char],
    i: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.c.get(self.i).copied()
    }

    fn expr(&mut self) -> Option<f64> {
        let mut v = self.term()?;
        while let Some(op @ ('+' | '-')) = self.peek() {
            self.i += 1;
            let r = self.term()?;
            if op == '+' { v += r } else { v -= r }
        }
        Some(v)
    }

    fn term(&mut self) -> Option<f64> {
        let mut v = self.power()?;
        while let Some(op @ ('*' | '/' | '%')) = self.peek() {
            self.i += 1;
            let r = self.power()?;
            match op {
                '*' => v *= r,
                '/' => v /= r,
                _ => v %= r,
            }
        }
        Some(v)
    }

    fn power(&mut self) -> Option<f64> {
        let b = self.unary()?;
        if self.peek() == Some('^') {
            self.i += 1;
            return Some(b.powf(self.power()?));
        }
        Some(b)
    }

    fn unary(&mut self) -> Option<f64> {
        match self.peek()? {
            '-' => {
                self.i += 1;
                Some(-self.unary()?)
            }
            '+' => {
                self.i += 1;
                self.unary()
            }
            '(' => {
                self.i += 1;
                let v = self.expr()?;
                (self.peek()? == ')').then(|| self.i += 1)?;
                Some(v)
            }
            _ => {
                let start = self.i;
                while self.peek().is_some_and(|c| c.is_ascii_digit() || c == '.') {
                    self.i += 1;
                }
                self.c[start..self.i].iter().collect::<String>().parse().ok()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Kind, calc, url_encode, web_hit};

    #[test]
    fn web_hits() {
        assert_eq!(url_encode("a b&c/é"), "a+b%26c%2F%C3%A9");
        let s = web_hit("rust lang");
        assert!(s.kind == Kind::Web && s.path.to_string_lossy() == "https://duckduckgo.com/?q=rust+lang");
        assert_eq!(web_hit("www.example.com").path.to_string_lossy(), "https://www.example.com");
        assert_eq!(web_hit("https://a.b/c?d=1").path.to_string_lossy(), "https://a.b/c?d=1");
    }

    #[test]
    fn evaluates() {
        assert_eq!(calc("2+3*4"), Some(14.0));
        assert_eq!(calc("(2+3)*4"), Some(20.0));
        assert_eq!(calc("2^3^2"), Some(512.0));
        assert_eq!(calc("-3+5"), Some(2.0));
        assert_eq!(calc("42"), None);
        assert_eq!(calc("2+"), None);
    }
}
