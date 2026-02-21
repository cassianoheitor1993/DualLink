# DualLink Makefile
# Usage:
#   make          → build (debug) + launch
#   make build    → swift build debug
#   make release  → swift build release
#   make run      → build + launch (same as run_app.sh)
#   make install  → release build → /Applications/DualLink.app
#   make clean    → remove .build/ and DualLink.app

.PHONY: all build release run install clean

all: run

build:
	swift build -c debug

release:
	swift build -c release

run: build
	./run_app.sh

install:
	./install.sh

clean:
	rm -rf .build DualLink.app
