RUSTFLAGS = -C target-cpu=native -Awarnings
export

RARGS:=
	
all-check:
	cargo c --message-format short --features=wayland --features=audio --features=wasmtime --features=wayland --features=x11

all-run:
	cargo r --message-format short --features=wayland --features=audio --features=wasmtime --features=wayland --features=x11

dhat-all-run:
	cargo r --message-format short --features=wayland --features=audio --features=wasmtime --features=wayland --features=x11 --features=dhat

dhat-all-run-prod:
	cargo r -r --message-format short --features=wayland --features=audio --features=wasmtime --features=wayland --features=x11 --features=dhat

dhat-wasm-run:
	cargo r --message-format short --no-default-features  --features=audio-mock --features=wayland --features=wasmtime --features=dhat

prod-run:
	cargo r -r --message-format short --features=wayland --features=audio --features=wasmtime --features=wayland --features=x11 $(RARGS)

prod-build:
	cargo b -r --message-format short --features=wayland --features=audio --features=wasmtime --features=wayland --features=x11

audio-mock-run:
	cargo r --message-format short --no-default-features --features=wayland --features=audio

wasm-mock-run:
	cargo r --message-format short --no-default-features --features=wayland --features=audio-mock --features=wasmtime

mock-run:
	cargo r --message-format short --no-default-features --features wayland --features=audio-mock $(RARGS)

dhat-mock-run:
	cargo r --message-format short --no-default-features --features wayland --features=audio-mock --features=dhat

mock-check:
	cargo c --message-format short --no-default-features --features wayland --features=audio-mock
