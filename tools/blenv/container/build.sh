#!/usr/bin/env bash
# Build blaulicht-core and/or plugins inside the blenv container.
#   build.sh [core|plugins|all] [plugin-dir...]
set -euo pipefail
cd /work

FEATURES="${BLENV_FEATURES:-x11 audio wasmtime}"
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
    plugins) build_plugins ;;
    all) build_plugins && build_core ;;
    *) echo "usage: build.sh [core|plugins|all] [plugin-dir...]" >&2; exit 2 ;;
esac
