DIR := ${CURDIR}
VERSION = 0.6.7
BUILD_OUTPUT_DIR = blaulicht-dist
PACKAGE = blaulicht-core
RUST_TARGET_DIR := $(if $(wildcard $(CARGO_TARGET_DIR)),$(CARGO_TARGET_DIR),./target)

.PHONY: cargo-build-armhf cargo-build-armel cargo-build-x64 build-docker-cargo \
		build-web build-archives build-archive-armhf build-archive-armel \
		build-archive-x64 release gh-release version clean

# For cross-compilation to X86_64 Musl
cargo-build-x64:
	docker run -it \
	-v $(RUST_TARGET_DIR):/build \
	-v `pwd`:/root/project \
	-e RUST_MIN_STACK=16777216 \
	blaulicht-cross \
	cargo build --package $(PACKAGE) --release --features=wasmtime --features=audio --features=wayland --features=x11 --target x86_64-unknown-linux-gnu
	cp $(RUST_TARGET_DIR)/x86_64-unknown-linux-gnu/release/blaulicht-core ./$(BUILD_OUTPUT_DIR)/blaulicht-x64

# For cross-compilation to X86_64 Musl (HASWELL)
cargo-build-x64-haswell:
	docker run -it \
	-v $(RUST_TARGET_DIR):/build \
	-v `pwd`:/root/project \
	-e RUSTFLAGS="-C target-cpu=haswell" \
	-e RUST_MIN_STACK=16777216 \
	blaulicht-cross \
	cargo build --package $(PACKAGE) --release --features=wasmtime --features=audio --features=wayland --features=x11 --target x86_64-unknown-linux-gnu
	cp $(RUST_TARGET_DIR)/x86_64-unknown-linux-gnu/release/blaulicht-core ./$(BUILD_OUTPUT_DIR)/blaulicht-x64-haswell

# audio-cargo-build-x64:
# 	docker run -it \
# 	-v $(DIR)/target:/build \
# 	-v `pwd`:/root/project \
# 	blaulicht-cross \
# 	cargo build --release --target x86_64-unknown-linux-gnu

cargo-build-x64-debug:
	docker run -it \
	-v $(RUST_TARGET_DIR):/build \
	-v `pwd`:/root/project \
	blaulicht-cross \
	cargo build --target x86_64-unknown-linux-gnu

build-docker-cargo:
	docker build . -t blaulicht-cross:latest

# For building distributable archive files
prepare-archives:
	mkdir -p dist

build-archives: prepare-archives build-archive-x64
build-archives-debug: prepare-archives build-archive-x64-debug

create-build-dir:
	mkdir -p ./$(BUILD_OUTPUT_DIR)

build-archive-x64: create-build-dir cargo-build-x64 cargo-build-x64-haswell
	tar -cvzf dist/blaulicht-x86_64-all-linux-gnu.tar.gz ./$(BUILD_OUTPUT_DIR)
	rm -rf $(BUILD_OUTPUT_DIR)

build-archive-x64-debug: cargo-build-x64-debug
	mkdir -p ./$(BUILD_OUTPUT_DIR)
	cp $(RUST_TARGET_DIR)/x86_64-unknown-linux-gnu/release/blaulicht-core ./$(BUILD_OUTPUT_DIR)/blaulicht
	tar -cvzf dist/blaulicht-x86_64-unknown-linux-gnu.tar.gz ./$(BUILD_OUTPUT_DIR)
	rm -rf $(BUILD_OUTPUT_DIR)

release: clean build-archives gh-release

# Publish the local release to Github releases
gh-release:
	gh release create v$(VERSION) ./dist/*.tar.gz -F ./CHANGELOG.md -t 'blaulicht v$(VERSION)'

version:
	python3 update_version.py

clean:
	rm -rf $(BUILD_OUTPUT_DIR)
	rm -rf dist
