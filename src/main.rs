mod config;
mod engine;
mod history;
mod resize_bin;
mod update;
mod window;

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gdk, gio, glib};

pub const APP_ID: &str = "au.sam.Calc";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const REPO_URL: &str = "https://github.com/uri-travoski/calculator-sam";
pub const RELEASES_URL: &str = "https://github.com/uri-travoski/calculator-sam/releases/latest";
pub const REPO_API_LATEST: &str =
    "https://api.github.com/repos/uri-travoski/calculator-sam/releases/latest";

fn main() -> glib::ExitCode {
    // A calculator needs no GPU: the software renderer avoids creating a GL
    // context per process, which saves ~55 MB. `GSK_RENDERER=ngl` overrides.
    if std::env::var_os("GSK_RENDERER").is_none() {
        std::env::set_var("GSK_RENDERER", "cairo");
    }
    let app = gtk::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();

    // One history for the whole process, shared by every window.
    let history: Rc<RefCell<Option<Rc<history::History>>>> = Rc::new(RefCell::new(None));

    app.connect_startup({
        let history = history.clone();
        move |app| {
            gtk::Window::set_default_icon_name(APP_ID);
            let css = gtk::CssProvider::new();
            css.load_from_string(include_str!("../data/style.css"));
            if let Some(display) = gdk::Display::default() {
                gtk::style_context_add_provider_for_display(
                    &display,
                    &css,
                    gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
                );
            }
            *history.borrow_mut() = Some(history::History::load());

            let new_window = gio::SimpleAction::new("new-window", None);
            new_window.connect_activate({
                let app = app.clone();
                let history = history.clone();
                move |_, _| open_window(&app, &history)
            });
            app.add_action(&new_window);

            let quit = gio::SimpleAction::new("quit", None);
            quit.connect_activate({
                let app = app.clone();
                move |_, _| {
                    for w in app.windows() {
                        let _ = w.activate_action("win.save-config", None);
                    }
                    app.quit()
                }
            });
            app.add_action(&quit);

            app.set_accels_for_action("app.new-window", &["<Ctrl>n"]);
            app.set_accels_for_action("app.quit", &["<Ctrl>q"]);
            app.set_accels_for_action("win.toggle-history", &["<Ctrl>h"]);
            app.set_accels_for_action("window.close", &["<Ctrl>w"]);
            app.set_accels_for_action("win.shortcuts", &["<Ctrl>question", "F1"]);
        }
    });

    // Every launch (first, second, or `--new-window`) opens a new window in
    // this one process; a second launch is forwarded here over D-Bus.
    app.connect_command_line({
        let history = history.clone();
        move |app, _cmd| {
            open_window(app, &history);
            glib::ExitCode::SUCCESS
        }
    });

    app.run()
}

fn open_window(app: &gtk::Application, history: &Rc<RefCell<Option<Rc<history::History>>>>) {
    if let Some(h) = history.borrow().as_ref() {
        window::new_window(app, h);
    }
}
