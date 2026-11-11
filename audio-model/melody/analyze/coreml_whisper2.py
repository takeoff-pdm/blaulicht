"""faster-whisper on Core ML, version 2: runs the models from ane/ane_decoder2.py (not yet measured).

Same CTranslate2 semantics as coreml_whisper.py (generate / detect_language / align are unchanged in
meaning); what changes is how the decoder is called:
  - cross-attention keys/values: one `fill` call per half and window, written into state on the device
    (v1: CrossKV + fp32 copies + write_state from Python, ~0.9 s per window);
  - the prompt (<|startoftranscript|> <|en|> <|transcribe|>) in one `multi` call instead of 3 steps;
  - the word-alignment pass in ceil(len / 64) `multi` calls instead of one step per token (v1 spent about
    half of its decoder time here);
  - generation stays one `step` per token, now with int8 weights.
Select with MELODY_WHISPER_BACKEND=coreml2 (lyrics.setup).
"""

import json
import time
from pathlib import Path

import numpy as np

from analyze.coreml_whisper import (C, CTX, FRAMES, FRAMES_STATE, VOCAB, AlignmentResult, CoreMLWhisper, EncoderOutput,
                                    _median_filter, _negative_dtw, _softmax)

NEG = -10000.0


class CoreMLWhisper2(CoreMLWhisper):
    def __init__(self, folder, ct2_folder, tokenizer, units="ane"):
        import coremltools as ct

        cu = {"ane": ct.ComputeUnit.CPU_AND_NE, "gpu": ct.ComputeUnit.CPU_AND_GPU, "cpu": ct.ComputeUnit.CPU_ONLY}[units]
        folder = Path(folder)
        meta = json.loads((folder / "ane_decoder.json").read_text())
        assert meta.get("version") == 2, "not an ane_decoder2.py build"
        self.multi = meta["multi"]
        self.encoder = ct.models.CompiledMLModel(str(folder / "AudioEncoder.mlmodelc"), compute_units=cu)
        self.halves = []
        for _, _, name in meta["halves"]:
            fns = {f: ct.models.CompiledMLModel(str(folder / f"{name}.mlmodelc"), compute_units=cu, function_name=f)
                   for f in ("fill", "step", "multi")}
            self.halves.append((fns, fns["step"].make_state()))  # one state, shared by the three functions
        self.tok_emb = np.memmap(folder / "token_embedding.f16", np.float16, "r", shape=(VOCAB, C))
        self.pos_emb = np.fromfile(folder / "position_embedding.f16", np.float16).reshape(CTX, C).astype(np.float32)
        cfg = json.loads((Path(ct2_folder) / "config.json").read_text())
        self.lang_ids, self.suppress_ids_begin, self.suppress_ids = cfg["lang_ids"], cfg["suppress_ids_begin"], cfg["suppress_ids"]
        tid = tokenizer.token_to_id
        self.sot, self.eot, self.no_ts = tid("<|startoftranscript|>"), tid("<|endoftext|>"), tid("<|notimestamps|>")
        self.no_speech, self.ts_begin = tid("<|nospeech|>"), tid("<|notimestamps|>") + 1
        self.lang_tokens = [tokenizer.id_to_token(i) for i in self.lang_ids]
        self._window = None
        self._enc = np.zeros((1, C, 1, FRAMES_STATE), np.float16)
        self._pos = 0  # next cache position of the generation sequence
        self.stats = dict(encodes=0, windows=0, steps=0, multi_calls=0, encode_s=0.0, cross_s=0.0, step_s=0.0,
                          multi_s=0.0, beam_requested=0)

    def _use(self, eo):
        if self._window is eo:
            return
        t = time.perf_counter()
        self._enc[..., :FRAMES] = np.asarray(eo.embeds, np.float16).reshape(1, C, 1, FRAMES)
        for fns, st in self.halves:
            fns["fill"].predict({"encoder_output": self._enc}, state=st)
        self._window = eo
        self.stats["windows"] += 1
        self.stats["cross_s"] += time.perf_counter() - t

    def _run(self, tokens, start):
        """Tokens at cache positions start.. (one call per half). Returns logits [n, VOCAB] fp32, align [10, n, FRAMES]."""
        n = len(tokens)
        fn = "step" if n == 1 else "multi"
        S = 1 if n == 1 else self.multi
        assert n <= S and start + n <= CTX
        t = time.perf_counter()
        x = np.zeros((1, C, 1, S), np.float32)
        x[0, :, 0, :n] = (self.tok_emb[tokens].astype(np.float32) + self.pos_emb[start: start + n]).T
        um = np.zeros((1, S, CTX), np.float16)
        am = np.full((1, S, CTX), NEG, np.float16)
        am[0, :, 0] = 0  # padding rows attend to position 0 only (no NaN); their output is ignored
        for r in range(n):
            um[0, r, start + r] = 1
            am[0, r, : start + r + 1] = 0  # earlier positions (cache) and itself (causal)
        h, aligns = x.astype(np.float16), []
        for fns, st in self.halves:
            o = fns[fn].predict({"x": h, "update_mask": um, "attn_mask": am}, state=st)
            h = o.get("x_out", o.get("logits"))
            if o["align"].ndim == 4 and o["align"].shape[2] == S:
                aligns.append(o["align"].reshape(-1, S, FRAMES)[:, :n])
        logits = np.asarray(h, np.float32).reshape(VOCAB, S)[:, :n].T
        key = "steps" if n == 1 else "multi_calls"
        self.stats[key] += 1
        self.stats["step_s" if n == 1 else "multi_s"] += time.perf_counter() - t
        return logits, np.concatenate(aligns, 0)

    def _step(self, token, pos):
        logits, align = self._run([token], pos)
        return logits[0], align[:, 0]

    def _prefill(self, prompt):
        logits = []
        for i in range(0, len(prompt), self.multi):
            logits.append(self._run(prompt[i: i + self.multi], i)[0])
        return np.concatenate(logits, 0)

    def detect_language(self, encoder_output):
        self._use(encoder_output)
        logits, _ = self._step(self.sot, 0)
        p = _softmax(logits[self.lang_ids])
        return [[(self.lang_tokens[i], float(p[i])) for i in np.argsort(-p)]]

    def generate(self, encoder_output, prompts, length_penalty=1.0, max_length=448, return_scores=False,
                 return_no_speech_prob=False, suppress_blank=True, suppress_tokens=(-1,), max_initial_timestamp_index=50,
                 beam_size=1, sampling_topk=1, sampling_temperature=1.0, num_hypotheses=1, **_):
        if len(prompts) != 1:
            raise NotImplementedError("batch size 1 only")
        if beam_size == 1 and sampling_topk != 1 and sampling_temperature > 0:
            raise NotImplementedError("temperature sampling is not implemented (use temperature=0)")
        if beam_size > 1:
            self.stats["beam_requested"] += 1
        self._use(encoder_output)
        prompt = list(prompts[0])
        suppress = [t for t in suppress_tokens if t >= 0] + (list(self.suppress_ids) if -1 in suppress_tokens else [])
        suppress = np.array(sorted(set(suppress)), int)
        limit = min(max_length, CTX)
        pl = self._prefill(prompt)
        no_speech = float(_softmax(pl[prompt.index(self.sot)])[self.no_speech]) if self.sot in prompt else 0.0
        logits = pl[-1]
        tokens, cum = [], 0.0
        while len(prompt) + len(tokens) < limit:
            l = logits.copy()
            if not tokens and suppress_blank:
                l[self.suppress_ids_begin] = -np.inf
            if len(suppress):
                l[suppress] = -np.inf
            self._timestamp_rules(l, tokens, max_initial_timestamp_index)
            nxt = int(np.argmax(l))
            lse = np.logaddexp.reduce(l[np.isfinite(l)])
            cum += float(l[nxt] - lse)
            if nxt == self.eot:
                break
            tokens.append(nxt)
            if len(prompt) + len(tokens) >= limit:
                break
            logits, _ = self._step(nxt, len(prompt) + len(tokens) - 1)
        score = cum / (len(tokens) ** length_penalty) if tokens else cum
        from analyze.coreml_whisper import GenerationResult
        return [GenerationResult([tokens], [score], no_speech)]

    def align(self, encoder_output, start_sequence, text_tokens, num_frames, median_filter_width=7):
        self._use(encoder_output)
        nf = int(num_frames if np.isscalar(num_frames) else num_frames[0]) // 2
        start = list(start_sequence)
        results = []
        for toks in text_tokens:
            seq = start + [self.no_ts] + list(toks) + [self.eot]
            ls, rows = [], []
            for i in range(0, len(seq), self.multi):
                lg, a = self._run(seq[i: i + self.multi], i)
                ls.append(lg)
                rows.append(a[:, :, :nf])
            logits, w = np.concatenate(ls, 0), np.concatenate(rows, 1).astype(np.float32)  # w [heads, tokens, frames]
            probs = [float(_softmax(logits[len(start) + k, : self.eot])[t]) for k, t in enumerate(toks)]
            w = (w - w.mean(1, keepdims=True)) / w.std(1, keepdims=True)
            w = _median_filter(w, median_filter_width).mean(0)
            results.append(AlignmentResult(_negative_dtw(w[len(start): -1]), probs))
        return results


def load_model(folder, units="ane"):
    import analyze.coreml_whisper as v1

    orig = v1.CoreMLWhisper
    v1.CoreMLWhisper = CoreMLWhisper2  # v1.load_model builds the WhisperModel shell around this class
    try:
        return v1.load_model(folder, units)
    finally:
        v1.CoreMLWhisper = orig
