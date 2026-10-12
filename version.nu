#!/usr/bin/env nu

# Bump the workspace version from conventional commits since the last vX.Y.Z
# tag, write it into Cargo.toml, refresh Cargo.lock, commit both, and tag.
#
#   nu version.nu            # write, commit, tag
#   nu version.nu --dry-run  # print the plan and stop
#
# Breaking changes (a ! after the type, or a BREAKING CHANGE footer) bump the
# major number. feat bumps minor. fix bumps patch. Other types do not bump.
# Member crates use version.workspace = true. Cargo.lock is refreshed in the
# same commit: a later cargo build would otherwise rewrite it and leave the
# tree dirty.

def main [
    --dry-run (-n) # print the plan; do not write, commit, or tag
] {
    cd $env.FILE_PWD

    let tag = (release-tag)
    let current = (read-workspace-version "Cargo.toml")
    let tagged = ($tag | str substring 1..)
    if $current != $tagged {
        fail $"Cargo.toml version ($current) does not match ($tag)"
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
    show-plan $current $next $level $tag $bumps $ignored $dry_run

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

    let toml_path = "Cargo.toml"
    let lock_path = "Cargo.lock"
    let original_toml = (open -r $toml_path)
    let original_lock = (open -r $lock_path)
    let updated = (rewrite-workspace-version $original_toml $next)
    $updated | save -f $toml_path

    let refreshed = (^cargo metadata --offline --format-version 1 | complete)
    if $refreshed.exit_code != 0 {
        restore-release $toml_path $lock_path $original_toml $original_lock
        fail $"cargo metadata failed: ($refreshed.stderr | str trim)"
    }

    let new_lock = (open -r $lock_path)
    if $new_lock == $original_lock {
        restore-release $toml_path $lock_path $original_toml $original_lock
        fail "Cargo.lock did not change with the version bump"
    }

    let unexpected = (
        tracked-changes
        | each {|line| $line | str substring 3..}
        | where {|path| $path not-in [$toml_path $lock_path]}
    )
    if not ($unexpected | is-empty) {
        restore-release $toml_path $lock_path $original_toml $original_lock
        fail $"unexpected changes: ($unexpected | str join ', ')"
    }

    let added = (git-result add "--" $toml_path $lock_path)
    if $added.exit_code != 0 {
        restore-release $toml_path $lock_path $original_toml $original_lock
        fail $"git add failed: ($added.stderr | str trim)"
    }

    let message = $"chore: release v($next)"
    let committed = (git-result commit "-m" $message "--" $toml_path $lock_path)
    if $committed.exit_code != 0 {
        restore-release $toml_path $lock_path $original_toml $original_lock
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

def restore-release [
    toml_path: string,
    lock_path: string,
    original_toml: string,
    original_lock: string,
] {
    git-result reset "-q" "--" $toml_path $lock_path | ignore
    $original_toml | save -f $toml_path
    $original_lock | save -f $lock_path
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

def read-workspace-version [path: string] {
    let found = (
        open -r $path
        | lines
        | reduce --fold {in_pkg: false, version: ""} {|line, acc|
            let trimmed = ($line | str trim)
            if ($trimmed | str starts-with "[") {
                {in_pkg: ($trimmed == "[workspace.package]"), version: $acc.version}
            } else if $acc.in_pkg and ($trimmed =~ '^version\s*=') {
                let parsed = ($trimmed | parse --regex 'version\s*=\s*"(?P<version>\d+\.\d+\.\d+)"')
                if ($parsed | is-empty) {
                    fail $"cannot read version from ($path)"
                }
                {in_pkg: true, version: $parsed.0.version}
            } else {
                $acc
            }
        }
    )
    if ($found.version | is-empty) {
        fail $"no [workspace.package] version in ($path)"
    }
    $found.version
}

def rewrite-workspace-version [text: string, new_version: string] {
    let result = (
        $text
        | lines
        | reduce --fold {in_pkg: false, changed: false, lines: []} {|line, acc|
            let trimmed = ($line | str trim)
            let section = ($trimmed | str starts-with "[")
            let in_pkg = if $section { $trimmed == "[workspace.package]" } else { $acc.in_pkg }
            if $in_pkg and (not $section) and ($trimmed =~ '^version\s*=') {
                let updated = ($line | str replace -r 'version\s*=\s*"[^"]*"' $"version = \"($new_version)\"")
                {
                    in_pkg: $in_pkg,
                    changed: ($acc.changed or ($updated != $line)),
                    lines: ($acc.lines | append $updated),
                }
            } else {
                {
                    in_pkg: $in_pkg,
                    changed: $acc.changed,
                    lines: ($acc.lines | append $line),
                }
            }
        }
    )
    if not $result.changed {
        fail "workspace.package version line was not found"
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
] {
    print $"($current) -> ($next) \(($level)\)"
    print $"from ($tag)"
    print $"commit: chore: release v($next)"
    print $"tag: v($next)"
    print "files: Cargo.toml, Cargo.lock"
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
