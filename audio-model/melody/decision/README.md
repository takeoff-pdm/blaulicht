# Should a model decide the show?

**Verdict: no training and no reinforcement learning. If anything, keep the rules as renderer and guard and add a validated per-song plan**, picked by a
beam search or an LLM. Nobody has listened to any of this yet, so `ab.html` is what decides whether a plan is worth having. Details and sources are in `notes.md`.

## What was measured (club-music subset, 175 electronic songs; LLM plans on 40)

- **Plans can't break the show.** A plan picks palette, look, speed and lead per segment, and strobe type, length and rate per drop. Hard-rule violations at 50 fps on
  `showAt` (strobes only from the drop for up to a bar, dark while strobing, periods 1/2/4/8, steps on the grid): 0 for rules, random and beam on 175 songs, and for the LLM on 40.
- **The rules barely vary where it's visible.** The strobe type follows the drop's index, not the music, and strobe length is 4 beats on 496 of 501 drops.
- **Human lighting as a reference.** 38 aligned Beat Saber maps on 25 songs (hobby VR lighting, small sample): per-bar activity against a human is rules .46, another human .41, loudness .49.
  Drop rhythm is rules .58, another human .59, but 448 of 501 drop rhythm profiles are identical for rules and random, so the plan layer doesn't touch rhythm.
- **Humans repeat a song's treatment across its drops and differ between songs** (.78 against .25). The rules do the reverse at strobe level, round robin between drops.
- **The LLM** picks magenta + cyan on 28 of 40 songs, so keep the palette out of its hands. Mean $0.069 per song by the CLI's report, median 23 s.

No published study compares learned lighting with a hand-tuned, beat-locked rule system. The only rule system Skip-BART and SeqLight were tested against is their authors' own emotion-based baseline, on rock and metal shows, shown as colour blocks or simulated lights.

## The listening test

    python3 audio-model/melody/decision/serve_ab.py     # http://127.0.0.1:8768/
    python3 audio-model/melody/decision/analyse_ab.py   # after the trials

48 blind trials, resumable, about 39 minutes of playback: 14 songs, each comparing the rules with a random, a beam and an LLM plan, plus 6 of those again with the sides swapped.

- Under 4 of 6 repeats consistent: stop, the differences aren't audible or visible enough to judge.
- Rules under 11 of 14 against random: build more looks before anything else.
- Candidate 11+ of 14: adopt it. 5 or fewer: reject it.

Fourteen songs only detect large preferences: 11 of 14 passes 12% of the time at a true 60% preference, 36% at 70%, 70% at 80%. Label songs first, so the plans rest on corrected sections.

## Files

| File | What |
|---|---|
| `build_club.py`, `club_plan.js`, `core.cjs`, `harness.cjs` | a patched copy of `viz/club.js` that takes a plan, and the 50 fps check of the hard rules |
| `build_cache.py`, `plan.py`, `plan_llm.py`, `compare_plans.py` | cue cache, validator, random and beam plans, LLM plans, comparison |
| `bs_match.py`, `bs_fetch.py`, `bs_compare.py`, `bs_rhythm.py` | Beat Saber maps of our songs (lighting events only, no audio kept) |
| `make_trials.py`, `serve_ab.py`, `ab.html`, `analyse_ab.py` | the blind A/B test |
| `check_looks.mjs` | every look and toggle of `viz/club_looks.mjs` against the hard rules (see the main README, Looks) |
