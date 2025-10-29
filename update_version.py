#!/usr/bin/env python3
# This file is used to update the version number in all relevant places
# The SemVer (https://semver.org) versioning system is used.
import re

makefile_path = "Makefile"
cargo_toml_paths = [
        "./crates/core/Cargo.toml",
        "./crates/shared/Cargo.toml",
        "./crates/plugin_framework/Cargo.toml",
]

path = cargo_toml_paths[0]
with open(path, "r") as file:
    content = file.read()
    old_version = content.split("version = \"")[1].split("\"\n")[0]
    print(f"Found old version in {path}: {old_version}")

try:
    VERSION = input(
        f"Current version: {old_version}\nNew version (without 'v' prefix): ")
except KeyboardInterrupt:
    print("\nCanceled by user")
    quit()

if VERSION == "":
    VERSION = old_version

if not re.match(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-((?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)(?:\.(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*))?(?:\+([0-9a-zA-Z-]+(?:\.[0-9a-zA-Z-]+)*))?$", VERSION):
    print(
        f"\x1b[31mThe version: '{VERSION}' is not a valid SemVer version.\x1b[0m")
    quit()


for path in cargo_toml_paths:
    with open(path, "r") as file:
        content = file.read()
    with open(path, "w") as file:
        file.write(content.replace(old_version, VERSION))


# Makefile
with open(makefile_path, "r") as file:
    content = file.read()
    old_version = content.split("VERSION = ")[1].split("\n")[0]
    print(f"Found old version in {makefile_path}:", old_version)

    with open(makefile_path, "w") as file:
        file.write(content.replace(old_version, VERSION))

print(f"Version has been changed from '{old_version}' -> '{VERSION}'")
