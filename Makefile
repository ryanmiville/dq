.PHONY: check release

check:
	cargo fmt --all
	cargo clippy --all-targets -- -D warnings
	cargo test

release:
	./scripts/release.sh
