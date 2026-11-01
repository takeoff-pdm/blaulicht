#!/usr/bin/env bash
# Build blaulicht-core (the desk), blaulicht-headless, the web UI and/or plugins inside the blenv container.
#   build.sh [core|headless|web|plugins|all] [plugin-dir...]
set -euo pipefail
cd /work

FEATURES="${BLENV_FEATURES:-x11 audio wasmtime}"
# The headless binary is the engine crate's: same features minus the windowing ones.
HEADLESS_FEATURES="${BLENV_HEADLESS_FEATURES:-$(tr ' ' '\n' <<<"$FEATURES" | grep -vxE 'x11|wayland' | xargs)}"
DEFAULT_PLUGINS=(
    inspector
    julia_animation_plugin
    audio_reactor_animation_plugin
    chaos_animation_plugin
    sample_egui_plugin
)

what="${1:-all}"
[[ $# -gt 0 ]] && shift
plugins=("$@")
[[ ${#plugins[@]} -eq 0 ]] && plugins=("${DEFAULT_PLUGINS[@]}")

build_core() {
    echo "==> blaulicht-core (features: $FEATURES)"
    cargo build -p blaulicht-core --no-default-features --features "$FEATURES"
}

build_headless() {
    echo "==> blaulicht-headless (features: $HEADLESS_FEATURES)"
    cargo build -p blaulicht-engine --bin blaulicht-headless --no-default-features \
        --features "$HEADLESS_FEATURES"
}

# The web UI bundle (crates/web/build.sh --lint) into $CARGO_TARGET_DIR/web, where the
# engine in $CARGO_TARGET_DIR/debug finds it. Installs wasm-bindgen-cli (the exact version
# in Cargo.lock, prebuilt, into the cargo volume) on first use.
build_web() {
    local want have
    want=$(awk '$0 == "name = \"wasm-bindgen\"" { getline; gsub(/"/, "", $3); print $3; exit }' Cargo.lock)
    have=$("$CARGO_HOME/bin/wasm-bindgen" --version 2>/dev/null | awk '{ print $2 }' || true)
    if [[ $have != "$want" ]]; then
        echo "==> installing wasm-bindgen-cli $want into $CARGO_HOME/bin"
        local tmp name
        tmp=$(mktemp -d)
        name="wasm-bindgen-$want-$(uname -m)-unknown-linux-gnu"
        if curl -fsSL "https://github.com/wasm-bindgen/wasm-bindgen/releases/download/$want/$name.tar.gz" \
            | tar -xz -C "$tmp"; then
            mkdir -p "$CARGO_HOME/bin"
            install -m 755 "$tmp/$name/wasm-bindgen" "$CARGO_HOME/bin/wasm-bindgen"
        else
            cargo install wasm-bindgen-cli --version "$want" --locked --root "$CARGO_HOME"
        fi
        rm -rf "$tmp"
    fi
    echo "==> web UI"
    crates/web/build.sh --lint
}

build_plugins() {
    local out=crates/plugins/bundle_debug failed=()
    mkdir -p "$out"
    for dir in "${plugins[@]}"; do
        local manifest="crates/plugins/$dir/Cargo.toml"
        if [[ ! -f $manifest ]]; then
            echo "error: no plugin crate at $manifest" >&2
            failed+=("$dir")
            continue
        fi
        echo "==> plugin $dir"
        local name
        name=$(cargo metadata --manifest-path "$manifest" --no-deps --format-version 1 \
            | python3 -c 'import json,sys; print(json.load(sys.stdin)["packages"][0]["name"].replace("-","_"))')
        if RUSTFLAGS="-C target-feature=+simd128 -Awarnings" cargo build \
            --manifest-path "$manifest" --target wasm32-unknown-unknown --target-dir /build/plugins; then
            cp "/build/plugins/wasm32-unknown-unknown/debug/$name.wasm" "$out/$dir.wasm"
        else
            failed+=("$dir")
        fi
    done
    if [[ ${#failed[@]} -gt 0 ]]; then
        echo "error: plugin build failed: ${failed[*]}" >&2
        return 1
    fi
}

case "$what" in
    core) build_core ;;
    headless) build_headless ;;
    web) build_web ;;
    plugins) build_plugins ;;
    all) build_plugins && build_core ;;
    *) echo "usage: build.sh [core|headless|web|plugins|all] [plugin-dir...]" >&2; exit 2 ;;
esac
