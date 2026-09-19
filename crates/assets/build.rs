use std::process::Command;

/// Returns `true` if `tool` is on `PATH`.
fn have(tool: &str) -> bool {
    Command::new("sh")
        .args(["-c", &format!("command -v {tool} >/dev/null 2>&1")])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn main() {
    println!("cargo:rerun-if-changed=icons");
    println!("cargo:rerun-if-changed=Makefile");

    // The generated `src/icons.rs` and `fonts/icons.ttf` are committed, so
    // regeneration is optional. Skip it when the toolchain (e.g. the
    // cross-compile docker image) lacks the required tools.
    let missing: Vec<&str> = ["make", "npx", "jq"]
        .into_iter()
        .filter(|t| !have(t))
        .collect();
    if !missing.is_empty() {
        println!(
            "cargo::warning=Skipping icon font generation: missing {}. Using committed assets.",
            missing.join(", ")
        );
        return;
    }

    let status = Command::new("make")
        .arg("all")
        .status()
        .expect("Failed to execute make.");

    if !status.success() {
        println!(
            "cargo::warning=Makefile failed to generate icons. Check the terminal output above."
        );
    }
}
