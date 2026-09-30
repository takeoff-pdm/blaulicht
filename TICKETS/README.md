# Tickets

One Markdown file per unit of work. Each ticket is sized so that **one agent session** can implement,
verify and finish it. The index is in the root [`TODO.md`](../TODO.md).

## Naming and status
- File name: `NNN-slug.<status>.md`, for example `014-palette-pointer-color-fix.planned.md`.
  - `NNN`: three digits, never reused. New tickets take the next free number.
  - `slug`: lowercase kebab-case, never changed after creation.
  - `<status>`: exactly one of `planned`, `doing`, `done`.
- To change a status, rename the file with `git mv`. Update the status column in `TODO.md` in the same change:
  ```sh
  git mv TICKETS/014-palette-pointer-color-fix.planned.md TICKETS/014-palette-pointer-color-fix.doing.md
  ```
- Filter by status: `ls TICKETS/*.doing.md`.
- Links in `TODO.md` point at the current file name, so update the link when you rename.

## Workflow for agents
1. **Take the assigned ticket.** Tickets are assigned by the maintainer, not picked by agents. Every ticket in its
   `Depends on` must already be `.done.md`; if one isn't, stop and report.
2. **Claim it as a commit:** `git mv` the ticket to `.doing.md`, update its status in `TODO.md`, and commit
   (`chore: start ticket NNN`) before touching code. Renames in an uncommitted worktree are invisible to other
   agents. If the ticket is already `.doing.md` and you were not told to continue it, stop and ask.
3. **Check for conflicts** with other `.doing.md` tickets (see Hotspots below). Only one agent at a time may run the
   app (inspector port 9099, headless display `:9` and `pkill -x blaulicht-core` are all shared). If `pgrep -a blaulicht-core`
   shows an instance you did not start, don't kill it; ask the maintainer or wait until it's gone.
4. **Implement** only what the Scope section says. Anything you notice outside the scope goes into the ticket's
   Notes or as a new entry in the `TODO.md` Inbox, not into this change.
5. **Verify** every item in the Verification section. UI changes need screenshots the agent looks at itself
   (see `crates/core/AGENTS.md`). If a check could not be run, write that in Notes instead of claiming it.
6. **Finish**: tick the acceptance criteria, write Notes (decisions and follow-ups), and rename to `.done.md`.
7. **Too big?** Split it: keep the original number for the first part, create new tickets with the next free numbers
   for the rest, update `Depends on` and the index. Don't leave a half-finished ticket as `.done.md`.

## Hotspots (don't run these in parallel)
- **Plugin ABI**: tickets that bump `PLUGIN_ABI_VERSION` (`crates/shared/src/abi.rs`) must run one at a time,
  because two bumps from the same base version collide. They are currently 005, 013, 015, 016, 019 and 020, and
  possibly 006, 010, 011, 014, 024 and 029 (see each ticket's ABI line). After a bump, run
  `cd crates/plugins && make build_noopt` and grep the output for `^error`, because the Makefile reports success even when a build fails.
- **Showfile shape**: new fields need `#[serde(default)]`. Load a scratch copy of an existing showfile and confirm with
  `python3 scripts/blctl.py state` that it did not silently boot a default show.
- **Shared UI files**: `crates/core/src/app/pages/fixtures_setup.rs` and `app/components/fixtures_shared.rs` are touched by
  006, 007, 011, 019 and 024. After 006 lands they are split into `app/pages/setup/`.
- **Enums**: append new `ControlEvent` / `MainUiEvent` / `AppPage` variants **at the end** to keep the bincode indices stable.

## Ticket template
```markdown
# NNN — Title

| | |
|---|---|
| Epic | E? Name |
| Depends on | none / NNN, NNN |
| ABI / showfile | none / ABI bump / showfile shape change |

**Touches:** `path/one.rs`, `path/two.rs`

## Problem
Why this ticket exists. Quote the original note verbatim if there is one.

## Scope
- What to build or change, concretely.

## Out of scope
- What belongs to other tickets (name them).

## Key code
- `function_or_type` — `path/to/file.rs:line`

## Acceptance criteria
- [ ] Observable, checkable outcome.

## Verification
- `cargo clippy --all-targets --all-features`, `cargo fmt`, `cargo test --all-features`.
- ABI bump + `make build_noopt` if shared wire types changed.
- UI: screenshots per `crates/core/AGENTS.md` (what to capture).

## Notes
Implementation notes, decisions, follow-ups.
```
