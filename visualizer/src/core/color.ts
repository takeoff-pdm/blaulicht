/**
 * Colour helpers mirroring `crates/shared/src/color.rs`.
 *
 * The visualiser works in 8-bit RGB because that is what almost every fixture
 * profile puts on the wire. Hue-based profiles (derbies, multi-FX bars) are the
 * exception, so we keep an HSV -> RGB path for them.
 */

export interface RGB {
  r: number;
  g: number;
  b: number;
}

/** `h` in degrees 0..360, `s`/`v` in 0..1. */
export function hsvToRgb(h: number, s: number, v: number): RGB {
  const hue = ((h % 360) + 360) % 360;
  const c = v * s;
  const x = c * (1 - Math.abs(((hue / 60) % 2) - 1));
  const m = v - c;

  let rp = 0;
  let gp = 0;
  let bp = 0;
  if (hue < 60) [rp, gp, bp] = [c, x, 0];
  else if (hue < 120) [rp, gp, bp] = [x, c, 0];
  else if (hue < 180) [rp, gp, bp] = [0, c, x];
  else if (hue < 240) [rp, gp, bp] = [0, x, c];
  else if (hue < 300) [rp, gp, bp] = [x, 0, c];
  else [rp, gp, bp] = [c, 0, x];

  return {
    r: Math.round((rp + m) * 255),
    g: Math.round((gp + m) * 255),
    b: Math.round((bp + m) * 255),
  };
}

/** Maps a DMX byte in `from` onto `to`, clamping out-of-range inputs.
 *  Port of `map_range_u8` in `crates/shared/src/fixture/mod.rs`. */
export function mapRangeU8(
  v: number,
  from: [number, number],
  to: [number, number],
): number {
  const fromSpan = from[1] - from[0];
  if (fromSpan === 0) return to[0];
  const clamped = Math.min(Math.max(v, from[0]), from[1]) - from[0];
  const toSpan = to[1] - to[0];
  return Math.trunc((clamped * toSpan) / fromSpan) + to[0];
}
