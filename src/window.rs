//! One calculator window: display, keypad, history sidebar.
//!
//! Every window owns its own expression, mode and sidebar state; only the
//! [`History`] model is shared.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use gtk::pango;
use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use rug::{Assign, Float};

use crate::config::Config;
use crate::engine::{self, AngleMode, Env, DISPLAY_DIGITS};
use crate::history::{History, HistoryItem};
use crate::resize_bin::ResizeBin;
use crate::update::{self, Outcome};
use crate::{APP_ID, REPO_URL, RELEASES_URL, VERSION};

const ADV_COLS: i32 = 3;
/// Fixed key size: resizing the window never changes the buttons.
const KEY_W: i32 = 74;
const KEY_H: i32 = 56;
const KEY_GAP: i32 = 6;
/// Extra window width the advanced column block needs.
const ADV_WIDTH: i32 = ADV_COLS * (KEY_W + KEY_GAP);

#[derive(Clone, Copy)]
enum Act {
    Ins(&'static str),
    Clear,
    Back,
    Eval,
    Angle,
}

struct Key {
    label: &'static str,
    act: Act,
    col: i32,
    row: i32,
    span: i32,
    class: &'static str,
    adv: bool,
}

const fn k(label: &'static str, act: Act, col: i32, row: i32, class: &'static str) -> Key {
    Key { label, act, col, row, span: 1, class, adv: col < ADV_COLS }
}

fn keys() -> Vec<Key> {
    use Act::*;
    let b = ADV_COLS; // basic block starts here
    let mut v = vec![
        // advanced block (3 columns)
        k("sin", Ins("sin("), 0, 0, "fn"),
        k("cos", Ins("cos("), 1, 0, "fn"),
        k("tan", Ins("tan("), 2, 0, "fn"),
        k("sin⁻¹", Ins("asin("), 0, 1, "fn"),
        k("cos⁻¹", Ins("acos("), 1, 1, "fn"),
        k("tan⁻¹", Ins("atan("), 2, 1, "fn"),
        k("ln", Ins("ln("), 0, 2, "fn"),
        k("log", Ins("log("), 1, 2, "fn"),
        k("eˣ", Ins("exp("), 2, 2, "fn"),
        k("π", Ins("π"), 0, 3, "fn"),
        k("e", Ins("e"), 1, 3, "fn"),
        k("xʸ", Ins("^"), 2, 3, "fn"),
        k("n!", Ins("!"), 0, 4, "fn"),
        k("10ˣ", Ins("10^"), 1, 4, "fn"),
        k("DEG", Angle, 2, 4, "fn"),
        k("ans", Ins("ans"), 0, 5, "fn"),
        // basic block (4 columns)
        k("(", Ins("("), b, 0, "op"),
        k(")", Ins(")"), b + 1, 0, "op"),
        k("x²", Ins("^2"), b + 2, 0, "op"),
        k("√", Ins("√("), b + 3, 0, "op"),
        k("C", Clear, b, 1, "clear"),
        k("⌫", Back, b + 2, 1, "op"),
        k("÷", Ins("÷"), b + 3, 1, "op"),
        k("7", Ins("7"), b, 2, ""),
        k("8", Ins("8"), b + 1, 2, ""),
        k("9", Ins("9"), b + 2, 2, ""),
        k("×", Ins("×"), b + 3, 2, "op"),
        k("4", Ins("4"), b, 3, ""),
        k("5", Ins("5"), b + 1, 3, ""),
        k("6", Ins("6"), b + 2, 3, ""),
        k("−", Ins("−"), b + 3, 3, "op"),
        k("1", Ins("1"), b, 4, ""),
        k("2", Ins("2"), b + 1, 4, ""),
        k("3", Ins("3"), b + 2, 4, ""),
        k("+", Ins("+"), b + 3, 4, "op"),
        k("%", Ins("%"), b, 5, "op"),
        k("0", Ins("0"), b + 1, 5, ""),
        k(".", Ins("."), b + 2, 5, ""),
        k("=", Eval, b + 3, 5, "suggested-action"),
    ];
    for key in &mut v {
        match key.label {
            "C" => key.span = 2, // the clear key is double width
            "ans" => key.span = 3,
            _ => {}
        }
    }
    v
}

/// The backspace key face: a left-pointing tag with an ×, drawn with cairo
/// in the current text colour so it follows the theme.
fn backspace_icon() -> gtk::DrawingArea {
    let area = gtk::DrawingArea::builder()
        .content_width(20)
        .content_height(14)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .build();
    area.set_draw_func(|a, cr, w, h| {
        let c = a.color();
        let (w, h) = (w as f64, h as f64);
        let (x0, x1, x2) = (0.5, 0.5 + h * 0.5, w - 0.5);
        cr.set_source_rgba(c.red() as f64, c.green() as f64, c.blue() as f64, c.alpha() as f64);
        cr.set_line_join(gtk::cairo::LineJoin::Round);
        cr.move_to(x0, h / 2.0);
        cr.line_to(x1, 0.5);
        cr.line_to(x2, 0.5);
        cr.line_to(x2, h - 0.5);
        cr.line_to(x1, h - 0.5);
        cr.close_path();
        let _ = cr.fill_preserve();
        cr.set_line_width(1.0);
        let _ = cr.stroke();
        // cut the × out of the filled tag
        let (cx, cy, r) = ((x1 + x2) / 2.0 + 0.5, h / 2.0, h * 0.17);
        cr.set_operator(gtk::cairo::Operator::Clear);
        cr.set_line_width(1.8);
        cr.set_line_cap(gtk::cairo::LineCap::Round);
        cr.move_to(cx - r, cy - r);
        cr.line_to(cx + r, cy + r);
        cr.move_to(cx + r, cy - r);
        cr.line_to(cx - r, cy + r);
        let _ = cr.stroke();
    });
    area
}

/// True if typing `s` after a finished calculation should start a new
/// expression (digits, functions, parentheses) rather than continue it.
fn starts_new(s: &str) -> bool {
    s.chars()
        .next()
        .is_some_and(|c| c.is_alphanumeric() || matches!(c, '.' | '(' | 'π' | '√'))
}

fn font_attrs(px: f64) -> pango::AttrList {
    let list = pango::AttrList::new();
    list.insert(pango::AttrSize::new_size_absolute(
        (px * pango::SCALE as f64) as i32,
    ));
    list
}

struct Ui {
    history: Rc<History>,
    entry: gtk::Entry,
    status: gtk::Label,
    angle_btn: gtk::Button,
    keys: Vec<(gtk::Button, usize)>,
    radians: Cell<bool>,
    ans: RefCell<Float>,
    /// The entry currently holds a finished result.
    fresh: Cell<bool>,
    advanced: Cell<bool>,
}

impl Ui {
    fn insert(&self, text: &str) {
        let e = &self.entry;
        if self.fresh.replace(false) && starts_new(text) {
            e.set_text("");
        }
        e.delete_selection();
        let mut pos = e.position();
        e.insert_text(text, &mut pos);
        e.set_position(pos);
        e.grab_focus_without_selecting();
    }

    fn backspace(&self) {
        let e = &self.entry;
        self.fresh.set(false);
        if e.selection_bounds().is_some() {
            e.delete_selection();
        } else {
            let pos = e.position();
            if pos > 0 {
                e.delete_text(pos - 1, pos);
            }
        }
        e.grab_focus_without_selecting();
    }

    fn clear(&self) {
        self.fresh.set(false);
        self.entry.set_text("");
        self.set_status("", false);
        self.entry.grab_focus_without_selecting();
    }

    fn set_status(&self, text: &str, error: bool) {
        self.status.set_text(text);
        if error {
            self.status.add_css_class("error");
        } else {
            self.status.remove_css_class("error");
        }
    }

    fn angle(&self) -> AngleMode {
        if self.radians.get() {
            AngleMode::Rad
        } else {
            AngleMode::Deg
        }
    }

    fn preview(&self) {
        let text = self.entry.text();
        let text = text.trim();
        if text.is_empty() {
            self.set_status("", false);
            return;
        }
        let ans = self.ans.borrow();
        let env = Env { angle: self.angle(), ans: &ans };
        match engine::compute_display(text, &env) {
            Ok(s) if s != text => self.set_status(&format!("= {s}"), false),
            _ => self.set_status("", false),
        }
    }

    fn commit(&self) {
        let expr = self.entry.text().trim().to_string();
        if expr.is_empty() {
            return;
        }
        let result = {
            let ans = self.ans.borrow();
            let env = Env { angle: self.angle(), ans: &ans };
            engine::compute(&expr, &env)
        };
        match result {
            Ok(v) => {
                let s = engine::format_float(&v, DISPLAY_DIGITS);
                if s != expr {
                    self.history.append(&expr, &s);
                }
                *self.ans.borrow_mut() = v;
                self.entry.set_text(&s);
                self.entry.set_position(-1);
                self.set_status(&format!("{expr} ="), false);
                self.fresh.set(true);
            }
            Err(e) => self.set_status(&e.to_string(), true),
        }
        self.entry.grab_focus_without_selecting();
    }

    fn act(&self, act: Act) {
        match act {
            Act::Ins(t) => self.insert(t),
            Act::Clear => self.clear(),
            Act::Back => self.backspace(),
            Act::Eval => self.commit(),
            Act::Angle => {
                self.radians.set(!self.radians.get());
                self.angle_btn.set_label(if self.radians.get() { "RAD" } else { "DEG" });
                self.preview();
            }
        }
    }
}

pub fn new_window(app: &gtk::Application, history: &Rc<History>) {
    let cfg = Config::load();
    let win = gtk::ApplicationWindow::builder()
        .application(app)
        .title("Calculator.Sam")
        .default_width(cfg.width)
        .default_height(cfg.height)
        .build();

    // ---------------------------------------------------------- display
    let entry = gtk::Entry::builder()
        .xalign(1.0)
        .has_frame(false)
        .vexpand(true)
        .valign(gtk::Align::Center)
        .placeholder_text("0")
        .width_chars(1)
        .max_width_chars(1)
        .hexpand(true)
        .build();
    let status = gtk::Label::builder()
        .xalign(1.0)
        .ellipsize(pango::EllipsizeMode::Start)
        .margin_end(4)
        .css_classes(["status"])
        .build();
    status.set_label(" ");
    let display_inner = gtk::Box::new(gtk::Orientation::Vertical, 0);
    display_inner.append(&entry);
    display_inner.append(&status);
    let display = ResizeBin::new(&display_inner);
    display.add_css_class("display");
    display.set_size_request(240, 96);

    // ----------------------------------------------------------- keypad
    // Fixed-size keys, anchored to the bottom of the window.
    let grid = gtk::Grid::builder()
        .row_spacing(KEY_GAP)
        .column_spacing(KEY_GAP)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::End)
        .build();
    grid.add_css_class("keypad");
    let defs = keys();
    let mut buttons = Vec::new();
    let mut angle_btn = None;
    for key in &defs {
        let btn = gtk::Button::builder()
            .focus_on_click(false)
            .width_request(KEY_W)
            .height_request(KEY_H)
            .build();
        if matches!(key.act, Act::Back) {
            btn.set_child(Some(&backspace_icon()));
            btn.set_tooltip_text(Some("Backspace"));
        } else {
            btn.set_label(key.label);
            if key.label.chars().count() >= 3 {
                btn.add_css_class("small");
            }
        }
        if !key.class.is_empty() {
            btn.add_css_class(key.class);
        }
        if key.label == "DEG" {
            angle_btn = Some(btn.clone());
        }
        grid.attach(&btn, key.col, key.row, key.span, 1);
        buttons.push((btn, key.label.chars().count()));
    }
    let angle_btn = angle_btn.expect("angle key");
    if cfg.radians {
        angle_btn.set_label("RAD");
    }

    let ui = Rc::new(Ui {
        history: history.clone(),
        entry: entry.clone(),
        status: status.clone(),
        angle_btn,
        keys: buttons.clone(),
        radians: Cell::new(cfg.radians),
        ans: RefCell::new(Float::new(engine::PRECISION)),
        fresh: Cell::new(false),
        advanced: Cell::new(cfg.advanced),
    });
    ui.ans.borrow_mut().assign(0);

    let weak = |ui: &Rc<Ui>| -> Weak<Ui> { Rc::downgrade(ui) };

    for ((btn, _), key) in buttons.iter().zip(&defs) {
        if key.adv {
            btn.set_visible(cfg.advanced);
        }
        let w = weak(&ui);
        let act = key.act;
        btn.connect_clicked(move |_| {
            if let Some(ui) = w.upgrade() {
                ui.act(act);
            }
        });
    }

    // Display font scales with the display area (event-driven).
    {
        let last = Cell::new(0);
        let w = weak(&ui);
        display.connect_resized(move |_, height| {
            let px = ((height as f64) * 0.36).clamp(16.0, 72.0) as i32;
            if px == last.replace(px) {
                return;
            }
            let Some(ui) = w.upgrade() else { return };
            let w = Rc::downgrade(&ui);
            glib::idle_add_local_once(move || {
                if let Some(ui) = w.upgrade() {
                    ui.entry.set_attributes(&font_attrs(px as f64));
                    ui.status.set_attributes(Some(&font_attrs((px as f64 * 0.42).max(11.0))));
                }
            });
        });
    }

    // ------------------------------------------------------------ entry
    {
        let w = weak(&ui);
        entry.connect_changed(move |_| {
            if let Some(ui) = w.upgrade() {
                ui.preview();
            }
        });
        let w = weak(&ui);
        entry.connect_activate(move |_| {
            if let Some(ui) = w.upgrade() {
                ui.commit();
            }
        });
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let w = weak(&ui);
        keys.connect_key_pressed(move |_, keyval, _, state| {
            let Some(ui) = w.upgrade() else {
                return glib::Propagation::Proceed;
            };
            let ctrl = state.intersects(gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::ALT_MASK);
            if ctrl {
                if matches!(keyval, gdk::Key::c | gdk::Key::C) && ui.entry.selection_bounds().is_none() {
                    ui.entry.clipboard().set_text(&ui.entry.text());
                    return glib::Propagation::Stop;
                }
                return glib::Propagation::Proceed;
            }
            if keyval == gdk::Key::Escape {
                ui.clear();
                return glib::Propagation::Stop;
            }
            if matches!(keyval, gdk::Key::equal | gdk::Key::KP_Equal) {
                ui.commit();
                return glib::Propagation::Stop;
            }
            if ui.fresh.get() {
                if let Some(c) = keyval.to_unicode().filter(|c| !c.is_control()) {
                    if starts_new(&c.to_string()) {
                        ui.entry.set_text("");
                    }
                    ui.fresh.set(false);
                } else if matches!(keyval, gdk::Key::BackSpace | gdk::Key::Delete) {
                    ui.fresh.set(false);
                }
            }
            glib::Propagation::Proceed
        });
        entry.add_controller(keys);
    }

    // ---------------------------------------------------- history pane
    let list_view = build_history_list(&ui, history);
    let scroller = gtk::ScrolledWindow::builder()
        .child(&list_view)
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    let hist_title = gtk::Label::builder()
        .label("History")
        .xalign(0.0)
        .hexpand(true)
        .css_classes(["heading"])
        .build();
    let export_btn = gtk::Button::builder()
        .label("Export")
        .tooltip_text("Export history as a text file")
        .css_classes(["flat"])
        .action_name("win.export-history")
        .build();
    let clear_btn = gtk::Button::builder()
        .label("Clear")
        .tooltip_text("Clear history")
        .css_classes(["flat"])
        .action_name("win.clear-history")
        .build();
    let hist_head = gtk::Box::builder().spacing(2).margin_start(10).margin_end(4).margin_top(4).build();
    hist_head.append(&hist_title);
    hist_head.append(&export_btn);
    hist_head.append(&clear_btn);
    let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 0);
    sidebar.set_size_request(230, -1);
    sidebar.append(&hist_head);
    sidebar.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    sidebar.append(&scroller);
    sidebar.set_visible(cfg.history_visible);

    // ------------------------------------------------------ pane layout
    display.set_hexpand(true);
    // Extra window height goes to a recent-calculations list above the
    // display; at the minimum height it collapses to nothing.
    let inline_view = build_history_list(&ui, history);
    inline_view.add_css_class("inline-history");
    let inline_scroller = gtk::ScrolledWindow::builder()
        .child(&inline_view)
        .vexpand(true)
        .hexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .min_content_height(0)
        .build();
    let calc = gtk::Box::new(gtk::Orientation::Vertical, 10);
    calc.set_margin_start(10);
    calc.set_margin_end(10);
    calc.set_margin_top(10);
    calc.set_margin_bottom(10);
    calc.append(&inline_scroller);
    calc.append(&display);
    calc.append(&grid);
    let root = gtk::Paned::builder()
        .orientation(gtk::Orientation::Horizontal)
        .wide_handle(true)
        .start_child(&calc)
        .end_child(&sidebar)
        .resize_start_child(true)
        .resize_end_child(false)
        .shrink_start_child(false)
        .shrink_end_child(false)
        .build();
    if cfg.history_visible {
        root.set_position(cfg.width - cfg.sidebar);
    }
    let root_bin = ResizeBin::new(&root);
    win.set_child(Some(&root_bin));

    // ------------------------------------------------------- header bar
    let header = gtk::HeaderBar::new();
    // One compact toggle keeps the header (and so the window) narrow.
    let adv_btn = gtk::ToggleButton::builder()
        .label("Advanced")
        .tooltip_text("Advanced mode: trig, logs, powers, factorial")
        .active(cfg.advanced)
        .build();
    header.pack_start(&adv_btn);
    header.set_title_widget(Some(&gtk::Box::new(gtk::Orientation::Horizontal, 0)));
    let hist_toggle = gtk::ToggleButton::builder()
        .icon_name("document-open-recent-symbolic")
        .tooltip_text("History (Ctrl+H)")
        .action_name("win.toggle-history")
        .build();
    header.pack_end(&hist_toggle);
    let menu = gio::Menu::new();
    let sec1 = gio::Menu::new();
    sec1.append(Some("New Window"), Some("app.new-window"));
    let sec2 = gio::Menu::new();
    sec2.append(Some("Export History…"), Some("win.export-history"));
    sec2.append(Some("Clear History…"), Some("win.clear-history"));
    let sec3 = gio::Menu::new();
    sec3.append(Some("Check for Updates…"), Some("win.check-updates"));
    sec3.append(Some("Keyboard Shortcuts"), Some("win.shortcuts"));
    sec3.append(Some("About Calculator.Sam"), Some("win.about"));
    menu.append_section(None, &sec1);
    menu.append_section(None, &sec2);
    menu.append_section(None, &sec3);
    let menu_btn = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .menu_model(&menu)
        .build();
    header.pack_end(&menu_btn);
    win.set_titlebar(Some(&header));

    {
        let w = weak(&ui);
        let win_w = win.downgrade();
        adv_btn.connect_toggled(move |b| {
            let Some(ui) = w.upgrade() else { return };
            let on = b.is_active();
            if on == ui.advanced.replace(on) {
                return;
            }
            for ((btn, _), key) in ui.keys.iter().zip(keys().iter()) {
                if key.adv {
                    btn.set_visible(on);
                }
            }
            // Keys keep their size, so the window grows/shrinks with them.
            if let Some(win) = win_w.upgrade() {
                if !win.is_maximized() && !win.is_fullscreen() {
                    let w = if on { win.width() + ADV_WIDTH } else { (win.width() - ADV_WIDTH).max(300) };
                    win.set_default_size(w, win.height());
                }
            }
        });
    }

    // ---------------------------------------------------------- actions
    let toggle = gio::SimpleAction::new_stateful(
        "toggle-history",
        None,
        &cfg.history_visible.to_variant(),
    );
    {
        let sidebar = sidebar.clone();
        let root = root.clone();
        let win_w = win.downgrade();
        let side_w = Cell::new(cfg.sidebar);
        toggle.connect_activate(move |a, _| {
            let show = !a.state().and_then(|s| s.get::<bool>()).unwrap_or(false);
            a.set_state(&show.to_variant());
            let Some(win) = win_w.upgrade() else { return };
            let (w, h) = (win.width(), win.height());
            if show {
                let sw = side_w.get();
                sidebar.set_visible(true);
                if !win.is_maximized() {
                    win.set_default_size(w + sw, h);
                    root.set_position(w);
                } else {
                    root.set_position((w - sw).max(240));
                }
            } else {
                let sw = (w - root.position()).max(230);
                side_w.set(sw);
                sidebar.set_visible(false);
                if !win.is_maximized() {
                    win.set_default_size((w - sw).max(300), h);
                }
            }
        });
    }
    win.add_action(&toggle);

    let export = gio::SimpleAction::new("export-history", None);
    {
        let h = history.clone();
        let win_w = win.downgrade();
        export.connect_activate(move |_, _| {
            let Some(win) = win_w.upgrade() else { return };
            let dialog = gtk::FileDialog::builder()
                .title("Export History")
                .initial_name("calculator-history.txt")
                .build();
            let h = h.clone();
            dialog.save(Some(&win), gio::Cancellable::NONE, move |res| {
                if let Ok(file) = res {
                    let _ = file.replace_contents(
                        h.export_text().as_bytes(),
                        None,
                        false,
                        gio::FileCreateFlags::REPLACE_DESTINATION,
                        gio::Cancellable::NONE,
                    );
                }
            });
        });
    }
    win.add_action(&export);

    let clear = gio::SimpleAction::new("clear-history", None);
    {
        let h = history.clone();
        let win_w = win.downgrade();
        clear.connect_activate(move |_, _| {
            let Some(win) = win_w.upgrade() else { return };
            let n = h.store.n_items();
            let dialog = gtk::AlertDialog::builder()
                .message("Clear history?")
                .detail(format!("This permanently deletes all {n} saved calculations."))
                .buttons(["Cancel", "Clear History"])
                .cancel_button(0)
                .default_button(0)
                .build();
            let h = h.clone();
            dialog.choose(Some(&win), gio::Cancellable::NONE, move |res| {
                if res == Ok(1) {
                    h.clear();
                }
            });
        });
    }
    win.add_action(&clear);

    let shortcuts = gio::SimpleAction::new("shortcuts", None);
    {
        let win_w = win.downgrade();
        shortcuts.connect_activate(move |_, _| {
            if let Some(win) = win_w.upgrade() {
                show_shortcuts(&win);
            }
        });
    }
    win.add_action(&shortcuts);

    let about = gio::SimpleAction::new("about", None);
    {
        let win_w = win.downgrade();
        about.connect_activate(move |_, _| {
            let Some(win) = win_w.upgrade() else { return };
            gtk::AboutDialog::builder()
                .transient_for(&win)
                .modal(true)
                .program_name("Calculator.Sam")
                .logo_icon_name(APP_ID)
                .version(VERSION)
                .website(REPO_URL)
                .website_label("github.com/uri-travoski/calculator-sam")
                .comments("A minimal, fast calculator with exact decimal results.")
                .build()
                .present();
        });
    }
    win.add_action(&about);

    let updates = gio::SimpleAction::new("check-updates", None);
    {
        let win_w = win.downgrade();
        updates.connect_activate(move |_, _| {
            let Some(win) = win_w.upgrade() else { return };
            check_for_updates(&win);
        });
    }
    win.add_action(&updates);

    // Keep the shared ui alive exactly as long as the window.
    unsafe { win.set_data("ui", ui.clone()) };

    // Persist size/mode whenever the user changes them (debounced), on
    // close, and before quitting, so Ctrl+Q cannot lose them.
    let save: Rc<dyn Fn()> = {
        let ui = Rc::downgrade(&ui);
        let win_w = win.downgrade();
        let sidebar = sidebar.clone();
        let root = root.clone();
        Rc::new(move || {
            let (Some(ui), Some(win)) = (ui.upgrade(), win_w.upgrade()) else { return };
            let mut c = Config::load();
            if !win.is_maximized() && !win.is_fullscreen() && win.width() > 1 {
                c.width = win.width();
                c.height = win.height();
            }
            c.advanced = ui.advanced.get();
            c.radians = ui.radians.get();
            c.history_visible = sidebar.is_visible();
            if sidebar.is_visible() {
                c.sidebar = (win.width() - root.position()).clamp(220, 1200);
            }
            c.save();
        })
    };
    {
        let generation = Rc::new(Cell::new(0u32));
        let save = save.clone();
        root_bin.connect_resized(move |_, _| {
            let n = generation.get().wrapping_add(1);
            generation.set(n);
            let (generation, save) = (generation.clone(), save.clone());
            glib::timeout_add_local_once(std::time::Duration::from_millis(500), move || {
                if generation.get() == n {
                    save();
                }
            });
        });
    }
    let save_action = gio::SimpleAction::new("save-config", None);
    {
        let save = save.clone();
        save_action.connect_activate(move |_, _| save());
    }
    win.add_action(&save_action);
    win.connect_close_request({
        let save = save.clone();
        move |_| {
            save();
            glib::Propagation::Proceed
        }
    });

    win.present();
    entry.grab_focus_without_selecting();
}

/// The virtualised history list: a `ListView` over the shared `ListStore`.
fn build_history_list(ui: &Rc<Ui>, history: &Rc<History>) -> gtk::ListView {
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup({
        let history = history.clone();
        move |_, obj| {
            let Some(li) = obj.downcast_ref::<gtk::ListItem>() else { return };
            let row = gtk::Box::new(gtk::Orientation::Vertical, 2);
            row.add_css_class("history-row");
            let expr = gtk::Label::builder()
                .xalign(1.0)
                .ellipsize(pango::EllipsizeMode::Start)
                .css_classes(["expr", "caption"])
                .build();
            let result = gtk::Label::builder()
                .xalign(1.0)
                .ellipsize(pango::EllipsizeMode::End)
                .css_classes(["title-4"])
                .build();
            row.append(&expr);
            row.append(&result);

            let click = gtk::GestureClick::builder().button(3).build();
            let li_weak = li.downgrade();
            let history = history.clone();
            click.connect_pressed({
                let row = row.clone();
                move |_, _, x, y| {
                    let Some(item) = li_weak.upgrade().and_then(|l| l.item().and_downcast::<HistoryItem>())
                    else {
                        return;
                    };
                    show_row_menu(&row, &history, &item, x, y);
                }
            });
            row.add_controller(click);
            li.set_child(Some(&row));
        }
    });
    factory.connect_bind(|_, obj| {
        let Some(li) = obj.downcast_ref::<gtk::ListItem>() else { return };
        let (Some(item), Some(row)) = (
            li.item().and_downcast::<HistoryItem>(),
            li.child().and_downcast::<gtk::Box>(),
        ) else {
            return;
        };
        if let (Some(e), Some(r)) = (
            row.first_child().and_downcast::<gtk::Label>(),
            row.last_child().and_downcast::<gtk::Label>(),
        ) {
            e.set_text(&item.expr());
            r.set_text(&format!("= {}", item.result()));
        }
    });

    let model = gtk::NoSelection::new(Some(history.store.clone()));
    let view = gtk::ListView::new(Some(model), Some(factory));
    view.set_single_click_activate(true);

    // Click an entry: insert its result into the current expression.
    let w = Rc::downgrade(ui);
    view.connect_activate(move |lv, pos| {
        let item = lv.model().and_then(|m| m.item(pos)).and_downcast::<HistoryItem>();
        if let (Some(item), Some(ui)) = (item, w.upgrade()) {
            ui.insert(&item.result());
        }
    });

    // Newest entries are at the bottom: keep the view scrolled there.
    let view_weak = view.downgrade();
    let id = history.store.connect_items_changed(move |store, pos, _, added| {
        if added > 0 && pos + added == store.n_items() {
            let view_weak = view_weak.clone();
            glib::idle_add_local_once(move || {
                if let Some(v) = view_weak.upgrade() {
                    let n = v.model().map_or(0, |m| m.n_items());
                    if n > 0 {
                        v.scroll_to(n - 1, gtk::ListScrollFlags::NONE, None);
                    }
                }
            });
        }
    });
    let id = RefCell::new(Some(id));
    let store = history.store.clone();
    view.connect_destroy(move |_| {
        if let Some(id) = id.borrow_mut().take() {
            store.disconnect(id);
        }
    });
    view
}

fn show_row_menu(row: &gtk::Box, history: &Rc<History>, item: &HistoryItem, x: f64, y: f64) {
    let popover = gtk::Popover::new();
    popover.set_parent(row);
    popover.set_has_arrow(false);
    popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
    let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let add = |label: &str, f: Box<dyn Fn()>| {
        let b = gtk::Button::builder().label(label).css_classes(["flat"]).build();
        if let Some(l) = b.child().and_downcast::<gtk::Label>() {
            l.set_xalign(0.0);
        }
        let p = popover.clone();
        b.connect_clicked(move |_| {
            p.popdown();
            f();
        });
        list.append(&b);
    };
    let clipboard = row.clipboard();
    {
        let (c, it) = (clipboard.clone(), item.clone());
        add("Copy result", Box::new(move || c.set_text(&it.result())));
    }
    {
        let (c, it) = (clipboard, item.clone());
        add("Copy expression", Box::new(move || c.set_text(&it.expr())));
    }
    {
        let (h, it) = (history.clone(), item.clone());
        add("Delete", Box::new(move || h.remove(&it)));
    }
    popover.set_child(Some(&list));
    popover.connect_closed(|p| {
        let p = p.clone();
        glib::idle_add_local_once(move || p.unparent());
    });
    popover.popup();
}

fn show_shortcuts(parent: &gtk::ApplicationWindow) {
    const ROWS: &[(&str, &str)] = &[
        ("Enter or =", "Calculate"),
        ("Escape", "Clear"),
        ("Backspace", "Delete last character"),
        ("Ctrl+C / Ctrl+V", "Copy / paste"),
        ("Ctrl+H", "Show or hide history"),
        ("Ctrl+N", "New window"),
        ("Ctrl+W", "Close window"),
        ("Ctrl+Q", "Quit"),
        ("F1", "This window"),
    ];
    let grid = gtk::Grid::builder()
        .row_spacing(8)
        .column_spacing(24)
        .margin_top(18)
        .margin_bottom(18)
        .margin_start(18)
        .margin_end(18)
        .build();
    for (i, (keys, what)) in ROWS.iter().enumerate() {
        let a = gtk::Label::builder().label(*keys).xalign(0.0).css_classes(["monospace"]).build();
        let b = gtk::Label::builder().label(*what).xalign(0.0).build();
        grid.attach(&a, 0, i as i32, 1, 1);
        grid.attach(&b, 1, i as i32, 1, 1);
    }
    gtk::Window::builder()
        .title("Keyboard Shortcuts")
        .transient_for(parent)
        .modal(true)
        .resizable(false)
        .child(&grid)
        .build()
        .present();
}

/// Look up the latest release (off the main thread) and tell the user:
/// open the download page only if a newer version exists.
fn check_for_updates(win: &gtk::ApplicationWindow) {
    let win = win.clone();
    glib::spawn_future_local(async move {
        let outcome = gio::spawn_blocking(update::check)
            .await
            .unwrap_or_else(|_| Outcome::Failed("The update check was interrupted".into()));
        match outcome {
            Outcome::Newer(latest) => {
                let dialog = gtk::AlertDialog::builder()
                    .message("A new version is available")
                    .detail(format!(
                        "Calculator.Sam {latest} is available. You are running {VERSION}."
                    ))
                    .buttons(["Not Now", "Open Download Page"])
                    .cancel_button(0)
                    .default_button(1)
                    .build();
                let parent = win.clone();
                dialog.choose(Some(&win), gio::Cancellable::NONE, move |res| {
                    if res == Ok(1) {
                        gtk::UriLauncher::new(RELEASES_URL).launch(
                            Some(&parent),
                            gio::Cancellable::NONE,
                            |_| {},
                        );
                    }
                });
            }
            Outcome::UpToDate => {
                gtk::AlertDialog::builder()
                    .message("You are running the latest version")
                    .detail(format!("Calculator.Sam {VERSION} is up to date."))
                    .buttons(["OK"])
                    .build()
                    .show(Some(&win));
            }
            Outcome::Failed(why) => {
                gtk::AlertDialog::builder()
                    .message("Could not check for updates")
                    .detail(why)
                    .buttons(["OK"])
                    .build()
                    .show(Some(&win));
            }
        }
    });
}
