//! `binnacle-cli` exists so a CLI host needs no terminal library: neither
//! ratatui nor crossterm may be anywhere in its dependency tree.

#[test]
fn the_tree_holds_no_terminal_library() {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let output = std::process::Command::new(cargo)
        .args(["tree", "-p", "binnacle-cli", "--all-features"])
        // On every target, so a target-specific dependency counts.
        .args(["-e", "normal", "--target", "all", "--prefix", "none"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "cargo tree failed:\n{stderr}");
    // The check can see the tree: the crate heads it, and it has the flags'
    // own dependencies.
    assert!(
        stdout.starts_with("binnacle-cli v") && stdout.contains("binnacle-layout v"),
        "unexpected tree:\n{stdout}"
    );
    let names: Vec<&str> = stdout
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    for banned in ["ratatui", "crossterm"] {
        assert!(
            !names.contains(&banned),
            "{banned} is in the tree:\n{stdout}"
        );
    }
}
