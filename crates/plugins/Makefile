SED:=sed
SED_PACKAGE_PLACEHOLDER:=placeholder

CARGO:=cargo
TARGET_TRIPLE:=wasm32-unknown-unknown

COMPILATION_MODE:=release

IN_FILE:=input.rs
OUT_FILE:=output.wasm

# Provided externally
# HASH:=placeholder
CRATE:=blaulicht_plugins

.PHONY: target

# prepare:
# 	$(SED) -e "s/$(SED_PACKAGE_PLACEHOLDER)/$(CRATE)/g" Cargo.toml > Cargo.toml

target:
	# Add to file with prelude
	# cat input.rs >> src/input.rs

	$(CARGO) build --target=$(TARGET_TRIPLE) --$(COMPILATION_MODE) --target-dir=target
	cp ./target/$(TARGET_TRIPLE)/$(COMPILATION_MODE)/$(CRATE).wasm $(OUT_FILE).temp
	wasm-opt -o $(OUT_FILE) -O4 --strip-debug $(OUT_FILE).temp
	rm $(OUT_FILE).temp

build_noopt:
	$(CARGO) build --target=$(TARGET_TRIPLE) --target-dir=target
	cp ./target/$(TARGET_TRIPLE)/debug/$(CRATE).wasm $(OUT_FILE)

build: target
