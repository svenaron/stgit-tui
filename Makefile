.PHONY: build check clippy fmt lint

build:
	cargo build

clippy:
	cargo clippy -- --deny warnings

fmt:
	cargo fmt --all -- --check

lint: fmt clippy

check: lint build
