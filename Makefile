DIR := ${CURDIR}
VERSION = 0.1.0
BUILD_OUTPUT_DIR = blaulicht-$(VERSION)

.PHONY: cargo-build-armhf cargo-build-armel cargo-build-x64 build-docker-cargo \
		build-web build-archives build-archive-armhf build-archive-armel \
		build-archive-x64 release gh-release version clean

# For cross-compilation to X86_64 Musl
cargo-build-x64:
	docker run -it \
	-v $(DIR)/target:/build \
	-v `pwd`:/root/project \
	blaulicht-cross \
	cargo build --release --target x86_64-unknown-linux-gnu

build-docker-cargo:
	docker build . -t blaulicht-cross:latest

# For building distributable archive files
prepare-archives:
	mkdir -p dist
build-archives: prepare-archives build-archive-x64


build-archive-x64: cargo-build-x64
	cp ./target/x86_64-unknown-linux-gnu/release/blaulicht-core ./$(BUILD_OUTPUT_DIR)
	tar -cvzf dist/blaulicht-$(VERSION)-x86_64-unknown-linux-gnu.tar.gz ./$(BUILD_OUTPUT_DIR)
	rm -rf $(BUILD_OUTPUT_DIR)

release: clean build-archives gh-release

# Publish the local release to Github releases
gh-release:
	gh release create v$(VERSION) ./dist/*.tar.gz -F ./CHANGELOG.md -t 'blaulicht v$(version)'

version:
	python3 update_version.py

clean:
	rm -rf $(BUILD_OUTPUT_DIR)
	rm -rf dist
