.PHONY: build test install

build:
	cargo build --release

test:
	cargo test

install:
	cargo install --path .
