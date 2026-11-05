//! LIKE work time (Sequence ▸ LIKE ▸ Munkaidő ezen a projekten): how long the user actively worked
//! on a project, so the 6–8 hours a video used to take can be compared with today.
//!
//! Measured from the UI loop ([`poll`], once per frame). An activity is an edit (the project
//! revision changed) or the playhead moving (playback, scrubbing). The time between two
//! activities counts as work when it is at most [`IDLE_SECONDS`]; a longer gap is a break. Each
//! counted gap is booked to the kind of work that ended it: the command of the newest undo step
//! (captions, audio, colour, graphics, cutting), or "Megtekintés" when only the playhead moved.
//! LIKE background jobs add their machine time ([`add_machine`]).
//!
//! Only a session with auto-save running (the desktop app) is measured; tests and the CLI are not.
//! The totals live next to the project in `<project>.like-munkaido.json`, written at most once a
//! minute and whenever the report is asked for. An unsaved project's time moves to the file it is
//! first saved as.

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize};
use web_time::Instant;

use filmcraft_time::Tick;

use crate::Session;

/// A pause longer than this (seconds) is a break, not work.
pub const IDLE_SECONDS: f64 = 300.0;
/// Totals are written to disk at most this often (seconds).
const WRITE_EVERY_SECONDS: f64 = 60.0;
/// Kind booked when only the playhead moved.
pub const VIEWING: &str = "Megtekintés";

/// Work time of one project.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Totals {
    /// Active work, seconds.
    #[serde(default)]
    pub active_seconds: f64,
    /// Active work by kind ("Vágás", "Felirat"…), seconds.
    #[serde(default)]
    pub by_kind: BTreeMap<String, f64>,
    /// Active work by local day (`YYYY-MM-DD`), seconds.
    #[serde(default)]
    pub by_day: BTreeMap<String, f64>,
    /// LIKE background jobs (transcription, AI correction…), seconds.
    #[serde(default)]
    pub machine_seconds: f64,
}

impl Totals {
    fn book(&mut self, seconds: f64, kind: &str, day: &str) {
        if !seconds.is_finite() || seconds <= 0.0 {
            return;
        }
        self.active_seconds += seconds;
        *self.by_kind.entry(kind.to_string()).or_insert(0.0) += seconds;
        *self.by_day.entry(day.to_string()).or_insert(0.0) += seconds;
    }

    fn merge(&mut self, o: Totals) {
        self.active_seconds += o.active_seconds;
        self.machine_seconds += o.machine_seconds;
        for (k, v) in o.by_kind {
            *self.by_kind.entry(k).or_insert(0.0) += v;
        }
        for (k, v) in o.by_day {
            *self.by_day.entry(k).or_insert(0.0) += v;
        }
    }

    /// Drop values a hand-edited or damaged file may carry.
    fn sanitized(mut self) -> Totals {
        let ok = |x: f64| if x.is_finite() && x >= 0.0 { x } else { 0.0 };
        self.active_seconds = ok(self.active_seconds);
        self.machine_seconds = ok(self.machine_seconds);
        self.by_kind.values_mut().for_each(|v| *v = ok(*v));
        self.by_day.values_mut().for_each(|v| *v = ok(*v));
        self
    }
}

/// The kind of work a command is.
pub fn kind_of(command: &str) -> &'static str {
    let c = command.to_ascii_lowercase();
    let has = |keys: &[&str]| keys.iter().any(|k| c.contains(k));
    if has(&["caption", "subtitle", "moveword"]) {
        "Felirat"
    } else if has(&["audio", "sound", "mixer", "volume", "narration", "cleanaudio", "gain", "loudness"]) {
        "Hang"
    } else if has(&["color", "colour", "lumetri", "lut"]) {
        "Szín"
    } else if has(&["graphic", "title", "text."]) {
        "Grafika"
    } else {
        "Vágás"
    }
}

struct Tracker {
    /// Project path ("" = not saved yet).
    key: String,
    revision: u64,
    playhead: Tick,
    last: Option<Instant>,
    totals: Totals,
    dirty: bool,
    written: Option<Instant>,
}

static TRACKER: Mutex<Option<Tracker>> = Mutex::new(None);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Where a project's totals are kept.
pub fn sidecar(project: &str) -> Option<std::path::PathBuf> {
    if project.is_empty() { None } else { Some(std::path::PathBuf::from(format!("{project}.like-munkaido.json"))) }
}

/// The saved totals of a project (empty when there are none or the file is unreadable).
pub fn load(project: &str) -> Totals {
    let Some(path) = sidecar(project) else { return Totals::default() };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Totals>(raw.trim_start_matches('\u{feff}')).ok())
        .map(Totals::sanitized)
        .unwrap_or_default()
}

fn save(project: &str, t: &Totals) {
    if cfg!(target_arch = "wasm32") {
        return;
    }
    let Some(path) = sidecar(project) else { return };
    if let Ok(bytes) = serde_json::to_vec_pretty(t) {
        // a failed write only loses the newest minutes; the work itself is unaffected
        let _ = std::fs::write(path, bytes);
    }
}

impl Tracker {
    fn flush(&mut self) {
        if self.dirty && !self.key.is_empty() {
            save(&self.key, &self.totals);
            self.dirty = false;
        }
    }
}

/// The local date (`YYYY-MM-DD`) of `unix`, at the session's UTC offset in seconds.
pub fn day_of(unix: i64, offset_secs: i32) -> String {
    chrono::DateTime::from_timestamp(unix.saturating_add(i64::from(offset_secs)), 0).map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default()
}

fn today(s: &Session) -> String {
    let unix = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX)).unwrap_or(0);
    let offset = s.persistence.as_ref().map_or(0, |p| (p.local_offset)(unix));
    day_of(unix, offset)
}

/// Measure one UI frame of `s`.
pub fn poll(s: &Session) {
    if s.persistence.is_none() {
        return;
    }
    let key = s.path.clone().unwrap_or_default();
    let now = Instant::now();
    let (revision, playhead) = (s.revision, s.playhead());
    let mut g = lock(&TRACKER);
    let t = g.get_or_insert_with(|| Tracker { key: key.clone(), revision, playhead, last: None, totals: load(&key), dirty: false, written: None });
    if t.key != key {
        // Save As of an unsaved project keeps its time; another project starts its own count
        let carried = if t.key.is_empty() {
            std::mem::take(&mut t.totals)
        } else {
            t.flush();
            Totals::default()
        };
        let mut totals = load(&key);
        totals.merge(carried);
        *t = Tracker { key, revision, playhead, last: None, totals, dirty: true, written: None };
        return;
    }
    let edited = revision != t.revision;
    if edited || playhead != t.playhead {
        if let Some(prev) = t.last {
            let gap = now.saturating_duration_since(prev).as_secs_f64();
            if gap <= IDLE_SECONDS {
                let kind = if edited { s.history.undo.last().and_then(|(_, _, cmd)| cmd.as_deref()).map(kind_of).unwrap_or("Vágás") } else { VIEWING };
                t.totals.book(gap, kind, &today(s));
                t.dirty = true;
            }
        }
        t.last = Some(now);
        t.revision = revision;
        t.playhead = playhead;
    }
    if t.dirty && t.written.is_none_or(|w| now.saturating_duration_since(w).as_secs_f64() >= WRITE_EVERY_SECONDS) {
        t.flush();
        t.written = Some(now);
    }
}

/// Book machine time of a finished LIKE job to the project being measured.
pub fn add_machine(seconds: f64) {
    if !seconds.is_finite() || seconds <= 0.0 {
        return;
    }
    if let Some(t) = lock(&TRACKER).as_mut() {
        t.totals.machine_seconds += seconds;
        t.dirty = true;
    }
}

/// The totals of `s`'s project (written to disk first), or with `reset` set back to zero.
pub fn report(s: &Session, reset: bool) -> Totals {
    let key = s.path.clone().unwrap_or_default();
    let mut g = lock(&TRACKER);
    match g.as_mut().filter(|t| t.key == key) {
        Some(t) => {
            if reset {
                t.totals = Totals::default();
                t.dirty = true;
            }
            t.flush();
            t.totals.clone()
        }
        None => {
            if reset {
                save(&key, &Totals::default());
            }
            if reset { Totals::default() } else { load(&key) }
        }
    }
}

/// "1 ó 25 p", "12 p", "1 p alatt".
pub fn duration_hu(seconds: f64) -> String {
    let m = if seconds.is_finite() && seconds > 0.0 { (seconds / 60.0).round() as u64 } else { 0 };
    match (m / 60, m % 60) {
        (0, 0) => "1 p alatt".into(),
        (0, p) => format!("{p} p"),
        (h, 0) => format!("{h} ó"),
        (h, p) => format!("{h} ó {p} p"),
    }
}

/// The report as one line for a toast.
pub fn summary(t: &Totals) -> String {
    let mut kinds: Vec<(&String, &f64)> = t.by_kind.iter().filter(|(_, v)| **v >= 30.0).collect();
    kinds.sort_by(|a, b| b.1.total_cmp(a.1));
    let parts: Vec<String> = kinds.iter().map(|(k, v)| format!("{} {}", k.to_lowercase(), duration_hu(**v))).collect();
    let mut line = format!("Munkaidő ezen a projekten: {}", duration_hu(t.active_seconds));
    if !parts.is_empty() {
        line.push_str(&format!(" ({})", parts.join(", ")));
    }
    if t.by_day.len() > 1 {
        line.push_str(&format!(", {} napon", t.by_day.len()));
    }
    if t.machine_seconds >= 1.0 {
        line.push_str(&format!(". Gépi feldolgozás: {}", duration_hu(t.machine_seconds)));
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_follow_the_command() {
        assert_eq!(kind_of("captions.setText"), "Felirat");
        assert_eq!(kind_of("like.moveWordNext"), "Felirat");
        assert_eq!(kind_of("like.correctCaptions"), "Felirat");
        assert_eq!(kind_of("essentialSound.applyPreset"), "Hang");
        assert_eq!(kind_of("like.cleanAudio"), "Hang");
        assert_eq!(kind_of("lumetri.set"), "Szín");
        assert_eq!(kind_of("timeline.razor"), "Vágás");
        assert_eq!(kind_of(""), "Vágás");
    }

    #[test]
    fn days_are_local() {
        // 2026-11-05 23:30 UTC is already the 6th in Budapest (UTC+1)
        let unix = 1_793_921_400;
        assert_eq!(day_of(unix, 0), "2026-11-05");
        assert_eq!(day_of(unix, 3600), "2026-11-06");
        assert_eq!(day_of(i64::MAX, i32::MAX), "");
    }

    #[test]
    fn durations_read_in_hungarian() {
        assert_eq!(duration_hu(0.0), "1 p alatt");
        assert_eq!(duration_hu(f64::NAN), "1 p alatt");
        assert_eq!(duration_hu(12.0 * 60.0), "12 p");
        assert_eq!(duration_hu(85.0 * 60.0), "1 ó 25 p");
        assert_eq!(duration_hu(120.0 * 60.0), "2 ó");
    }

    #[test]
    fn booking_and_summary() {
        let mut t = Totals::default();
        t.book(40.0 * 60.0, "Vágás", "2026-11-05");
        t.book(30.0 * 60.0, "Felirat", "2026-11-05");
        t.book(15.0 * 60.0, "Felirat", "2026-11-06");
        t.book(-5.0, "Vágás", "2026-11-06");
        t.book(f64::INFINITY, "Vágás", "2026-11-06");
        t.machine_seconds = 200.0;
        assert_eq!(t.active_seconds, 85.0 * 60.0);
        assert_eq!(summary(&t), "Munkaidő ezen a projekten: 1 ó 25 p (felirat 45 p, vágás 40 p), 2 napon. Gépi feldolgozás: 3 p");
    }

    #[test]
    fn totals_survive_a_round_trip_and_damage() {
        let dir = std::env::temp_dir().join(format!("like-worktime-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let project = dir.join("p.fcproj").to_string_lossy().into_owned();
        let mut t = Totals::default();
        t.book(90.0, "Felirat", "2026-11-05");
        save(&project, &t);
        assert_eq!(load(&project), t);
        std::fs::write(sidecar(&project).unwrap(), r#"{"active_seconds": -3, "by_kind": {"Vágás": 1e400}}"#).unwrap();
        let bad = load(&project);
        assert_eq!(bad.active_seconds, 0.0);
        std::fs::write(sidecar(&project).unwrap(), "nem json").unwrap();
        assert_eq!(load(&project), Totals::default());
        assert_eq!(load(""), Totals::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sessions_without_auto_save_are_not_measured() {
        let s = Session::default();
        poll(&s);
        assert!(lock(&TRACKER).as_ref().is_none_or(|t| t.key != "never"), "no panic, nothing written");
    }
}
