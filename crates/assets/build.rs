use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=icons");
    println!("cargo:rerun-if-changed=Makefile");

    let status = Command::new("make")
        .arg("all")
        .status()
        .expect("Failed to execute make. Is it installed?");

    if !status.success() {
        println!(
            "cargo::warning=Makefile failed to generate icons. Check the terminal output above."
        );
    }
}
