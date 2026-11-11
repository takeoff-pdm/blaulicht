"""faster-whisper on Core ML (Whisper large-v3 on the Apple Neural Engine), version 1.

faster-whisper's own pipeline (features, segmenting, timestamp handling, filters, word timestamps) runs
unchanged; only its CTranslate2 model object is replaced by CoreMLWhisper, which runs the models built by
ane/ane_decoder.py (+ the split AudioEncoder; all ops on the Neural Engine):
  AudioEncoder (WhisperKit's encoder split in two halves under the Neural Engine's 1 GiB limit),
  CrossKV (cross-attention keys/values, once per 30 s window), DecoderA/DecoderB (16 layers each, with
  the self-attention cache and cross-attention keys/values in Core ML state), embedding tables.
Measured 2026-11-09 on Stereo Love: 64.8 s/song (+37 s load), text 91.8% / timing 88% within 50 ms of ae-charlie.
CoreMLWhisper reproduces CTranslate2's Whisper semantics:
  generate: suppress_blank (config suppress_ids_begin), suppress_tokens, timestamp rules (pairs, no
    decrease, first token a timestamp <= max_initial_timestamp_index, timestamp mass vs best text token),
    <|notimestamps|> suppressed, no_speech_prob = softmax at the <|startoftranscript|> step, score =
    cumulative log prob (incl. <|endoftext|>) / len ** length_penalty.
    Greedy only: a requested beam search (faster-whisper's beam_size=5) is decoded greedily.
  detect_language: softmax over the language tokens at the <|startoftranscript|> step.
  align: teacher-forced pass over start + <|notimestamps|> + text + <|endoftext|>, text token
    probabilities over the text vocabulary, alignment-head cross-attention probabilities cut to the
    audio's frames, normalised over tokens per head and frame, median filter (reflect) over frames,
    mean over heads, DTW (ties go to the horizontal move), as in CTranslate2.
"""

import json
import os
import time
from pathlib import Path

import numpy as np

C, CTX, FRAMES, FRAMES_STATE, VOCAB = 1280, 448, 1500, 1536, 51866


class EncoderOutput:
    __slots__ = ("embeds",)

    def __init__(self, embeds):
        self.embeds = embeds


class GenerationResult:
    def __init__(self, sequences_ids, scores, no_speech_prob):
        self.sequences_ids, self.scores, self.no_speech_prob = sequences_ids, scores, no_speech_prob


class AlignmentResult:
    def __init__(self, alignments, text_token_probs):
        self.alignments, self.text_token_probs = alignments, text_token_probs


def _log_softmax(x):
    m = np.max(x)
    if not np.isfinite(m):
        return np.full_like(x, -np.inf)
    y = x - m
    return y - np.log(np.sum(np.exp(y)))


def _softmax(x):
    return np.exp(_log_softmax(x))


def _median_filter(x, width):
    """Median over the last axis with reflect padding (CTranslate2's MedianFilter / openai)."""
    r = width // 2
    if x.shape[-1] <= r:
        return x
    p = np.pad(x, [(0, 0)] * (x.ndim - 1) + [(r, r)], mode="reflect")
    return np.median(np.lib.stride_tricks.sliding_window_view(p, width, axis=-1), axis=-1)


def _negative_dtw(matrix):
    """CTranslate2's negative_dtw: DTW on -matrix, ties go to the horizontal move; (text, time) pairs."""
    x = -matrix.astype(np.float32)
    n, m = x.shape
    cost = np.full((n + 1, m + 1), np.inf, np.float32)
    trace = np.full((n + 1, m + 1), -1, np.int8)
    cost[0, 0] = 0
    for d in range(2, n + m + 1):  # anti-diagonals: every cell depends only on earlier diagonals
        i = np.arange(max(1, d - m), min(n, d - 1) + 1)
        j = d - i
        c0, c1, c2 = cost[i - 1, j - 1], cost[i - 1, j], cost[i, j - 1]
        t = np.where((c0 < c1) & (c0 < c2), 0, np.where((c1 < c0) & (c1 < c2), 1, 2)).astype(np.int8)
        cost[i, j] = x[i - 1, j - 1] + np.choose(t, [c0, c1, c2])
        trace[i, j] = t
    trace[0, :] = 2
    trace[:, 0] = 1
    i, j, out = n, m, []
    while i > 0 or j > 0:
        out.append((i - 1, j - 1))
        t = trace[i, j]
        if t == 0:
            i, j = i - 1, j - 1
        elif t == 1:
            i -= 1
        elif t == 2:
            j -= 1
        else:
            raise ValueError("unexpected DTW trace")
    return out[::-1]


class CoreMLWhisper:
    """Stands in for ctranslate2.models.Whisper (batch 1, greedy)."""

    is_multilingual = True
    n_mels = 128
    device = "cpu"
    device_index = [0]

    def __init__(self, folder, ct2_folder, tokenizer, units="ane"):
        import coremltools as ct

        cu = {"ane": ct.ComputeUnit.CPU_AND_NE, "gpu": ct.ComputeUnit.CPU_AND_GPU, "cpu": ct.ComputeUnit.CPU_ONLY}[units]
        folder = Path(folder)
        load = lambda name: ct.models.CompiledMLModel(str(folder / f"{name}.mlmodelc"), compute_units=cu)
        self.encoder, self.cross = load("AudioEncoder"), load("CrossKV")
        self.dec_a, self.dec_b = load("DecoderA"), load("DecoderB")
        self.state_a, self.state_b = self.dec_a.make_state(), self.dec_b.make_state()
        self.tok_emb = np.memmap(folder / "token_embedding.f16", np.float16, "r", shape=(VOCAB, C))
        self.pos_emb = np.fromfile(folder / "position_embedding.f16", np.float16).reshape(CTX, C)
        cfg = json.loads((Path(ct2_folder) / "config.json").read_text())
        self.lang_ids, self.suppress_ids_begin, self.suppress_ids = cfg["lang_ids"], cfg["suppress_ids_begin"], cfg["suppress_ids"]
        tid = tokenizer.token_to_id
        self.sot, self.eot, self.no_ts = tid("<|startoftranscript|>"), tid("<|endoftext|>"), tid("<|notimestamps|>")
        self.no_speech, self.ts_begin = tid("<|nospeech|>"), tid("<|notimestamps|>") + 1
        self.lang_tokens = [tokenizer.id_to_token(i) for i in self.lang_ids]
        self._window = None
        self._buf = {n: np.zeros((1, 16 * C, 1, FRAMES_STATE), np.float32) for n in ("cross_k", "cross_v")}
        self._kpm = np.full((1, CTX), -10000, np.float16)
        self.stats = dict(encodes=0, windows=0, steps=0, encode_s=0.0, cross_s=0.0, step_s=0.0, beam_requested=0)

    # --- model calls -------------------------------------------------------------------------------
    def encode(self, features, to_cpu=False):
        f = np.asarray(features, dtype=np.float32)
        if f.ndim == 2:
            f = f[None]
        assert f.shape[0] == 1 and f.shape[1] == self.n_mels and f.shape[2] == 3000, f.shape
        t = time.perf_counter()
        out = self.encoder.predict({"melspectrogram_features": f.reshape(1, self.n_mels, 1, 3000).astype(np.float16)})
        self.stats["encodes"] += 1
        self.stats["encode_s"] += time.perf_counter() - t
        return EncoderOutput(out["encoder_output_embeds"])

    def _use(self, eo):
        if self._window is eo:
            return
        t = time.perf_counter()
        kv = self.cross.predict({"encoder_output_embeds": eo.embeds})
        for st, k, v in ((self.state_a, "cross_k_a", "cross_v_a"), (self.state_b, "cross_k_b", "cross_v_b")):
            for name, out in (("cross_k", k), ("cross_v", v)):
                np.copyto(self._buf[name][..., :FRAMES], kv[out])
                st.write_state(name, self._buf[name])
        self._window = eo
        self.stats["windows"] += 1
        self.stats["cross_s"] += time.perf_counter() - t

    def _step(self, token, pos):
        """One decoder position: logits (fp32, [VOCAB]) and the 10 alignment heads' rows ([10, FRAMES])."""
        t = time.perf_counter()
        x = (self.tok_emb[token].astype(np.float32) + self.pos_emb[pos].astype(np.float32)).astype(np.float16)
        if pos == 0:
            self._kpm.fill(-10000)
        self._kpm[0, pos] = 0
        um = np.zeros((1, CTX), np.float16)
        um[0, pos] = 1
        oa = self.dec_a.predict({"x": x.reshape(1, C, 1, 1), "update_mask": um, "key_padding_mask": self._kpm}, state=self.state_a)
        ob = self.dec_b.predict({"x": oa["x_out"], "update_mask": um, "key_padding_mask": self._kpm}, state=self.state_b)
        self.stats["steps"] += 1
        self.stats["step_s"] += time.perf_counter() - t
        align = np.concatenate([oa["align"].reshape(-1, FRAMES), ob["align"].reshape(-1, FRAMES)], 0)
        return ob["logits"].reshape(-1).astype(np.float32), align

    # --- CTranslate2 Whisper API -------------------------------------------------------------------
    def detect_language(self, encoder_output):
        self._use(encoder_output)
        logits, _ = self._step(self.sot, 0)
        p = _softmax(logits[self.lang_ids])
        return [[(self.lang_tokens[i], float(p[i])) for i in np.argsort(-p)]]

    def _timestamp_rules(self, l, tokens, max_initial):
        tb = self.ts_begin
        l[self.no_ts] = -np.inf
        if tokens:
            last_ts = tokens[-1] >= tb
            pen_ts = len(tokens) < 2 or tokens[-2] >= tb
            if last_ts:
                if pen_ts:
                    l[tb:] = -np.inf          # has to be non-timestamp
                else:
                    l[: self.eot] = -np.inf   # cannot be normal text tokens
            ts = [t for t in tokens if t >= tb]
            if ts:  # timestamps shouldn't decrease (and segments need a nonzero length)
                last = ts[-1] if (last_ts and not pen_ts) else ts[-1] + 1
                l[tb:last] = -np.inf
        else:
            l[:tb] = -np.inf                  # first token is a timestamp
            if max_initial is not None:
                l[tb + max_initial + 1:] = -np.inf
        lp = _log_softmax(l)
        if np.logaddexp.reduce(lp[tb:]) > np.max(lp[:tb]):
            l[:tb] = -np.inf

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
        logits, no_speech = None, 0.0
        for i, tok in enumerate(prompt):
            logits, _ = self._step(tok, i)
            if tok == self.sot:
                no_speech = float(_softmax(logits)[self.no_speech])
        tokens, cum = [], 0.0
        while len(prompt) + len(tokens) < limit:
            l = logits.copy()
            if not tokens and suppress_blank:
                l[self.suppress_ids_begin] = -np.inf
            if len(suppress):
                l[suppress] = -np.inf
            self._timestamp_rules(l, tokens, max_initial_timestamp_index)
            nxt = int(np.argmax(l))
            cum += float(_log_softmax(l)[nxt])
            if nxt == self.eot:
                break
            tokens.append(nxt)
            if len(prompt) + len(tokens) >= limit:
                break
            logits, _ = self._step(nxt, len(prompt) + len(tokens) - 1)
        score = cum / (len(tokens) ** length_penalty) if tokens else cum
        return [GenerationResult([tokens], [score], no_speech)]

    def align(self, encoder_output, start_sequence, text_tokens, num_frames, median_filter_width=7):
        self._use(encoder_output)
        nf = int(num_frames if np.isscalar(num_frames) else num_frames[0]) // 2  # encoder stride 2
        start = list(start_sequence)
        results = []
        for toks in text_tokens:
            seq = start + [self.no_ts] + list(toks) + [self.eot]
            rows, probs = [], []
            for i, tok in enumerate(seq):
                logits, a = self._step(tok, i)
                rows.append(a[:, :nf])
                k = i - len(start)  # position i predicts seq[i + 1]; text token k is predicted at len(start) + k
                if 0 <= k < len(toks):
                    probs.append(float(_softmax(logits[: self.eot])[toks[k]]))
            w = np.stack(rows, 1).astype(np.float32)  # [heads, tokens, frames]
            w = (w - w.mean(1, keepdims=True)) / w.std(1, keepdims=True)
            w = _median_filter(w, median_filter_width).mean(0)
            results.append(AlignmentResult(_negative_dtw(w[len(start): -1]), probs))
        return results


def load_model(folder, units="ane"):
    """A faster_whisper.WhisperModel whose CTranslate2 model is replaced by CoreMLWhisper."""
    import tokenizers
    from faster_whisper import WhisperModel
    from faster_whisper.feature_extractor import FeatureExtractor
    from faster_whisper.utils import download_model, get_logger

    ct2 = download_model("large-v3", local_files_only=True)  # tokenizer, preprocessor and config only
    cls = CoreMLWhisper  # looked up at call time so coreml_whisper2 can swap in its class

    class CoreMLWhisperModel(WhisperModel):
        def __init__(self):
            self.logger = get_logger()
            self.hf_tokenizer = tokenizers.Tokenizer.from_file(os.path.join(ct2, "tokenizer.json"))
            self.model = cls(folder, ct2, self.hf_tokenizer, units)
            self.feat_kwargs = self._get_feature_kwargs(ct2, None)
            self.feature_extractor = FeatureExtractor(**self.feat_kwargs)
            self.input_stride = 2
            self.num_samples_per_token = self.feature_extractor.hop_length * self.input_stride
            self.frames_per_second = self.feature_extractor.sampling_rate // self.feature_extractor.hop_length
            self.tokens_per_second = self.feature_extractor.sampling_rate // self.num_samples_per_token
            self.time_precision = 0.02
            self.max_length = 448

    return CoreMLWhisperModel()
