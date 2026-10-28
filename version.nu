#!/usr/bin/env nu

# Bump the workspace version from conventional commits since the last vX.Y.Z
# tag, write it into Cargo.toml, numass-processing/pyproject.toml, and
# numass-processing/convert-to-root.py, refresh Cargo.lock and uv.lock, commit
# them, and tag.
#
#   nu version.nu            # write, commit, tag
#   nu version.nu --dry-run  # print the plan and stop
#
# Breaking changes (a ! after the type, or a BREAKING CHANGE footer) bump the
# major number. feat bumps minor. fix bumps patch. Other types do not bump.
# Member crates use version.workspace = true. Cargo.lock is refreshed in the
# same commit: a later cargo build would otherwise rewrite it and leave the
# tree dirty. uv.lock is refreshed the same way. A full run keeps the Python
# versions equal to the workspace version. If uv is not on PATH, those Python
# files are left unchanged and the Cargo release still proceeds.
#
# Files carrying the version are listed once in `version-specs`; paths, TOML
# sections, and the uv-missing fallback are derived from it. uv.lock sits at
# the repository root because the root pyproject.toml is the uv workspace
# manifest.

const CARGO_MANIFEST = "Cargo.toml"
const WORKSPACE_SECTION = "[workspace.package]"
const CARGO_LOCK = "Cargo.lock"
const UV_LOCK = "uv.lock"
const UV_PACKAGE = "numass-processing"

# `kind` selects how the version is read and written; `uv` marks the Python side
# of the release, skipped when uv is not installed.
def version-specs [] {
    [
        {path: $CARGO_MANIFEST, kind: "toml-section", section: $WORKSPACE_SECTION, uv: false},
        {path: "numass-processing/pyproject.toml", kind: "toml-section", section: "[project]", uv: true},
        {path: "numass-processing/convert-to-root.py", kind: "python", section: "", uv: true},
    ]
}

def main [
    --dry-run (-n) # print the plan; do not write, commit, or tag
] {
    cd $env.FILE_PWD

    let tag = (release-tag)
    let current = (read-section-version $CARGO_MANIFEST $WORKSPACE_SECTION)
    let tagged = ($tag | str substring 1..)
    if $current != $tagged {
        fail $"($CARGO_MANIFEST) version ($current) does not match ($tag)"
    }

    let has_uv = (uv-installed)
    let all_specs = (version-specs)
    let specs = ($all_specs | where {|spec| $has_uv or (not $spec.uv)})
    let skipped = ($all_specs | where {|spec| $spec.uv and (not $has_uv)} | get path)
    let lock_files = if $has_uv { [$CARGO_LOCK, $UV_LOCK] } else { [$CARGO_LOCK] }
    let files = (($specs | get path) | append $lock_files)

    for spec in $specs {
        let version = (spec-version $spec)
        if $version != $current {
            fail $"($spec.path) version ($version) does not match ($CARGO_MANIFEST) ($current)"
        }
    }

    let commits = (commits-since $tag)
    let classified = ($commits | each {|commit|
        $commit | insert level (commit-bump $commit.subject $commit.body)
    })
    let bumps = ($classified | where level != "none")
    let level = (highest-bump ($classified | get level))

    if $level == "none" {
        print $"($current) stays ($tag); no feat, fix, or breaking commit"
        return
    }

    let next = (next-version $current $level)
    let ignored = ($classified | where level == "none" | length)
    show-plan $current $next $level $tag $bumps $ignored $dry_run $files $skipped

    if $dry_run {
        let dirty = (tracked-changes)
        if not ($dirty | is-empty) {
            print "tracked changes present; a real run will stop"
        }
        return
    }

    let dirty = (tracked-changes)
    if not ($dirty | is-empty) {
        print -e "error: tracked files have uncommitted changes"
        $dirty | each {|line| print -e $"  ($line)"}
        exit 1
    }
    if not (tag-missing $"v($next)") {
        fail $"tag v($next) already exists"
    }

    let saved = ($files | each {|path| {path: $path, text: (open -r $path)}})
    let updated = ($specs | each {|spec|
        {
            path: $spec.path,
            text: (spec-rewritten $spec (saved-text $saved $spec.path) $next),
        }
    })
    for file in $updated {
        $file.text | save -f $file.path
    }

    let refreshed = (^cargo metadata --offline --format-version 1 | complete)
    if $refreshed.exit_code != 0 {
        restore-release $saved
        fail $"cargo metadata failed: ($refreshed.stderr | str trim)"
    }

    if $has_uv {
        let locked = (^uv lock --offline | complete)
        if $locked.exit_code != 0 {
            restore-release $saved
            fail $"uv lock failed: ($locked.stderr | str trim)"
        }
    }

    if (open -r $CARGO_LOCK) == (saved-text $saved $CARGO_LOCK) {
        restore-release $saved
        fail $"($CARGO_LOCK) did not change with the version bump"
    }
    if $has_uv {
        if (open -r $UV_LOCK) == (saved-text $saved $UV_LOCK) {
            restore-release $saved
            fail $"($UV_LOCK) did not change with the version bump"
        }
        let locked_version = (read-lock-package-version $UV_LOCK $UV_PACKAGE)
        if $locked_version != $next {
            restore-release $saved
            fail $"($UV_LOCK) version ($locked_version) is not ($next)"
        }
        for spec in $specs {
            let version = (spec-version $spec)
            if $version != $next {
                restore-release $saved
                fail $"($spec.path) version ($version) is not ($next)"
            }
        }
    }

    let expected = ($saved | get path)
    let unexpected = (
        tracked-changes
        | each {|line| $line | str substring 3..}
        | where {|path| $path not-in $expected}
    )
    if not ($unexpected | is-empty) {
        restore-release $saved
        fail $"unexpected changes: ($unexpected | str join ', ')"
    }

    let added = (git-result add "--" ...$expected)
    if $added.exit_code != 0 {
        restore-release $saved
        fail $"git add failed: ($added.stderr | str trim)"
    }

    let message = $"chore: release v($next)"
    let committed = (git-result commit "-m" $message "--" ...$expected)
    if $committed.exit_code != 0 {
        restore-release $saved
        fail $"git commit failed: ($committed.stderr | str trim)"
    }

    let left = (tracked-changes)
    if not ($left | is-empty) {
        fail "tracked changes remain after the release commit; tag was not created"
    }

    let tagged_result = (git-result tag $"v($next)")
    if $tagged_result.exit_code != 0 {
        fail $"git tag failed: ($tagged_result.stderr | str trim)"
    }

    print $"tagged v($next)"
}

def restore-release [saved: list] {
    let paths = ($saved | get path)
    git-result reset "-q" "--" ...$paths | ignore
    for file in $saved {
        $file.text | save -f $file.path
    }
}

def saved-text [saved: list, path: string] {
    $saved | where path == $path | first | get text
}

def spec-version [spec: record] {
    match $spec.kind {
        "toml-section" => (read-section-version $spec.path $spec.section)
        "python" => (read-python-version $spec.path)
        _ => (fail $"unknown version spec ($spec.kind)")
    }
}

def spec-rewritten [spec: record, text: string, new_version: string] {
    match $spec.kind {
        "toml-section" => (rewrite-section-version $text $spec.section $new_version)
        "python" => (rewrite-python-version $text $new_version)
        _ => (fail $"unknown version spec ($spec.kind)")
    }
}

def uv-installed [] {
    not (which uv | is-empty)
}

def fail [message: string] {
    print -e $"error: ($message)"
    exit 1
}

def git-result [...args: string] {
    ^git ...$args | complete
}

def release-tag [] {
    let result = (git-result describe "--tags" "--abbrev=0" "--match" "v[0-9]*")
    if $result.exit_code != 0 {
        fail "no version tag reachable from HEAD"
    }
    let tag = ($result.stdout | str trim)
    if not ($tag =~ '^v\d+\.\d+\.\d+$') {
        fail $"latest tag ($tag) is not vX.Y.Z"
    }
    $tag
}

def tag-missing [tag: string] {
    let result = (git-result tag "--list" $tag)
    if $result.exit_code != 0 {
        fail $"git tag --list failed: ($result.stderr | str trim)"
    }
    ($result.stdout | str trim | is-empty)
}

def tracked-changes [] {
    let result = (git-result status "--porcelain=v1" "--untracked-files=no")
    if $result.exit_code != 0 {
        fail $"git status failed: ($result.stderr | str trim)"
    }
    $result.stdout | lines | where {|line| not ($line | is-empty)}
}

def commits-since [tag: string] {
    let result = (git-result log "--reverse" $"($tag)..HEAD" "--format=%x1e%h%x1f%s%x1f%b")
    if $result.exit_code != 0 {
        fail $"git log failed: ($result.stderr | str trim)"
    }
    $result.stdout
    | split row (char --unicode '1e')
    | where {|entry| not ($entry | is-empty)}
    | each {|entry|
        let parts = ($entry | split row (char --unicode '1f'))
        {
            hash: ($parts.0 | str trim),
            subject: ($parts.1? | default "" | str trim),
            body: ($parts.2? | default ""),
        }
    }
}

def commit-bump [subject: string, body: string] {
    let parsed = ($subject | parse --regex '^(?P<type>[a-z]+)(?:\((?P<scope>[^)]*)\))?(?P<bang>!)?: .+$')
    if ($parsed | is-empty) {
        return "none"
    }
    let breaking = (($parsed.0.bang? | default "") == "!") or (breaking-footer $body)
    if $breaking {
        "major"
    } else if $parsed.0.type == "feat" {
        "minor"
    } else if $parsed.0.type == "fix" {
        "patch"
    } else {
        "none"
    }
}

def breaking-footer [body: string] {
    $body | lines | any {|line|
        let trimmed = ($line | str trim)
        ($trimmed | str starts-with "BREAKING CHANGE:") or ($trimmed | str starts-with "BREAKING-CHANGE:")
    }
}

def highest-bump [levels: list<string>] {
    if ($levels | any {|level| $level == "major"}) {
        "major"
    } else if ($levels | any {|level| $level == "minor"}) {
        "minor"
    } else if ($levels | any {|level| $level == "patch"}) {
        "patch"
    } else {
        "none"
    }
}

def next-version [current: string, level: string] {
    let parts = ($current | split row "." | each {|part| $part | into int})
    match $level {
        "major" => $"($parts.0 + 1).0.0"
        "minor" => $"($parts.0).($parts.1 + 1).0"
        "patch" => $"($parts.0).($parts.1).($parts.2 + 1)"
        _ => (fail $"unknown bump ($level)")
    }
}

def read-section-version [path: string, section: string] {
    let found = (
        open -r $path
        | lines
        | reduce --fold {in_section: false, version: ""} {|line, acc|
            let trimmed = ($line | str trim)
            if ($trimmed | str starts-with "[") {
                {in_section: ($trimmed == $section), version: $acc.version}
            } else if $acc.in_section and ($trimmed =~ '^version\s*=') {
                let parsed = ($trimmed | parse --regex 'version\s*=\s*"(?P<version>\d+\.\d+\.\d+)"')
                if ($parsed | is-empty) {
                    fail $"cannot read version from ($path)"
                }
                {in_section: true, version: $parsed.0.version}
            } else {
                $acc
            }
        }
    )
    if ($found.version | is-empty) {
        fail $"no ($section) version in ($path)"
    }
    $found.version
}

# uv.lock repeats `version` for every package. Capture the line that follows
# `name = "<package>"` and ignore the lockfile header `version = 1`.
def read-lock-package-version [path: string, package: string] {
    let found = (
        open -r $path
        | lines
        | reduce --fold {want: false, version: ""} {|line, acc|
            let trimmed = ($line | str trim)
            if $trimmed == $"name = \"($package)\"" {
                {want: true, version: $acc.version}
            } else if $acc.want and ($trimmed =~ '^version\s*=') {
                let parsed = ($trimmed | parse --regex 'version\s*=\s*"(?P<version>\d+\.\d+\.\d+)"')
                if ($parsed | is-empty) {
                    fail $"cannot read ($package) version from ($path)"
                }
                {want: false, version: $parsed.0.version}
            } else if ($trimmed | str starts-with "[") {
                {want: false, version: $acc.version}
            } else {
                $acc
            }
        }
    )
    if ($found.version | is-empty) {
        fail $"no ($package) version in ($path)"
    }
    $found.version
}

def read-python-version [path: string] {
    let found = (
        open -r $path
        | lines
        | where {|line| ($line | str trim) =~ '^__version__\s*='}
    )
    if ($found | is-empty) {
        fail $"no __version__ in ($path)"
    }
    let parsed = ($found.0 | parse --regex '__version__\s*=\s*"(?P<version>\d+\.\d+\.\d+)"')
    if ($parsed | is-empty) {
        fail $"cannot read __version__ from ($path)"
    }
    $parsed.0.version
}

def rewrite-python-version [text: string, new_version: string] {
    let result = (
        $text
        | lines
        | reduce --fold {changed: false, lines: []} {|line, acc|
            let trimmed = ($line | str trim)
            if $trimmed =~ '^__version__\s*=' {
                let updated = ($line | str replace -r '__version__\s*=\s*"[^"]*"' $"__version__ = \"($new_version)\"")
                {
                    changed: ($acc.changed or ($updated != $line)),
                    lines: ($acc.lines | append $updated),
                }
            } else {
                {changed: $acc.changed, lines: ($acc.lines | append $line)}
            }
        }
    )
    if not $result.changed {
        fail "__version__ line was not found"
    }
    ($result.lines | str join (char nl)) + (char nl)
}

def rewrite-section-version [text: string, section: string, new_version: string] {
    let result = (
        $text
        | lines
        | reduce --fold {in_section: false, changed: false, lines: []} {|line, acc|
            let trimmed = ($line | str trim)
            let header = ($trimmed | str starts-with "[")
            let in_section = if $header { $trimmed == $section } else { $acc.in_section }
            if $in_section and (not $header) and ($trimmed =~ '^version\s*=') {
                let updated = ($line | str replace -r 'version\s*=\s*"[^"]*"' $"version = \"($new_version)\"")
                {
                    in_section: $in_section,
                    changed: ($acc.changed or ($updated != $line)),
                    lines: ($acc.lines | append $updated),
                }
            } else {
                {
                    in_section: $in_section,
                    changed: $acc.changed,
                    lines: ($acc.lines | append $line),
                }
            }
        }
    )
    if not $result.changed {
        fail $"($section) version line was not found"
    }
    ($result.lines | str join (char nl)) + (char nl)
}

def show-plan [
    current: string,
    next: string,
    level: string,
    tag: string,
    bumps: list,
    ignored: int,
    dry_run: bool,
    files: list<string>,
    skipped: list<string>,
] {
    print $"($current) -> ($next) \(($level)\)"
    print $"from ($tag)"
    print $"commit: chore: release v($next)"
    print $"tag: v($next)"
    print $"files: ($files | str join ', ')"
    if not ($skipped | is-empty) {
        print $"warning: uv is not installed; leaving ($skipped | str join ', ') unchanged"
    }
    for bump in $bumps {
        print $"  ($bump.hash) ($bump.level) ($bump.subject)"
    }
    if $ignored > 0 {
        print $"other commits: ($ignored)"
    }
    if $dry_run {
        print "dry-run: nothing written"
    }
}
