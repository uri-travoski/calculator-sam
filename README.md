# Calculator.Sam

A minimal, fast desktop calculator for Linux, written in Rust with plain GTK4
(no libadwaita, no WebKit). Works on X11 and Wayland.

* **Exact decimals** – `0.1 + 0.2` is `0.3` (≈50 significant digits internally via GMP/MPFR, 15 shown).
* **Basic and Advanced modes** – trig (deg/rad), inverse trig, ln, log, eˣ, xʸ, π, e, factorial.
* **Type or click** – one editable expression entry; implicit multiplication (`2π`, `3(4+1)`), unary minus, `ans`.
* **Resizable window** – keys keep their size and stay anchored at the bottom; the display takes the extra room. Size and mode are remembered.
* **History sidebar** – virtualised list (50 000+ entries are fine), click an entry to reuse its result, right-click to copy or delete, **export as a text file**, clear with confirmation.
* **Multiple windows, one process** – a second launch or `sam-calc --new-window` opens a window in the running instance; all windows share one history.
* **Check for Updates** in the menu: opens the GitHub download page only if a newer release exists (checked only when you ask; needs the repo to be public).
* Remembers window size, mode and angle unit in `$XDG_CONFIG_HOME/au.sam.Calc/config.toml`; history lives in `$XDG_DATA_HOME/au.sam.Calc/history.jsonl`.

Keys: `Enter`/`=` calculate · `Esc` clear · `Ctrl+H` history · `Ctrl+N` new window · `Ctrl+W` close · `Ctrl+Q` quit · `F1` shortcuts.

## Install

Download a package from the [Releases](../../releases) page: `.deb`, `.rpm`, `.flatpak`
or `.AppImage`.

## Build from source

Needs Rust (stable, 1.83+), GTK 4.12+ development files, `m4`, `make` and a C
compiler (the arbitrary-precision library `rug` builds GMP/MPFR from source, so
no system GMP is needed).

**Fedora**

```sh
sudo dnf install gcc make m4 pkgconf-pkg-config gtk4-devel
```

**Debian / Ubuntu / Mint** (Ubuntu 24.04+, Debian 13+, Mint 22+ ship GTK ≥ 4.12)

```sh
sudo apt install build-essential m4 pkg-config libgtk-4-dev
```

**Arch**

```sh
sudo pacman -S base-devel m4 gtk4
```

Then:

```sh
make build      # cargo build --release
make test       # engine + config + history unit tests
make run
make install    # to ~/.local: binary, desktop file, icons
make deb        # needs: cargo install cargo-deb
make rpm        # needs: cargo install cargo-generate-rpm
```

Packages install `/usr/bin/sam-calc`, `au.sam.Calc.desktop` and the icon
`au.sam.Calc.svg` (the desktop file name equals the application ID so Wayland
shells match windows to the icon and show the *New Window* action).

## Releases

Pushing a tag `vX.Y.Z` runs `.github/workflows/release.yml`, which builds the
deb, rpm, Flatpak and AppImage and attaches them to the GitHub Release.

## Layout

```
src/engine/   lexer, Pratt parser, evaluator, formatter (pure Rust, unit-tested)
src/history.rs, src/config.rs, src/window.rs, src/main.rs
data/         desktop file, SVG + PNG icons, metainfo, style.css
packaging/    Flatpak manifest
```

MIT licensed. All code written from scratch.
