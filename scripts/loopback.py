#!/usr/bin/env python3
from __future__ import annotations
import argparse
import platform
import shutil
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Optional

VIRTUAL_SINK_NAME = "Virtual-Out"
VIRTUAL_SINK_DESCRIPTION = "Virtual-Out"
VIRTUAL_MIC_NAME = "Virtual-Mic"
VIRTUAL_MIC_DESCRIPTION = "Virtual-Mic"
CONFIG_FILENAME = "config.txt"
CONFIG_PATH = Path(__file__).resolve().parent / CONFIG_FILENAME


@dataclass(frozen=True)
class SinkInfo:
    index: int
    name: str
    description: str


@dataclass(frozen=True)
class OutputConfig:
    sink: str
    volume: str
    node_name: str


def list_sink_infos() -> list[SinkInfo]:
    result = run_pactl("list", "sinks", capture_output=True)
    sinks: list[SinkInfo] = []
    current_index: Optional[int] = None
    current_name: Optional[str] = None
    current_description: Optional[str] = None

    for raw_line in result.stdout.splitlines():
        line = raw_line.strip()
        if line.startswith("Sink #"):
            if current_index is not None and current_name:
                sinks.append(
                    SinkInfo(
                        index=current_index,
                        name=current_name,
                        description=current_description or current_name,
                    )
                )
            current_index = int(line.partition("#")[2])
            current_name = None
            current_description = None
        elif line.startswith("Name:"):
            current_name = line.partition(":")[2].strip()
        elif line.startswith("Description:"):
            current_description = line.partition(":")[2].strip()

    if current_index is not None and current_name:
        sinks.append(
            SinkInfo(
                index=current_index,
                name=current_name,
                description=current_description or current_name,
            )
        )

    return sinks


def ensure_linux() -> None:
    if platform.system() != "Linux":
        sys.exit("This script only supports Linux.")


def ensure_pipewire_available() -> None:
    candidates = ("pipewire", "pw-cli", "pw-top", "pw-cat", "pipewire-pulse")
    if not any(shutil.which(cmd) for cmd in candidates):
        sys.exit("PipeWire binaries were not found. Ensure PipeWire is installed and available in PATH.")


def ensure_command_available(command: str) -> None:
    if shutil.which(command) is None:
        sys.exit(f"Required command '{command}' not found in PATH.")


def run_pactl(*args: str, capture_output: bool = False) -> subprocess.CompletedProcess:
    return subprocess.run(
        ("pactl",) + args,
        check=True,
        text=True,
        capture_output=capture_output,
    )


def find_sink_input_id(tag: str) -> Optional[str]:
    result = run_pactl("list", "short", "sink-inputs", capture_output=True)
    for line in result.stdout.splitlines():
        if tag in line:
            return line.split()[0]
    return None


def parse_volume(raw_volume: str) -> str:
    if raw_volume.endswith("%"):
        numeric_part = raw_volume[:-1]
        if numeric_part.isdigit():
            return raw_volume
    elif raw_volume.isdigit():
        return raw_volume
    sys.exit(f"Invalid volume value '{raw_volume}'. Use either an integer or a percentage (e.g., 32768 or 50%).")


def load_output_config(path: Path) -> list[OutputConfig]:
    if not path.exists():
        sys.exit(f"Configuration file '{path}' not found.")

    outputs: list[OutputConfig] = []
    with path.open(encoding="utf-8") as config_file:
        for idx, raw_line in enumerate(config_file, start=1):
            line = raw_line.strip()
            if not line or line.startswith("#"):
                continue
            parts = line.split()
            if len(parts) < 3:
                sys.exit(f"Invalid configuration on line {idx} in '{path}'. Expected format: <sink_name> <volume> <node_name>.")
            sink_name, volume_token, node_name = parts[0], parse_volume(parts[1]), parts[2]
            outputs.append(OutputConfig(sink=sink_name, volume=volume_token, node_name=node_name))

    if not outputs:
        sys.exit(f"No output routes defined in '{path}'.")

    return outputs


def prompt_with_default(prompt: str, default: str) -> str:
    response = input(f"{prompt} [{default}]: ").strip()
    return response or default


def prompt_confirmation(prompt: str, default: bool = False) -> bool:
    suffix = "Y/n" if default else "y/N"
    while True:
        response = input(f"{prompt} ({suffix}): ").strip().lower()
        if not response:
            return default
        if response in {"y", "yes"}:
            return True
        if response in {"n", "no"}:
            return False
        print("Please answer 'y' or 'n'.")


def sanitize_node_name(sink_name: str, suffix: int) -> str:
    base = "".join(char if char.isalnum() else "_" for char in sink_name).strip("_")
    if not base:
        base = "VirtualLoop"
    return f"{base}_{suffix}"


def generate_config_interactive(config_path: Path) -> None:
    print("🔧 Generating PipeWire routing configuration.")
    ensure_linux()
    ensure_pipewire_available()
    ensure_command_available("pactl")

    sinks = list_sink_infos()
    if not sinks:
        sys.exit("No sinks found via pactl. Cannot generate configuration.")

    print("\nAvailable sinks:")
    for idx, sink in enumerate(sinks, start=1):
        print(f"  {idx:>2}. {sink.description} ({sink.name})")

    created_configs: list[OutputConfig] = []
    route_counter = 1
    while True:
        selection = input("\nSelect sink by number (or type 'done' to finish): ").strip().lower()
        if selection in {"done", "d"}:
            break
        if not selection:
            print("Please enter a number or 'done'.")
            continue
        if not selection.isdigit():
            print("Invalid selection. Enter the number shown in the list.")
            continue
        index = int(selection) - 1
        if index < 0 or index >= len(sinks):
            print("Selection out of range. Try again.")
            continue
        sink = sinks[index]

        default_volume = "65536"
        raw_volume = input(
            f"Set volume for '{sink.description}' (integer level or percentage, default {default_volume}): "
        ).strip()
        if raw_volume:
            volume = parse_volume(raw_volume)
        else:
            volume = default_volume

        suggested_node = sanitize_node_name(sink.name, route_counter)
        node_name = prompt_with_default(f"Loopback node name for '{sink.description}'", suggested_node)

        created_configs.append(OutputConfig(sink=sink.name, volume=volume, node_name=node_name))
        route_counter += 1

        if not prompt_confirmation("Add another route?", default=True):
            break

    if not created_configs:
        sys.exit("No routes selected. Configuration not written.")

    if config_path.exists():
        if not prompt_confirmation(f"Configuration file '{config_path}' exists. Overwrite?", default=False):
            sys.exit("Aborted; configuration not overwritten.")

    config_path.parent.mkdir(parents=True, exist_ok=True)
    with config_path.open("w", encoding="utf-8") as config_file:
        config_file.write(f"# Generated by {Path(__file__).name}\n")
        config_file.write("# sink_name volume node_name\n")
        for route in created_configs:
            config_file.write(f"{route.sink} {route.volume} {route.node_name}\n")

    print(f"\n✅ Configuration written to {config_path}")


def apply_routes(config_path: Path) -> None:
    ensure_linux()
    ensure_pipewire_available()
    ensure_command_available("pactl")
    output_configs = load_output_config(config_path)

    run_pactl(
        "load-module",
        "module-null-sink",
        f"sink_name={VIRTUAL_SINK_NAME}",
        f"sink_properties=device.description={VIRTUAL_SINK_DESCRIPTION}",
    )
    time.sleep(1)

    for output in output_configs:
        run_pactl(
            "load-module",
            "module-loopback",
            "source=Virtual-Out.monitor",
            f"sink={output.sink}",
            f"sink_input_properties=node.name={output.node_name}",
        )
        time.sleep(0.5)

        sink_input = find_sink_input_id(output.node_name)
        if sink_input:
            run_pactl("set-sink-input-volume", sink_input, output.volume)
        else:
            print(f"Warning: sink input for node '{output.node_name}' not found; volume unchanged.")

    run_pactl(
        "load-module",
        "module-remap-source",
        "master=Virtual-Out.monitor",
        f"source_name={VIRTUAL_MIC_NAME}",
        f"source_properties=device.description={VIRTUAL_MIC_DESCRIPTION}",
    )

    print("✅ Virtual-Out + Virtual-Mic setup complete.")
    print("   → Output device: Virtual-Out")
    print("   → Input device:  Virtual-Mic")
    print("   → Routed outputs:")
    for output in output_configs:
        print(f"      - sink '{output.sink}' via node '{output.node_name}' at volume {output.volume}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Configure PipeWire loopbacks for virtual audio routing.",
    )
    parser.add_argument(
        "command",
        nargs="?",
        choices=("apply", "generate-config"),
        default="apply",
        help="Command to run. Defaults to 'apply'.",
    )
    parser.add_argument(
        "-c",
        "--config",
        type=Path,
        default=CONFIG_PATH,
        help=f"Path to configuration file (default: {CONFIG_PATH})",
    )
    return parser.parse_args()


if __name__ == "__main__":
    try:
        arguments = parse_args()
        config_path = Path(arguments.config).expanduser().resolve()
        if arguments.command == "generate-config":
            generate_config_interactive(config_path)
        else:
            apply_routes(config_path)
    except subprocess.CalledProcessError as error:
        sys.exit(f"Command failed: {' '.join(error.cmd)}")
