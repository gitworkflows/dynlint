use assert_cmd::prelude::*;
use dynlint_internal::{env, packaging::isolate};
use predicates::prelude::*;
use std::{env::set_var, fs::OpenOptions, io::Write};
use tempfile::{tempdir, tempdir_in};

// smoelius: "Separate lints into categories" commit
const REV: &str = "402fc24351c60a3c474e786fd76aa66aa8638d55";

#[ctor::ctor]
fn initialize() {
    set_var(env::CARGO_TERM_COLOR, "never");
}

#[test]
fn invalid_pattern() {
    for pattern in ["/*", "../*"] {
        let tempdir = tempdir().unwrap();

        dynlint_internal::cargo::init("package `invalid_pattern_test`", false)
            .current_dir(&tempdir)
            .args(["--name", "invalid_pattern_test"])
            .success()
            .unwrap();

        let mut file = OpenOptions::new()
            .write(true)
            .append(true)
            .open(tempdir.path().join("Cargo.toml"))
            .unwrap();

        // smoelius: For the `../*` test to be effective, there must be multiple copies of Dynlint
        // in Cargo's `checkouts` directory.
        write!(
            file,
            r#"
[workspace.metadata.dynlint]
libraries = [
    {{ git = "https://github.com/khulnasoft-lab/dynlint", pattern = "examples/general/crate_wide_allow", rev = "{REV}" }},
    {{ git = "https://github.com/khulnasoft-lab/dynlint", pattern = "{pattern}" }},
]
"#,
        )
        .unwrap();

        std::process::Command::cargo_bin("cargo-dynlint")
            .unwrap()
            .current_dir(&tempdir)
            .args(["dynlint", "--all"])
            .assert()
            .failure()
            .stderr(
                predicate::str::is_match(r#"Could not canonicalize "[^"]*""#)
                    .unwrap()
                    .or(predicate::str::is_match(
                        r"Pattern `[^`]*` could refer to `[^`]*`, which is outside of `[^`]*`",
                    )
                    .unwrap()),
            );
    }
}

#[test]
fn list() {
    let tempdir = tempdir().unwrap();

    dynlint_internal::cargo::init("package `list_test`", false)
        .current_dir(&tempdir)
        .args(["--name", "list_test"])
        .success()
        .unwrap();

    let mut file = OpenOptions::new()
        .write(true)
        .append(true)
        .open(tempdir.path().join("Cargo.toml"))
        .unwrap();

    write!(
        file,
        r#"
[[workspace.metadata.dynlint.libraries]]
git = "https://github.com/khulnasoft-lab/dynlint"
pattern = "examples/general/crate_wide_allow"
"#,
    )
    .unwrap();

    std::process::Command::cargo_bin("cargo-dynlint")
        .unwrap()
        .current_dir(&tempdir)
        .args(["dynlint", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("<unbuilt>"));
}

/// Verify that changes to workspace metadata cause the lints to be rerun.
#[test]
fn metadata_change() {
    let tempdir = tempdir_in(".").unwrap();

    dynlint_internal::cargo::init("package `metadata_change_test`", false)
        .current_dir(&tempdir)
        .args(["--name", "metadata_change_test"])
        .success()
        .unwrap();

    isolate(tempdir.path()).unwrap();

    let mut file = OpenOptions::new()
        .write(true)
        .append(true)
        .open(tempdir.path().join("Cargo.toml"))
        .unwrap();

    write!(
        file,
        r#"
    [[workspace.metadata.dynlint.libraries]]
    path = "../../examples/general/crate_wide_allow"
    "#
    )
    .unwrap();

    std::process::Command::cargo_bin("cargo-dynlint")
        .unwrap()
        .current_dir(&tempdir)
        .args(["dynlint", "--all"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Checking metadata_change_test"));

    std::process::Command::cargo_bin("cargo-dynlint")
        .unwrap()
        .current_dir(&tempdir)
        .args(["dynlint", "--all"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Checking metadata_change_test").not());

    write!(
        file,
        r#"
        [[workspace.metadata.dynlint.libraries]]
        path = "../../examples/restriction/question_mark_in_expression"
        "#
    )
    .unwrap();

    std::process::Command::cargo_bin("cargo-dynlint")
        .unwrap()
        .current_dir(&tempdir)
        .args(["dynlint", "--all"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Checking metadata_change_test"));
}

#[test]
fn nonexistent_git_library() {
    let tempdir = tempdir().unwrap();

    dynlint_internal::cargo::init("package `nonexistent_git_library_test`", false)
        .current_dir(&tempdir)
        .args(["--name", "nonexistent_git_library_test"])
        .success()
        .unwrap();

    let mut file = OpenOptions::new()
        .write(true)
        .append(true)
        .open(tempdir.path().join("Cargo.toml"))
        .unwrap();

    write!(
        file,
        r#"
[[workspace.metadata.dynlint.libraries]]
git = "https://github.com/khulnasoft-lab/dynlint"
pattern = "examples/general/crate_wide_allow"
"#
    )
    .unwrap();

    std::process::Command::cargo_bin("cargo-dynlint")
        .unwrap()
        .current_dir(&tempdir)
        .args(["dynlint", "--all"])
        .assert()
        .success();

    write!(
        file,
        r#"
[[workspace.metadata.dynlint.libraries]]
git = "https://github.com/khulnasoft-lab/dynlint"
pattern = "examples/general/nonexistent_library"
"#
    )
    .unwrap();

    std::process::Command::cargo_bin("cargo-dynlint")
        .unwrap()
        .current_dir(&tempdir)
        .args(["dynlint", "--all"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("No paths matched"));
}

#[test]
fn nonexistent_path_library() {
    let tempdir = tempdir_in(".").unwrap();

    dynlint_internal::cargo::init("package `nonexistent_path_library_test`", false)
        .current_dir(&tempdir)
        .args(["--name", "nonexistent_path_library_test"])
        .success()
        .unwrap();

    isolate(tempdir.path()).unwrap();

    let mut file = OpenOptions::new()
        .write(true)
        .append(true)
        .open(tempdir.path().join("Cargo.toml"))
        .unwrap();

    write!(
        file,
        r#"
[[workspace.metadata.dynlint.libraries]]
path = "../../examples/general/crate_wide_allow"
"#
    )
    .unwrap();

    std::process::Command::cargo_bin("cargo-dynlint")
        .unwrap()
        .current_dir(&tempdir)
        .args(["dynlint", "--all"])
        .assert()
        .success();

    write!(
        file,
        r#"
[[workspace.metadata.dynlint.libraries]]
path = "../../examples/general/nonexistent_library"
"#
    )
    .unwrap();

    std::process::Command::cargo_bin("cargo-dynlint")
        .unwrap()
        .current_dir(&tempdir)
        .args(["dynlint", "--all"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("No paths matched"));
}

/// Verify that changes to `RUSTFLAGS` do not cause workspace metadata entries to be rebuilt.
#[test]
fn rustflags_change() {
    let tempdir = tempdir_in(".").unwrap();

    dynlint_internal::cargo::init("package `rustflags_change_test`", false)
        .current_dir(&tempdir)
        .args(["--name", "rustflags_change_test"])
        .success()
        .unwrap();

    isolate(tempdir.path()).unwrap();

    let mut file = OpenOptions::new()
        .write(true)
        .append(true)
        .open(tempdir.path().join("Cargo.toml"))
        .unwrap();

    write!(
        file,
        r#"
[[workspace.metadata.dynlint.libraries]]
path = "../../examples/general/crate_wide_allow"
"#
    )
    .unwrap();

    std::process::Command::cargo_bin("cargo-dynlint")
        .unwrap()
        .current_dir(&tempdir)
        .args(["dynlint", "--all"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Compiling"));

    std::process::Command::cargo_bin("cargo-dynlint")
        .unwrap()
        .current_dir(&tempdir)
        .env(env::RUSTFLAGS, "--verbose")
        .args(["dynlint", "--all"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Compiling").not());
}

#[test]
fn unknown_keys() {
    let tempdir = tempdir().unwrap();

    dynlint_internal::cargo::init("package `unknown_keys_test`", false)
        .current_dir(&tempdir)
        .args(["--name", "unknown_keys_test"])
        .success()
        .unwrap();

    let mut file = OpenOptions::new()
        .write(true)
        .append(true)
        .open(tempdir.path().join("Cargo.toml"))
        .unwrap();

    write!(
        file,
        r#"
[[workspace.metadata.dynlint.libraries]]
git = "https://github.com/khulnasoft-lab/dynlint"
pattern = "examples/general/crate_wide_allow"
"#,
    )
    .unwrap();

    std::process::Command::cargo_bin("cargo-dynlint")
        .unwrap()
        .current_dir(&tempdir)
        .args(["dynlint", "--all"])
        .assert()
        .success();

    writeln!(file, r#"revision = "{REV}""#,).unwrap();

    std::process::Command::cargo_bin("cargo-dynlint")
        .unwrap()
        .current_dir(&tempdir)
        .args(["dynlint", "--all"])
        .assert()
        .failure()
        .stderr(predicate::str::is_match(r"Unknown library keys:\r?\n\s*revision\r?\n").unwrap());
}
