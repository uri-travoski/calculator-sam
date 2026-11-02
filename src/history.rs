//! Calculation history, kept per session.
//!
//! One [`Archive`] exists per process: every saved calculation of every
//! session, backed by an append-only JSON-lines file at
//! `$XDG_DATA_HOME/au.sam.Calc/history.jsonl`. Each window owns a
//! [`History`], the view of one session: its own `gio::ListStore` (the model
//! behind the virtualised `ListView`s). Rows saved by older versions have no
//! session and are grouped as session `0`.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::rc::Rc;

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;

use crate::APP_ID;

/// How many sessions the Manage History list shows.
pub const SESSION_LIMIT: usize = 15;

mod imp {
    use std::cell::RefCell;

    use gtk::glib;
    use gtk::glib::prelude::*;
    use gtk::glib::subclass::prelude::*;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::HistoryItem)]
    pub struct HistoryItem {
        #[property(get, set)]
        expr: RefCell<String>,
        #[property(get, set)]
        result: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for HistoryItem {
        const NAME: &'static str = "SamCalcHistoryItem";
        type Type = super::HistoryItem;
    }

    #[glib::derived_properties]
    impl ObjectImpl for HistoryItem {}
}

glib::wrapper! {
    pub struct HistoryItem(ObjectSubclass<imp::HistoryItem>);
}

impl HistoryItem {
    pub fn new(expr: &str, result: &str) -> Self {
        glib::Object::builder()
            .property("expr", expr)
            .property("result", result)
            .build()
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Row {
    session: u64,
    expr: String,
    result: String,
}

/// What the Manage History list shows about one session.
pub struct SessionInfo {
    /// Start time (unix seconds); `0` for history saved before sessions.
    pub id: u64,
    pub count: usize,
    pub last: Option<(String, String)>,
}

pub struct Archive {
    rows: RefCell<Vec<Row>>,
    path: PathBuf,
    /// True while the file is still being read in the background.
    loading: Cell<bool>,
    /// Rows added while loading; flushed (in order) once loading ends.
    pending: RefCell<Vec<Row>>,
    last_id: Cell<u64>,
    /// Windows currently showing a session.
    open: RefCell<HashMap<u64, glib::WeakRef<gtk::ApplicationWindow>>>,
}

impl Archive {
    /// Create the archive and start loading the file without blocking the
    /// UI: parsing runs on a worker thread.
    pub fn load() -> Rc<Self> {
        let archive = Rc::new(Self {
            rows: RefCell::new(Vec::new()),
            path: data_path(),
            loading: Cell::new(true),
            pending: RefCell::new(Vec::new()),
            last_id: Cell::new(0),
            open: RefCell::new(HashMap::new()),
        });
        let a = archive.clone();
        glib::spawn_future_local(async move {
            let path = a.path.clone();
            let rows = gio::spawn_blocking(move || read_rows(&path))
                .await
                .unwrap_or_default();
            *a.rows.borrow_mut() = rows;
            a.loading.set(false);
            let pending = std::mem::take(&mut *a.pending.borrow_mut());
            for row in pending {
                a.push(row);
            }
        });
        archive
    }

    fn new_id(&self) -> u64 {
        let now = (glib::real_time() / 1_000_000) as u64;
        let id = now.max(self.last_id.get() + 1);
        self.last_id.set(id);
        id
    }

    fn push(&self, row: Row) {
        if self.loading.get() {
            self.pending.borrow_mut().push(row);
            return;
        }
        if let Some(mut f) = self.open_append() {
            let _ = writeln!(f, "{}", row_to_json(&row));
        }
        self.rows.borrow_mut().push(row);
    }

    fn session_rows(&self, id: u64) -> Vec<Row> {
        self.rows.borrow().iter().filter(|r| r.session == id).cloned().collect()
    }

    /// The newest `limit` sessions that have at least one calculation.
    pub fn sessions(&self, limit: usize) -> Vec<SessionInfo> {
        let mut map: BTreeMap<u64, SessionInfo> = BTreeMap::new();
        for r in self.rows.borrow().iter() {
            let s = map.entry(r.session).or_insert(SessionInfo {
                id: r.session,
                count: 0,
                last: None,
            });
            s.count += 1;
            s.last = Some((r.expr.clone(), r.result.clone()));
        }
        map.into_values().rev().take(limit).collect()
    }

    /// Delete a whole session from the saved history.
    pub fn delete_session(&self, id: u64) {
        if self.loading.get() {
            return;
        }
        self.rows.borrow_mut().retain(|r| r.session != id);
        self.rewrite();
    }

    pub fn register(&self, id: u64, win: &gtk::ApplicationWindow) {
        self.open.borrow_mut().insert(id, win.downgrade());
    }

    pub fn unregister(&self, id: u64) {
        self.open.borrow_mut().remove(&id);
    }

    /// The window currently showing this session, if any.
    pub fn window_of(&self, id: u64) -> Option<gtk::ApplicationWindow> {
        self.open.borrow().get(&id).and_then(|w| w.upgrade())
    }

    fn open_append(&self) -> Option<fs::File> {
        if let Some(dir) = self.path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        OpenOptions::new().create(true).append(true).open(&self.path).ok()
    }

    /// Only used for delete/clear: the file is otherwise never rewritten.
    fn rewrite(&self) {
        if let Some(dir) = self.path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let mut text = String::new();
        for r in self.rows.borrow().iter() {
            text.push_str(&row_to_json(r));
            text.push('\n');
        }
        let _ = fs::write(&self.path, text);
    }
}

/// One session's calculations, as shown in a window.
pub struct History {
    pub store: gio::ListStore,
    pub session: u64,
    archive: Rc<Archive>,
}

impl History {
    /// A brand-new session, or `Some(id)` to revive a saved one.
    pub fn new(archive: &Rc<Archive>, revive: Option<u64>) -> Rc<Self> {
        let store = gio::ListStore::new::<HistoryItem>();
        let session = match revive {
            Some(id) => {
                let items: Vec<HistoryItem> = archive
                    .session_rows(id)
                    .iter()
                    .map(|r| HistoryItem::new(&r.expr, &r.result))
                    .collect();
                store.extend_from_slice(&items);
                id
            }
            None => archive.new_id(),
        };
        Rc::new(Self { store, session, archive: archive.clone() })
    }

    /// Result of the newest calculation (to restore `ans` on revive).
    pub fn last_result(&self) -> Option<String> {
        let n = self.store.n_items();
        (n > 0)
            .then(|| self.store.item(n - 1).and_downcast::<HistoryItem>())
            .flatten()
            .map(|i| i.result())
    }

    pub fn append(&self, expr: &str, result: &str) {
        if expr.trim().is_empty() {
            return;
        }
        self.store.append(&HistoryItem::new(expr, result));
        self.archive.push(Row {
            session: self.session,
            expr: expr.into(),
            result: result.into(),
        });
    }

    pub fn remove(&self, item: &HistoryItem) {
        if self.archive.loading.get() {
            return;
        }
        if let Some(pos) = self.store.find(item) {
            self.store.remove(pos);
            let mut rows = self.archive.rows.borrow_mut();
            let idx = rows
                .iter()
                .enumerate()
                .filter(|(_, r)| r.session == self.session)
                .nth(pos as usize)
                .map(|(i, _)| i);
            if let Some(i) = idx {
                rows.remove(i);
            }
            drop(rows);
            self.archive.rewrite();
        }
    }

    pub fn clear(&self) {
        if self.archive.loading.get() {
            return;
        }
        self.store.remove_all();
        self.archive.delete_session(self.session);
    }

    /// Plain-text export: one `expression = result` line per entry.
    pub fn export_text(&self) -> String {
        let mut out = String::new();
        for i in 0..self.store.n_items() {
            if let Some(it) = self.store.item(i).and_downcast::<HistoryItem>() {
                out.push_str(&it.expr());
                out.push_str(" = ");
                out.push_str(&it.result());
                out.push('\n');
            }
        }
        out
    }
}

fn data_path() -> PathBuf {
    glib::user_data_dir().join(APP_ID).join("history.jsonl")
}

fn row_to_json(r: &Row) -> String {
    serde_json::json!({ "s": r.session, "expr": r.expr, "result": r.result }).to_string()
}

/// Parse the JSONL file, skipping blank or corrupt lines.
fn read_rows(path: &PathBuf) -> Vec<Row> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            let v: serde_json::Value = serde_json::from_str(line).ok()?;
            Some(Row {
                session: v.get("s").and_then(|s| s.as_u64()).unwrap_or(0),
                expr: v.get("expr")?.as_str()?.to_string(),
                result: v.get("result")?.as_str()?.to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_roundtrip_with_special_chars() {
        let row = Row { session: 1700000000, expr: "\"a\" + 1\n".into(), result: "2".into() };
        let line = row_to_json(&row);
        let dir = std::env::temp_dir().join("sam-calc-test-history.jsonl");
        // a row from before sessions existed has no "s"
        fs::write(&dir, format!("garbage\n\n{line}\n{{\"expr\":\"1+1\",\"result\":\"2\"}}\n")).unwrap();
        let rows = read_rows(&dir);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], row);
        assert_eq!(rows[1].session, 0);
        let _ = fs::remove_file(dir);
    }
}
