# art-engine build infrastructure
# On systems without make, use: bash xtask.sh <command>
# See xtask.sh for all available commands.

.PHONY: check test clippy fmt doc build wasm clean

check: fmt clippy test doc
	@echo "All checks passed."

test:
ifdef CRATE
	cargo test -p $(CRATE)
else
	cargo test --all
	cargo test -p art-engine-core --features render
endif

clippy:
	cargo clippy --all -- -D warnings
	cargo clippy -p art-engine-core --features render -- -D warnings

fmt:
	cargo fmt --all -- --check

doc:
	cargo doc --all --no-deps

build:
	cargo build --all
	cargo build -p art-engine-core --features render

wasm:
	cargo build -p art-engine-wasm --target wasm32-unknown-unknown

clean:
	cargo clean
