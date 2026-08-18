#!/usr/bin/env python3
"""
Migrate a pre-FixtureValue showfile to the current schema.

Old fixture state:
    {"color": {"h": h, "s": s, "v": v},
     "alpha": a, "orientation": {"pan": p, "tilt": t},
     "strobe_speed": s, "focus": f}

New fixture state (each slot is a FixtureValue enum, serialized as {"Literal": u16}):
    {"color_h": {"Literal": ...}, "color_s": {"Literal": ...}, ...}

Usage:
    python3 migrate_showfile.py <input.json> [output.json]
"""
import json
import sys
from pathlib import Path


def lit(v: int) -> dict:
    return {"Literal": int(v) & 0xFFFF}


def migrate_fixture_state(old: dict) -> dict:
    color = old.get("color", {})
    orient = old.get("orientation", {})
    h = float(color.get("h", 0.0))
    s = float(color.get("s", 0.0))
    v = float(color.get("v", 0.0))
    h_u16 = int(max(0.0, min(360.0, h)))
    s_u16 = int(max(0.0, min(1.0, s)) * 255.0)
    v_u16 = int(max(0.0, min(1.0, v)) * 255.0)
    return {
        "color_h": lit(h_u16),
        "color_s": lit(s_u16),
        "color_v": lit(v_u16),
        "alpha": lit(old.get("alpha", 0)),
        "pan": lit(orient.get("pan", 0)),
        "tilt": lit(orient.get("tilt", 0)),
        "strobe_speed": lit(old.get("strobe_speed", 0)),
        "focus": lit(old.get("focus", 0)),
    }


def migrate_animation_spec(spec: dict) -> None:
    body = spec.get("body", {})
    phaser = body.get("Phaser")
    if phaser is None:
        return
    kind = phaser.get("kind", {})
    math = kind.get("Mathematical")
    if math is None:
        return
    for key in ("amplitude_min", "amplitude_max"):
        val = math.get(key)
        if isinstance(val, int):
            math[key] = lit(val)


def migrate(doc: dict) -> dict:
    engine = doc.get("engine", {})
    for tmpl in engine.get("animations", []):
        migrate_animation_spec(tmpl.get("value", {}).get("spec", {}))
    for scene_entry in engine.get("scenes", []):
        sink = scene_entry.get("value", {}).get("sink", {})
        for fs in sink.get("fixture_states", []):
            fs["value"] = migrate_fixture_state(fs["value"])
        for sel_entry in sink.get("active_animations", []):
            for anim_entry in sel_entry.get("value", []):
                migrate_animation_spec(anim_entry.get("value", {}).get("spec", {}))
    return doc


def main() -> int:
    if len(sys.argv) < 2 or len(sys.argv) > 3:
        print(__doc__, file=sys.stderr)
        return 2
    src = Path(sys.argv[1])
    dst = Path(sys.argv[2]) if len(sys.argv) == 3 else src.with_suffix(".migrated.json")
    doc = json.loads(src.read_text())
    dst.write_text(json.dumps(migrate(doc), indent=2))
    print(f"Wrote {dst}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
