# Decision models: research notes (2026-11-05)

## Code reading: what the rules actually decide (club.js)
- palette: key -> circle-of-fifths -> 1 of 6 pairs (keyPalette). Not mood or style. One per song.
- look per segment: animOf(melody stats) -> 5 patterns (wave, cross, fan, circle, sweep), never twice in a row; hook overrides; breakdown/intro/outro forced to wave/circle; build forced to rise.
- period: melody density thresholds per 2-bar block -> 1,2,4,8 beats; preset energy > .6 halves, calm > .5 doubles; kind scales.
- level: table per section kind x loudness.
- strobe type per drop: TYPES[i % 3] (index, NOT music!), `all` for longest breakdown >= 8 bars.
- strobe length: low-end within 6 dB of first beat, 1..4 beats ("a full bar for all 71 drops in first 40 songs" README -> effectively constant).
- colour lead: (hook|part|seg + rootLead) mod 2.
So the decision surface is small and categorical, compile-time; action space per song ~ 6 x (5*4*2)^segments x (4*4)^drops. The two things the user cares about, "differ between songs and drops", are decided by key lookup and drop index.

## Data check
- SPARTACUS_SOLI.json: format_version 4 show file: 15 fixture groups, 4 scenes (OFF, WHITE, STATIC, FOG-ON), 9 animations (sine phasers, flash, wasm plugins), 3 palettes. No timeline/cue/song/recording: a rig + scene setup, not a recorded show. visualizer/tests/fixtures/mini-show.json: same format, test fixture.
- labels: only queue.txt so far. No recorded shows in repo.
- Beat Saber public maps: human lighting events (see below). NEW data source.

## Published evidence (READ = I opened the primary)
- Skip-BART (Zhao et al., ICLR 2026) READ v2 html. 38 raters (9 pro), 3 x 20 s clips, rendered as "dynamic color block", overall: GT 4.51, Skip-BART 4.35, rule 2.67 (p<.001), GT vs SB p=.724. Rule baseline = authors' own reconstruction of Hsiao 2017 (BiLSTM on DEAM -> emotion -> hue/value); no release of its patterns; authors note "may introduce some unfairness". Data 699 samples from 35 live shows, rock/punk/metal/core. Labels = histogram of video. Limits: "fails to maintain the long-term rhythmic stability", no rhythm or section input.
- SeqLight (Zhao et al., arXiv 2605.03660, NeurIPS 2026 per repo) READ html + appendix. 30 raters (7 pro), 6 pieces (3 ID, 3 OOD from Suno), colour block; ID overall: Ours 4.54, GT 4.38, SB 3.90, rule 2.70. OOD: 3.94, 3.47, 2.70. ours vs SB OOD p=.226. Preference ID: 42.2 / 40.0 / 16.7 / 1.1 %. Simulated 8 point lights. Same rule baseline. No blinding statement. No objective beat metric. Same group as Skip-BART, same dataset.
- Own pilot contradicts on club music (phase lock, drop rise, seed variance).
- LightingGen (IEEE 11397196) only snippet; WebFetch empty. NOT read.
- Hsiao 2017 (Inf Sci): 403, only the snippet.
- No published comparison of learned lighting vs a hand-tuned beat-locked rule system. No published study with real fixtures in room found.

## Analogies
- EDGE (CVPR 2023) READ ar5iv: 147 raters, 11,610 pairs; win 91.1% vs Bailando, 90.0% vs FACT, 65.7% vs GT. "Metrics do not represent human quality": FID worse for GT than generated; BAS higher than GT. AIST++ 1,408 motions. Lesson: objective metrics fail; pairwise preference works, but with data and many raters.
- DanceChat (2506.10574) READ: LLM writes textual instruction from structured music captions (tempo, chords, key, beat); 30 raters x 20 pairs: preferred over FACT 86.25, Bailando 61.88, EDGE 60.00, GT 48.12%; in the wild vs EDGE 66.36%. No sig tests. LLM guides a diffusion model; analogy only.
- ChoreoMaster (SIGGRAPH 2021): project page READ, gives no numbers. 3dvf blog: 35 participants, 10 choreographers (secondary). Embedding + graph search with choreographic rules, "production-ready" - closest analogue of rules + search.
- Leake 2017 (Computational video editing) page snippet: idioms as HMMs linking edit decisions to labels; 2-3 s. Analogue of HMM over looks. Evaluation not read.
- MusicRL (ICML 2024) snippet + abstract: 300,000 pairwise preferences from users -> RL; preferred 74% vs base. Shows scale RLHF used in music.
- Christiano 2017 READ abstract: "about an hour of human time", "<1%" of interactions; ~900 queries for backflip from secondary. Needs a simulator-trained policy and a task with learnable reward; not read in full.

## Preference learning
- CPBO (Iwai, Koyama, Goto et al. 2025, IJCAI) READ pdf text: constrained PBO; 11 pro designers; 12D + 6D; 50 pairwise per task; avg 4.2 s per judgement. Constraints handled by design. Static images!
- Sequential Gallery (Koyama 2020) READ pdf text: 12D photo task, 10D body shape; 5 of 6 satisfied in < 15 iterations; plane-search.
- Brochu 2007, 2010: snippet only (fluid animation parameters).
- Key difference: our stimulus takes ~20-60 s, not 4 s, and is song dependent.
- None on lighting.

## Commercial
- rekordbox Lighting (READ FAQ, Pioneer news): phrase analysis (Intro/Up/Down/Chorus/Bridge/Verse/Outro) + mood HIGH/MID/LOW -> Scene with Bank (COOL, NATURAL, HOT, SUBTLE, WARM, VIVID, CLUB 1, CLUB 2). User may edit by Macro Editor. Table of phrase -> scene not published.
- SoundSwitch (READ support pages): Autoscript analyzes audio, beatgrid, phrase detection, presets and styles; stores script as metadata; algorithm undocumented.
- MaestroDMX manual V1.5 Jan 2026 READ: real time; "Activity Level" = excitement; kick detection required; user picks pattern ("vibes" lowest -> highest energy: Still, Beam, Ambient, Chill, Energetic, Dance, Party) + palette group; "moving through the different colors in the group based on transitions in the music"; "Autonomous Strobing" recipe = snapshot on kick when Activity Range 80-85. "Hands-off autonomous decision-making network" is claimed, internals undocumented.
- Pattern: all documented products = audio analysis -> section/energy class -> preset look + human override. None publishes a learned decision or evaluation. MyStrow, SOH snippet only. xLights autosequencer and MCP servers: snippets.
- LLM cue list: only github TilanTAB/DMX-Light-Show (0 stars; README read; cue list per section, no evaluation or schema documented); 2026 trade blog says drafting tool (low authority). No peer-reviewed LLM lighting found.

## Beat Saber (new data source)
- bsmg.wiki basic lighting READ: on/flash/fade/off per light type, boost swaps colour pair, "if you emphasize everything, you emphasize nothing".
- ScoreSaber criteria: ranked maps must have sufficient lighting throughout (snippet).
- BeatSaver API works; Darude Sandstorm 849a: curated+ranked, 13,998 events in 224 s.
- beatformer_v5 HF: lights_v2 dedicated lighter; 57,007 map IDs in manifests; held CE lights .571; no human eval.
- afgonczol/beat-saber-auto-mapper: rule fallback when ML sparse, no evaluation.

## Experiments (all on the Mac, CPU; ae-charlie and the GPUs not used)
Code in audio-model/melody/decision/.

### Plan layer
build_club.py patches a copy of club.js (club_plan.js, core.cjs): plan picks palette (6), per segment look (5) / speed (3) / lead (2), per drop strobe type (4) / beats (1,2,4, can only shorten) / rate (2,4). No plan = rules, unchanged. plan.py: validate, random control, beam on hand-written score. plan_llm.py: claude -p, opus, no tools, cue table only.
Hard rules at 50 fps on showAt (harness.cjs check): strobes only in [drop, drop+strobeBeats), never over a bar, dark while strobing, periods 1/2/4/8, steps on grid. Fixture code, rope flash code, patterns unchanged, so 30% flashes and few lights hold by construction (not separately measured at fixture level).
Violations: 0 in rules (175), random (175), beam (175), llm (40). LLM validator problems before repair: 0 of 40 songs.
Beat lock with the pilot's formula on a proxy brightness (not fixture output): rules 0.238, random .247, beam .243, llm .251; detuned .006. Skip-BART pilot .008-.015.
Rule facts: strobe beats = 4 on 496 of 501 drops; type sequence only 22 distinct over 172 songs.
Descriptive (rules on 175 / rules on LLM's 40 / random on 40 / beam 175 / beam 40 / llm 40):
- distinct drop sequences per 100 songs: 14.9 / 47.5 / 97.5 / 38.9 / 72.5 / 82.5
- palette entropy bits: 2.54 / 2.39 / 2.55 / 2.46 / 2.26 / 1.33 (LLM picks magenta+cyan on 28 of 40)
- size vs breakdown Spearman: .49 / .55 / .07 / .51 / .49 / .51; size vs low jump: .09 / .07 / .11 / .22 / .15 / .22 (beam: by construction)
- returning letter same look (after something else): .44 / .51 / .24 / .52 / .57 / .41
LLM: median 23 s, mean $0.069 per song as the CLI reports it; repeats over 6 songs x 3 runs: same palette 78% (chance 17), same look 52% (chance 20), same strobe type 83%. Rationales cite table values; I read 6 songs' rationales, none invented an instrument, one uses title knowledge (Flames -> red). Not a systematic check.

### Human reference: Beat Saber maps on our songs
bs_match: 51 of 175 electronic songs have a non-automapped map with >= 200 lighting events (strict matching; more may exist); 27 with 2+. Fetched 96, kept 38 maps on 25 songs after alignment (peak >= .5, sharp >= 12, on-16th share >= .6; median 96% on our 16ths after a constant shift, chance 41%); 11 songs with pairs.
Per-bar activity Spearman vs a human (median): loudness .49, rules level .46, rules brightness .43, drums+bass .42, another human .41 (15 pairs), gradient boosting on unseen songs .41 (better than rules brightness on 39.5% of 38 maps, median diff -.04), shifted control -.07.
Look change F1: rules' segment starts .26, random same count .14, another human .33 (14 pairs).
Rhythm after drop, bars 2-4, Pearson over 16ths: rules show .58, another human .59 (30 pairs), human from other song .10, rules vs other song .29; pulses alone .53. n=88. Rules, random, beam are IDENTICAL here: 448 of 501 drop profiles do not change between rules and random, because sub is 4 for periods 1-2 and pulses come from drums. The plan layer doesn't touch rhythm.
Drops (n=89 map-drops): activity up in 73%, median x1.21; hit on downbeat +-30 ms 69%; quiet last half beat 65% (the rules' blackout).
Treatment similarity (which group on which 16th, first 4 bars): same map other drop .78 (same letter .88, other letter .77), other song .25. Rhythm only: same lighter other drop .94, two lighters same drop .81, other song .54.
Reading: music fixes rhythm (two people agree); lighters repeat a song's treatment over its drops and differ between songs. The rules do the reverse at strobe level (round robin between drops, few sequences across songs).
Limits: hobby VR lighting, different fixtures, small n, activity is coarse; GB trained on 25 songs only; nothing here says good.

### A/B (needs the user)
serve_ab.py + ab.html, port 8768. 48 blind trials: 14 songs (first with 2+ drops in the label queue) x 3 comparisons + 6 swapped repeats. Each side plays 1 bar before + 6 bars after two drops. 39 min playback at one listen. Tested end to end in a muted tab: plan applied, keys work, answer stored, next trial loads, no console errors; test answer removed, tab closed, server stopped.
Power: P(>=11 of 14) = .12 at true 60% preference, .36 at 70%, .70 at 80%, .85 at 85%, .96 at 90%: only large preferences pass, and rule 2 leans to stopping.

Decision Transformer / bandits on lighting: one search found nothing published. Patents and building-lighting RL only (snippets).
