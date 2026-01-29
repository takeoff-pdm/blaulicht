all-check:
	RUSTFLAGS=-Awarnings cargo c --message-format short --features=wayland --features=audio --features=wasmtime --features=wayland --features=x11

all-run:
	RUSTFLAGS=-Awarnings cargo r --message-format short --features=wayland --features=audio --features=wasmtime --features=wayland --features=x11

dhat-all-run:
	RUSTFLAGS=-Awarnings cargo r --message-format short --features=wayland --features=audio --features=wasmtime --features=wayland --features=x11 --features=dhat

dhat-wasm-run:
	RUSTFLAGS=-Awarnings cargo r --message-format short --no-default-features  --features=audio-mock --features=wayland --features=wasmtime --features=dhat

prod-run:
	RUSTFLAGS=-Awarnings cargo r -r --message-format short --features=wayland --features=audio --features=wasmtime --features=wayland --features=x11

prod-build:
	RUSTFLAGS=-Awarnings cargo b -r --message-format short --features=wayland --features=audio --features=wasmtime --features=wayland --features=x11

audio-mock-run:
	RUSTFLAGS=-Awarnings cargo r --message-format short --no-default-features --features=wayland --features=audio

mock-run:
	RUSTFLAGS=-Awarnings cargo r --message-format short --no-default-features --features wayland --features=audio-mock

dhat-mock-run:
	RUSTFLAGS=-Awarnings cargo r --message-format short --no-default-features --features wayland --features=audio-mock --features=dhat

mock-check:
	RUSTFLAGS=-Awarnings cargo c --message-format short --no-default-features --features wayland --features=audio-mock
