# blenv: headless X environment for building, running and driving blaulicht-core.
# Tooling only; the source tree is bind-mounted at /work by tools/blenv/blenv.py.
FROM debian:trixie

ARG RUST_NIGHTLY=nightly

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates curl git build-essential pkg-config clang python3 procps \
        libasound2-dev libudev-dev libdbus-1-dev \
        xvfb openbox x11vnc xdotool imagemagick x11-utils mesa-utils \
        libgl1-mesa-dri libegl1 libgl1 libglx-mesa0 \
        libx11-6 libx11-xcb1 libxcursor1 libxi6 libxrandr2 libxkbcommon-x11-0 libxcb1 \
        fonts-dejavu-core \
    && rm -rf /var/lib/apt/lists/*

# rustup lives in /opt so the shared /cargo volume only holds registry/git caches.
ENV RUSTUP_HOME=/opt/rustup \
    PATH=/opt/cargo/bin:${PATH}
RUN curl -sSf https://sh.rustup.rs | CARGO_HOME=/opt/cargo sh -s -- -y --profile minimal \
        --default-toolchain ${RUST_NIGHTLY} \
    && CARGO_HOME=/opt/cargo rustup target add wasm32-unknown-unknown \
    && CARGO_HOME=/opt/cargo rustup component add rust-src clippy rustfmt \
    && chmod -R a+rX /opt/rustup /opt/cargo

# RUSTUP_TOOLCHAIN beats rust-toolchain.toml, so a pinned date is used instead of rustup
# fetching whatever "nightly" is today.
ENV RUSTUP_TOOLCHAIN=${RUST_NIGHTLY} \
    CARGO_HOME=/cargo \
    CARGO_TARGET_DIR=/build/target \
    DISPLAY=:9 \
    LIBGL_ALWAYS_SOFTWARE=1 \
    RUST_MIN_STACK=16777216

WORKDIR /work
CMD ["/work/tools/blenv/container/entrypoint.sh"]
