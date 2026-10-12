use std::process::Command;

/// Runs `git` with the given arguments and returns its trimmed stdout, or
/// `None` if git is unavailable or the command failed.
fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    Some(stdout.trim().to_string())
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // Recompute on any source change so the label tracks the tree state of the
    // build currently being produced.
    println!("cargo:rerun-if-changed=src");

    let describe = git(&["describe", "--tags", "--always", "--dirty"]).unwrap_or_else(|| {
        format!(
            "v{}",
            std::env::var("CARGO_PKG_VERSION").unwrap_or_default()
        )
    });
    println!("cargo:rustc-env=GIT_DESCRIBE={describe}");

    // "Dirty" mirrors `git describe --dirty`: uncommitted changes to tracked
    // files (staged or unstaged). Untracked files are ignored.
    let dirty = git(&["status", "--porcelain", "--untracked-files=no"])
        .map(|status| !status.is_empty())
        .unwrap_or(false);
    println!("cargo:rustc-env=GIT_DIRTY={}", u8::from(dirty));

    // Rerun when the repository state itself changes.
    if let Some(git_dir) = git(&["rev-parse", "--absolute-git-dir"]) {
        println!("cargo:rerun-if-changed={git_dir}/HEAD");
        println!("cargo:rerun-if-changed={git_dir}/index");
    }
}
