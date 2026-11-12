//! Embeds the git commit (`BLAULICHT_GIT_COMMIT`, "<short hash>, <commit date>")
//! for System › General › About. Empty when git or the repository is missing.

use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

fn main() {
    // Rebuild when HEAD moves: HEAD itself (branch switch) and the ref it points to.
    for path in ["HEAD", "packed-refs"] {
        if let Some(p) = git(&["rev-parse", "--git-path", path]) {
            println!("cargo:rerun-if-changed={p}");
        }
    }
    if let Some(r) = git(&["symbolic-ref", "-q", "HEAD"]) {
        if let Some(p) = git(&["rev-parse", "--git-path", &r]) {
            println!("cargo:rerun-if-changed={p}");
        }
    }
    let commit = git(&["log", "-1", "--format=%h, %cs"]).unwrap_or_default();
    println!("cargo:rustc-env=BLAULICHT_GIT_COMMIT={commit}");
}
