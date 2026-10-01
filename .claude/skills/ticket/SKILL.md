---
name: ticket
description: Work on a ticket from TICKETS/ end to end (claim, implement, verify, finish), or list which tickets are ready when no number is given.
argument-hint: "[NNN]"
disable-model-invocation: true
---

# /ticket $ARGUMENTS

All paths are relative to the repo root (`git rev-parse --show-toplevel`). The session may have started in `crates/core`.

## No argument: show what's ready
If `$ARGUMENTS` is empty, don't implement anything:
1. List `TICKETS/*.md` and read the `Depends on` row of each ticket.
2. Report in three short groups:
   - **Doing**: tickets that are `.doing.md`.
   - **Ready**: `.planned.md` tickets whose dependencies are all `.done.md`, with the ABI-bumping ones marked.
   - **Blocked**: the rest, with the dependencies they are waiting on.
3. Recommend the next ticket, using the order in `TODO.md` and the hotspot rules in `TICKETS/README.md`. Stop there.

## With a number: do ticket $ARGUMENTS
Before anything else, read:
1. `TICKETS/README.md` (workflow, hotspots, rules).
2. The ticket `TICKETS/$ARGUMENTS-*.md`. If there's no match or more than one, stop and say so.
3. `crates/core/AGENTS.md` (build commands, plugin ABI, UI verification recipes).

Then:
- **Dependencies**: every ticket in `Depends on` must be `.done.md`. If one isn't, stop and report.
- **Status**:
  - Already `.done.md`: stop and report.
  - Already `.doing.md`: continue only if the current branch is `ticket/$ARGUMENTS-<slug>`. Otherwise stop and ask.
- **Branch**: create `ticket/$ARGUMENTS-<slug>` from the current branch.
- **Claim**: `git mv` the ticket to `.doing.md`, set its status and link in `TODO.md` to doing, and commit `chore: start ticket $ARGUMENTS`.
- **Scope**: implement only what the Scope section says. Anything else you notice goes into the ticket's Notes section or the Inbox in `TODO.md`, not into the code. If the ticket turns out too big for one session, split it as `TICKETS/README.md` describes instead of cutting corners.
- **Verify** every item in the ticket's Verification section. For UI changes, run the app and look at the screenshots yourself, following `crates/core/AGENTS.md`. If something could not be verified (screen locked, or another `blaulicht-core` you didn't start is running), write that down instead of claiming it.

Rules:
- Never use `git stash`. Don't commit or revert unrelated uncommitted changes (e.g. `SPARTACUS_DRAFT.json`, `crates/core/config.toml`). Stage only the files you changed.
- Run cargo through the flake: `direnv exec . cargo …` from the repo root.
- Never `pkill` a `blaulicht-core` you didn't start. Stop your own by PID.
- If you bump `PLUGIN_ABI_VERSION`:
  - rebuild the plugins with `cd crates/plugins && make build_noopt`;
  - grep that output for `^error`, since the Makefile reports success even when a build fails.
- If you change the showfile format:
  - load a scratch copy of an existing showfile;
  - confirm with `python3 scripts/blctl.py state` that it didn't silently boot a default show.
- Commits use conventional prefixes (`feat:`, `fix:`, `refactor:`, `chore:`) and stay atomic.

When finished:
1. Tick the ticket's acceptance criteria and fill in Notes (decisions, follow-ups, anything unverified).
2. `git mv` the ticket to `.done.md`, update its status and link in `TODO.md`, and commit.
3. Report back:
   - what changed;
   - how it was verified, i.e. which screenshots and commands;
   - any open points.
4. Don't merge the branch. The maintainer reviews and merges it.
