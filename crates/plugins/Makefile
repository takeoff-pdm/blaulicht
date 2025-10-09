# SED:=sed
# SED_PACKAGE_PLACEHOLDER:=placeholder

# CARGO:=cargo
# TARGET_TRIPLE:=wasm32-unknown-unknown

# COMPILATION_MODE:=release

# IN_FILE:=input.rs
# OUT_FILE:=output.wasm

# # Provided externally
# # HASH:=placeholder
# CRATE:=blaulicht_plugins

# .PHONY: target

# # prepare:
# # 	$(SED) -e "s/$(SED_PACKAGE_PLACEHOLDER)/$(CRATE)/g" Cargo.toml > Cargo.toml

# target:
# 	# Add to file with prelude
# 	# cat input.rs >> src/input.rs

# 	$(CARGO) build --target=$(TARGET_TRIPLE) --$(COMPILATION_MODE) --target-dir=target
# 	cp ./target/$(TARGET_TRIPLE)/$(COMPILATION_MODE)/$(CRATE).wasm $(OUT_FILE).temp
# 	wasm-opt -o $(OUT_FILE) -O4 --strip-debug $(OUT_FILE).temp
# 	rm $(OUT_FILE).temp

# build_noopt:
# 	$(CARGO) build --target=$(TARGET_TRIPLE) --target-dir=target
# 	cp ./target/$(TARGET_TRIPLE)/debug/$(CRATE).wasm $(OUT_FILE)

# build: target


# Find immediate subdirectories that contain a Cargo.toml
CRATES := $(shell find . -maxdepth 2 -mindepth 2 -name Cargo.toml -not -path "./bundle/*" -exec dirname {} \; | sed 's|^\./||')
TARGET_TRIPLE:=wasm32-unknown-unknown

# Default target
all: build_noopt

.PHONY: build_noopt build_opt clean

build_noopt:
	mkdir -p bundle_debug
	@for crate in $(CRATES); do \
		echo "Building $$crate..."; \
		(cd $$crate && RUSTFLAGS=-Awarnings cargo build --message-format short --target=$(TARGET_TRIPLE) --target-dir=target && cp ./target/$(TARGET_TRIPLE)/debug/*.wasm ../bundle_debug/$$crate.wasm) \
	done
	@echo "All crates built successfully!"

build_opt:
	mkdir -p bundle_release
	@for crate in $(CRATES); do \
		echo "Building $$crate..."; \
		(cd $$crate && RUSTFLAGS=-Awarnings cargo build --message-format short --target=$(TARGET_TRIPLE) --release --target-dir=target && cp ./target/$(TARGET_TRIPLE)/release/*.wasm ../bundle_release/$$crate.wasm) \
	done
	@echo "All crates built successfully!"

clean:
	@for crate in $(CRATES); do \
		echo "Cleaning $$crate..."; \
		( cd $$crate && cargo clean ) || exit 1; \
	done
	@echo "All crates cleaned!"

.PHONY: all build clean
