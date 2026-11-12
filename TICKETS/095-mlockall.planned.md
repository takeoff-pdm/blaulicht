# 095 — Lock Blaulicht's memory (`mlockall`) on the show host

| | |
|---|---|
| Epic | Ops / show host (latency) |
| Depends on | none |
| ABI / showfile | none |

**Touches:** `crates/core/src/main.rs` (or engine startup), `crates/engine/src/config.rs`, `install/limits.conf`

## Problem
`bl` has 3.7 GB RAM and 2.9 GB swap. If memory gets tight (large showfile, audio models, `/tmp` on tmpfs), pages of
the running show can be swapped out; touching them again stalls the thread for milliseconds. `install/tune.sh` lowers
`vm.swappiness` to 10, which makes it less likely but doesn't rule it out. The `blaulicht` user's `memlock` limit is
8 MB (`ulimit -l` on `bl`), so `mlockall` would fail today.

## Scope
- At startup on Linux, call `mlockall(MCL_CURRENT | MCL_FUTURE)` (optionally `MCL_ONFAULT` to avoid committing all
  virtual memory at once), behind a `config.toml` key `lock_memory` (default off, on for `bl`).
- On failure, log a warning with the reason and continue.
- `install/limits.conf`: `blaulicht - memlock unlimited` (or a fixed large value).
- Check the resident size first (`/proc/<pid>/status` VmRSS on `bl` during a show) and that locking it leaves enough RAM;
  note the number. The binary is ~950 MB on disk (debug info?), so `MCL_CURRENT` without `MCL_ONFAULT` may lock more
  than expected.

## Acceptance criteria
- [ ] With `lock_memory = true` on `bl`, `/proc/<pid>/status` shows `VmLck` ≈ `VmRSS`, and the system stays
      responsive.
- [ ] Without the key, nothing changes. Failure logs a warning only.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- On `bl`: `grep -E 'VmRSS|VmLck' /proc/$(pgrep -x blaulicht)/status`, `free -h` after an hour of a show.

## Notes
