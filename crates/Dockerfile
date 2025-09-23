# Install Rust toolchain (nightly) for Debian target
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain nightly \
    && rustup target add x86_64-unknown-linux-gnu --toolchain nightly
```

If you previously installed stable, remove it or set nightly as default:

```Dockerfile
<code_block_to_apply_changes_from>
```

Optional (if you need -Z build-std or similar):
```Dockerfile
RUN rustup component add rust-src --toolchain nightly
```