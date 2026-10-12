use std::env;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;

/// Файлы, которые `data-viewer-web` вшивает через `include_str!` / `include_bytes!`
/// (`src/bin/data-viewer-web.rs`, пути относительно `numass-server/dist`).
const VIEWER_ASSETS: &[&str] = &[
    "index.html",
    "data-viewer.js",
    "data-viewer_bg.wasm",
    "worker.js",
    "worker_bg.wasm",
];

/// Cargo-переменные, которые вложенному `trunk`/`cargo` нужно оставить.
/// Остальные `CARGO_*` относятся к сборке самого сервера и ломают wasm-сборку
/// (флаги хоста, фичи пакета, jobserver — из-за него вложенный cargo может
/// навсегда ждать свободный слот).
const KEEP_CARGO_ENV: &[&str] = &[
    "CARGO_HOME",
    "CARGO_TERM_COLOR",
    "CARGO_NET_OFFLINE",
    "CARGO_NET_GIT_FETCH_WITH_CLI",
    "CARGO_HTTP_CAINFO",
    "CARGO_HTTP_CHECK_REVOKE",
    "CARGO_REGISTRIES_CRATES_IO_PROTOCOL",
];

fn main() {
    // В debug ассеты не компилируются: роуты закрыты `#[cfg(not(debug_assertions))]`.
    // `DEBUG` у cargo совпадает с этим cfg.
    if env::var("DEBUG").ok().as_deref() == Some("true") {
        return;
    }

    let paths = Paths::resolve();
    declare_reruns(&paths);

    if env::var("NUMASS_SKIP_VIEWER_BUILD").ok().as_deref() == Some("1") {
        ensure_assets(&paths.dist_dir);
        println!(
            "cargo:warning=NUMASS_SKIP_VIEWER_BUILD=1, using existing {}",
            paths.dist_dir.display()
        );
        return;
    }

    match run_trunk(&paths) {
        TrunkRun::Finished => ensure_assets(&paths.dist_dir),
        TrunkRun::NotInstalled if assets_present(&paths.dist_dir) => {
            println!(
                "cargo:warning=trunk not found on PATH, using existing {}",
                paths.dist_dir.display()
            );
        }
        TrunkRun::NotInstalled => {
            let missing = missing_assets(&paths.dist_dir);
            panic!(
                "trunk is not installed and viewer assets are missing ({}) in {}.\n\
                 Install trunk (`cargo install trunk`) or set NUMASS_SKIP_VIEWER_BUILD=1 \
                 after building the assets yourself.",
                missing
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
                paths.dist_dir.display()
            );
        }
    }
}

struct Paths {
    workspace_root: PathBuf,
    viewers_dir: PathBuf,
    dist_dir: PathBuf,
    trunk_target_dir: PathBuf,
}

impl Paths {
    fn resolve() -> Self {
        let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
        let workspace_root = workspace_root(&manifest_dir);
        let viewers_dir = workspace_root.join("numass-viewers");
        if !viewers_dir.join("index.html").is_file() {
            panic!(
                "numass-viewers not found at {} (expected index.html)",
                viewers_dir.display()
            );
        }

        Self {
            // Отдельный target dir: cargo держит lock на `target/`, и trunk,
            // запущенный из build.rs, навсегда встанет на этом lock.
            trunk_target_dir: workspace_root.join("target-trunk"),
            // `include_*!("../../dist/...")` из `src/bin/` резолвится сюда.
            dist_dir: manifest_dir.join("dist"),
            workspace_root,
            viewers_dir,
        }
    }
}

fn workspace_root(manifest_dir: &Path) -> PathBuf {
    let mut dir = manifest_dir.to_path_buf();
    loop {
        let manifest = dir.join("Cargo.toml");
        if let Ok(text) = std::fs::read_to_string(&manifest)
            && text.lines().any(|line| line.trim() == "[workspace]")
        {
            return dir;
        }
        if !dir.pop() {
            panic!(
                "workspace root not found starting from {}",
                manifest_dir.display()
            );
        }
    }
}

fn declare_reruns(paths: &Paths) {
    let watch = [
        paths.viewers_dir.join("src"),
        paths.viewers_dir.join("build.rs"),
        paths.viewers_dir.join("index.html"),
        paths.viewers_dir.join("Trunk.toml"),
        paths.viewers_dir.join("Cargo.toml"),
        paths.workspace_root.join("numass-processing/src"),
        paths.workspace_root.join("numass-processing/Cargo.toml"),
        paths.workspace_root.join("dataforge-parser-rust/src"),
        paths
            .workspace_root
            .join("dataforge-parser-rust/Cargo.toml"),
        paths.workspace_root.join("dataforge-parser-numass/src"),
        paths
            .workspace_root
            .join("dataforge-parser-numass/Cargo.toml"),
    ];
    for path in &watch {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    // Состояние git меняет метку версии во wasm (`GIT_DIRTY`), но не трогает
    // исходники. Без этого trunk не перезапускается, и вшитый wasm остаётся
    // со старым dirty после коммита.
    watch_git_state();
    println!("cargo:rerun-if-env-changed=NUMASS_SKIP_VIEWER_BUILD");
}

fn watch_git_state() {
    let Ok(output) = Command::new("git")
        .args(["rev-parse", "--absolute-git-dir"])
        .output()
    else {
        return;
    };
    if !output.status.success() {
        return;
    }
    let Ok(git_dir) = String::from_utf8(output.stdout) else {
        return;
    };
    let git_dir = git_dir.trim();
    if git_dir.is_empty() {
        return;
    }
    println!("cargo:rerun-if-changed={git_dir}/HEAD");
    println!("cargo:rerun-if-changed={git_dir}/index");
}

enum TrunkRun {
    Finished,
    NotInstalled,
}

fn run_trunk(paths: &Paths) -> TrunkRun {
    let mut cmd = Command::new("trunk");
    cmd.current_dir(&paths.viewers_dir)
        .stdin(Stdio::null())
        .arg("build")
        .arg("--release")
        .arg("--dist")
        .arg(&paths.dist_dir);
    isolate_nested_cargo(&mut cmd, &paths.trunk_target_dir);

    let tty = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .ok();

    if let Some(tty) = &tty {
        let mut header = tty.try_clone().expect("dup /dev/tty");
        let _ = writeln!(
            header,
            "\ntrunk build --release\n  viewers: {}\n  dist:    {}\n  target:  {}\n",
            paths.viewers_dir.display(),
            paths.dist_dir.display(),
            paths.trunk_target_dir.display()
        );
        let _ = header.flush();
        // Прямой tty, а не stdout build.rs: cargo прячет вывод скрипта до -vv.
        cmd.stdout(Stdio::from(tty.try_clone().expect("dup /dev/tty stdout")));
        cmd.stderr(Stdio::from(tty.try_clone().expect("dup /dev/tty stderr")));
    } else {
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        println!(
            "cargo:warning=trunk build --release -> {} (target {})",
            paths.dist_dir.display(),
            paths.trunk_target_dir.display()
        );
    }

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return TrunkRun::NotInstalled,
        Err(err) => panic!("failed to spawn trunk: {err}"),
    };

    if tty.is_none() {
        let stdout = child.stdout.take().expect("trunk stdout");
        let stderr = child.stderr.take().expect("trunk stderr");
        let stdout_thread = thread::spawn(move || forward_as_warnings(stdout));
        let stderr_thread = thread::spawn(move || forward_as_warnings(stderr));
        let status = child.wait().expect("wait for trunk");
        stdout_thread.join().expect("trunk stdout thread");
        stderr_thread.join().expect("trunk stderr thread");
        if !status.success() {
            panic!("trunk build failed ({status})");
        }
    } else {
        let status = child.wait().expect("wait for trunk");
        if !status.success() {
            panic!("trunk build failed ({status})");
        }
    }

    TrunkRun::Finished
}

/// Без tty cargo показывает stderr build-скрипта только при падении или `-vv`.
/// `cargo:warning` виден в обычном выводе.
fn forward_as_warnings(pipe: impl std::io::Read) {
    let reader = BufReader::new(pipe);
    for line in reader.lines().map_while(Result::ok) {
        if line.is_empty() {
            continue;
        }
        println!("cargo:warning=trunk: {line}");
    }
}

fn isolate_nested_cargo(cmd: &mut Command, trunk_target_dir: &Path) {
    for key in [
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "MAKEFLAGS",
        "MFLAGS",
        "CARGO_MAKEFLAGS",
        "OUT_DIR",
        "DEBUG",
        "OPT_LEVEL",
        "PROFILE",
        "TARGET",
        "HOST",
        "NUM_JOBS",
    ] {
        cmd.env_remove(key);
    }

    let remove: Vec<String> = env::vars()
        .map(|(key, _)| key)
        .filter(|key| key.starts_with("CARGO_") && !KEEP_CARGO_ENV.contains(&key.as_str()))
        .collect();
    for key in remove {
        cmd.env_remove(key);
    }

    cmd.env("CARGO_TARGET_DIR", trunk_target_dir);
    // `progress.when = "always"` без width роняет cargo, чей stdout — pipe.
    // Trunk так вызывает cargo за списком артефактов.
    cmd.env("CARGO_TERM_PROGRESS_WHEN", "auto");
}

fn assets_present(dist_dir: &Path) -> bool {
    missing_assets(dist_dir).is_empty()
}

fn missing_assets(dist_dir: &Path) -> Vec<PathBuf> {
    VIEWER_ASSETS
        .iter()
        .map(|name| dist_dir.join(name))
        .filter(|path| !path.is_file())
        .collect()
}

fn ensure_assets(dist_dir: &Path) {
    let missing = missing_assets(dist_dir);
    if !missing.is_empty() {
        panic!(
            "viewer assets missing in {}: {}",
            dist_dir.display(),
            missing
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
}
