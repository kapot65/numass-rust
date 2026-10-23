use std::process::Command;

/// Paths that never affect the produced binaries: editing them must not mark
/// the build dirty. `:(top)` anchors the pattern at the repository root,
/// because build scripts run with the crate directory as their working
/// directory.
const NOT_IN_BUILD: &[&str] = &[":(top,exclude)docs"];

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

/// Mirrors `git describe --dirty`: uncommitted changes to tracked files
/// (staged or unstaged), untracked files ignored. Changes under
/// [`NOT_IN_BUILD`] are ignored as well, so editing docs keeps the label
/// clean.
fn is_dirty() -> bool {
    let mut args = vec!["status", "--porcelain", "--untracked-files=no", "--"];
    args.extend_from_slice(NOT_IN_BUILD);
    git(&args).map(|status| !status.is_empty()).unwrap_or(false)
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // Recompute on any source change so the label tracks the tree state of the
    // build currently being produced.
    println!("cargo:rerun-if-changed=src");

    let dirty = is_dirty();
    println!("cargo:rustc-env=GIT_DIRTY={}", u8::from(dirty));

    let describe = git(&["describe", "--tags", "--always"]).unwrap_or_else(|| {
        format!(
            "v{}",
            std::env::var("CARGO_PKG_VERSION").unwrap_or_default()
        )
    });
    // Appended by hand: `--dirty` would also flag the paths in [`NOT_IN_BUILD`].
    let describe = if dirty {
        format!("{describe}-dirty")
    } else {
        describe
    };
    println!("cargo:rustc-env=GIT_DESCRIBE={describe}");

    // Rerun when the repository state itself changes.
    if let Some(git_dir) = git(&["rev-parse", "--absolute-git-dir"]) {
        println!("cargo:rerun-if-changed={git_dir}/HEAD");
        println!("cargo:rerun-if-changed={git_dir}/index");
    }
}
