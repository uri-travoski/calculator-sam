//! Shared calculation history.
//!
//! One [`History`] exists per process and is shared by every window: a
//! `gio::ListStore` (the model behind the virtualised sidebar `ListView`)
//! plus an append-only JSON-lines file at
//! `$XDG_DATA_HOME/au.sam.Calc/history.jsonl`.

use std::cell::{Cell, RefCell};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;

use crate::APP_ID;

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

pub struct History {
    pub store: gio::ListStore,
    path: PathBuf,
    /// True while the file is still being read in the background.
    loading: Cell<bool>,
    /// Entries added while loading; flushed (in order) once loading ends.
    pending: RefCell<Vec<(String, String)>>,
}

impl History {
    /// Create the shared history and start loading the file without
    /// blocking the UI: parsing runs on a worker thread, and rows are
    /// added to the model in chunks.
    pub fn load() -> Rc<Self> {
        let hist = Rc::new(Self {
            store: gio::ListStore::new::<HistoryItem>(),
            path: data_path(),
            loading: Cell::new(true),
            pending: RefCell::new(Vec::new()),
        });
        let h = hist.clone();
        glib::spawn_future_local(async move {
            let path = h.path.clone();
            let rows = gio::spawn_blocking(move || read_rows(&path))
                .await
                .unwrap_or_default();
            for chunk in rows.chunks(2000) {
                let items: Vec<HistoryItem> =
                    chunk.iter().map(|(e, r)| HistoryItem::new(e, r)).collect();
                h.store.extend_from_slice(&items);
                glib::timeout_future(Duration::from_millis(1)).await;
            }
            h.loading.set(false);
            let pending = std::mem::take(&mut *h.pending.borrow_mut());
            for (e, r) in pending {
                h.append(&e, &r);
            }
        });
        hist
    }

    pub fn append(&self, expr: &str, result: &str) {
        if expr.trim().is_empty() {
            return;
        }
        if self.loading.get() {
            self.pending.borrow_mut().push((expr.into(), result.into()));
            return;
        }
        self.store.append(&HistoryItem::new(expr, result));
        if let Some(mut f) = self.open_append() {
            let _ = writeln!(f, "{}", row_to_json(expr, result));
        }
    }

    pub fn remove(&self, item: &HistoryItem) {
        if self.loading.get() {
            return;
        }
        if let Some(pos) = self.store.find(item) {
            self.store.remove(pos);
            self.rewrite();
        }
    }

    pub fn clear(&self) {
        if self.loading.get() {
            return;
        }
        self.store.remove_all();
        self.rewrite();
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
        for i in 0..self.store.n_items() {
            if let Some(it) = self.store.item(i).and_downcast::<HistoryItem>() {
                text.push_str(&row_to_json(&it.expr(), &it.result()));
                text.push('\n');
            }
        }
        let _ = fs::write(&self.path, text);
    }
}

fn data_path() -> PathBuf {
    glib::user_data_dir().join(APP_ID).join("history.jsonl")
}

fn row_to_json(expr: &str, result: &str) -> String {
    serde_json::json!({ "expr": expr, "result": result }).to_string()
}

/// Parse the JSONL file, skipping blank or corrupt lines.
fn read_rows(path: &PathBuf) -> Vec<(String, String)> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            let v: serde_json::Value = serde_json::from_str(line).ok()?;
            Some((
                v.get("expr")?.as_str()?.to_string(),
                v.get("result")?.as_str()?.to_string(),
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_roundtrip_with_special_chars() {
        let line = row_to_json("\"a\" + 1\n", "2");
        let dir = std::env::temp_dir().join("sam-calc-test-history.jsonl");
        fs::write(&dir, format!("garbage\n\n{line}\n")).unwrap();
        let rows = read_rows(&dir);
        assert_eq!(rows, vec![("\"a\" + 1\n".to_string(), "2".to_string())]);
        let _ = fs::remove_file(dir);
    }
}
