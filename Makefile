PREFIX ?= $(HOME)/.local
APP_ID = io.github.uri_travoski.Calc

build:
	cargo build --release

run:
	cargo run --release

test:
	cargo test

install: build
	install -Dm755 target/release/sam-calc $(PREFIX)/bin/sam-calc
	install -Dm644 data/$(APP_ID).desktop $(PREFIX)/share/applications/$(APP_ID).desktop
	install -Dm644 data/$(APP_ID).metainfo.xml $(PREFIX)/share/metainfo/$(APP_ID).metainfo.xml
	install -Dm644 data/$(APP_ID).svg $(PREFIX)/share/icons/hicolor/scalable/apps/$(APP_ID).svg
	for s in 48 128 256 512; do install -Dm644 data/icons/$(APP_ID)-$$s.png $(PREFIX)/share/icons/hicolor/$${s}x$${s}/apps/$(APP_ID).png; done
	-gtk-update-icon-cache -q -t $(PREFIX)/share/icons/hicolor
	-update-desktop-database -q $(PREFIX)/share/applications
	# a per-user desktop file must call the binary by absolute path
	sed -i 's|^Exec=sam-calc|Exec=$(PREFIX)/bin/sam-calc|' $(PREFIX)/share/applications/$(APP_ID).desktop

deb: build
	cargo deb --no-build

rpm: build
	cargo generate-rpm

.PHONY: build run test install deb rpm
