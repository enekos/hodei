.PHONY: all build build-servo test run clean init check fmt clippy help

all: build build-servo

init:
	git submodule update --init --recursive

build:
	cargo build --workspace

build-release:
	cargo build --workspace --release

build-servo:
	cargo build --manifest-path crates/hodei-servo/Cargo.toml

build-servo-release:
	cargo build --manifest-path crates/hodei-servo/Cargo.toml --release

test:
	cargo test --workspace

test-servo:
	cargo test --manifest-path crates/hodei-servo/Cargo.toml

test-all: test test-servo

run:
	cargo run -p hodei-app

check:
	cargo check --workspace
	cargo check --manifest-path crates/hodei-servo/Cargo.toml

fmt:
	cargo fmt --all

clippy:
	cargo clippy --workspace -- -D warnings
	cargo clippy --manifest-path crates/hodei-servo/Cargo.toml -- -D warnings

clean:
	cargo clean
	cargo clean --manifest-path crates/hodei-servo/Cargo.toml

help:
	@echo "Available targets:"
	@echo "  init              Initialize git submodules"
	@echo "  build             Build workspace crates (debug)"
	@echo "  build-release     Build workspace crates (release)"
	@echo "  build-servo       Build the Servo facade (debug)"
	@echo "  build-servo-release Build the Servo facade (release)"
	@echo "  all               Build workspace + Servo facade"
	@echo "  test              Run workspace tests"
	@echo "  test-servo        Run Servo facade tests"
	@echo "  test-all          Run all tests"
	@echo "  run               Run hodei-app"
	@echo "  check             Run cargo check on all crates"
	@echo "  fmt               Format all Rust code"
	@echo "  clippy            Run clippy on all crates"
	@echo "  clean             Clean build artifacts"
	@echo "  help              Show this help message"
