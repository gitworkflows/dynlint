use crate::error::warn;
use anyhow::{anyhow, ensure, Context, Result};
use cargo_metadata::MetadataCommand;
use dynlint_internal::{
    driver as dynlint_driver, env,
    rustup::{toolchain_path, SanitizeEnvironment},
};
use semver::Version;
use std::{
    env::consts,
    fs::{copy, create_dir_all, write},
    path::{Path, PathBuf},
};
use tempfile::tempdir;

include!(concat!(env!("OUT_DIR"), "/dynlint_driver_manifest_dir.rs"));

const README_TXT: &str = "
This directory contains Rust compiler drivers used by Dynlint
(https://github.com/khulnasoft-lab/dynlint).

Deleting this directory will cause Dynlint to rebuild the drivers
the next time it needs them, but will have no ill effects.
";

fn cargo_toml(toolchain: &str, dynlint_driver_spec: &str) -> String {
    format!(
        r#"
[package]
name = "dynlint_driver-{toolchain}"
version = "0.1.0"
edition = "2018"

[dependencies]
anyhow = "1.0"
env_logger = "0.10"
dynlint_driver = {{ {dynlint_driver_spec} }}
"#,
    )
}

fn rust_toolchain(toolchain: &str) -> String {
    format!(
        r#"
[toolchain]
channel = "{toolchain}"
components = ["llvm-tools-preview", "rustc-dev"]
"#,
    )
}

const MAIN_RS: &str = r"
use anyhow::Result;
use std::env;
use std::ffi::OsString;

pub fn main() -> Result<()> {
    env_logger::init();

    let args: Vec<_> = env::args().map(OsString::from).collect();

    dynlint_driver::dynlint_driver(&args)
}
";

#[cfg_attr(
    dynlint_lib = "question_mark_in_expression",
    allow(question_mark_in_expression)
)]
pub fn get(opts: &crate::Dynlint, toolchain: &str) -> Result<PathBuf> {
    let dynlint_drivers = dynlint_drivers()?;

    let driver_dir = dynlint_drivers.join(toolchain);
    if !driver_dir.is_dir() {
        create_dir_all(&driver_dir).with_context(|| {
            format!(
                "`create_dir_all` failed for `{}`",
                driver_dir.to_string_lossy()
            )
        })?;
    }

    let driver = driver_dir.join("dynlint-driver");
    if !driver.exists() || is_outdated(opts, toolchain, &driver)? {
        build(opts, toolchain, &driver)?;
    }

    Ok(driver)
}

fn dynlint_drivers() -> Result<PathBuf> {
    if let Ok(dynlint_driver_path) = env::var(env::DYNLINT_DRIVER_PATH) {
        let dynlint_drivers = Path::new(&dynlint_driver_path);
        ensure!(dynlint_drivers.is_dir());
        Ok(dynlint_drivers.to_path_buf())
    } else {
        let home = dirs::home_dir().ok_or_else(|| anyhow!("Could not find HOME directory"))?;
        let dynlint_drivers = Path::new(&home).join(".dynlint_drivers");
        if !dynlint_drivers.is_dir() {
            create_dir_all(&dynlint_drivers).with_context(|| {
                format!(
                    "`create_dir_all` failed for `{}`",
                    dynlint_drivers.to_string_lossy()
                )
            })?;
            let readme_txt = dynlint_drivers.join("README.txt");
            write(&readme_txt, README_TXT).with_context(|| {
                format!("`write` failed for `{}`", readme_txt.to_string_lossy())
            })?;
        }
        Ok(dynlint_drivers)
    }
}

fn is_outdated(opts: &crate::Dynlint, toolchain: &str, driver: &Path) -> Result<bool> {
    (|| -> Result<bool> {
        let mut command = dynlint_driver(toolchain, driver)?;
        let output = command.args(["-V"]).output()?;
        let stdout = std::str::from_utf8(&output.stdout)?;
        let theirs = stdout
            .trim_end()
            .rsplit_once(' ')
            .map(|(_, s)| s)
            .ok_or_else(|| anyhow!("Could not determine driver version"))?;

        let their_version = Version::parse(theirs)
            .with_context(|| format!("Could not parse driver version `{theirs}`"))?;

        let our_version = Version::parse(env!("CARGO_PKG_VERSION"))?;

        Ok(their_version < our_version)
    })()
    .or_else(|error| {
        warn(opts, &error.to_string());
        Ok(true)
    })
}

#[cfg_attr(dynlint_lib = "supplementary", allow(commented_code))]
fn build(opts: &crate::Dynlint, toolchain: &str, driver: &Path) -> Result<()> {
    let tempdir = tempdir().with_context(|| "`tempdir` failed")?;
    let package = tempdir.path();

    initialize(toolchain, package)?;

    let metadata = MetadataCommand::new()
        .current_dir(package)
        .no_deps()
        .exec()?;

    let toolchain_path = toolchain_path(package)?;

    // smoelius: The commented code was the old behavior. It would cause the driver to have rpaths
    // like `$ORIGIN/../../`... (see https://github.com/khulnasoft-lab/dynlint/issues/54). The new
    // behavior causes the driver to have absolute rpaths.
    // let rustflags = "-C rpath=yes";
    let rustflags = format!(
        "-C link-args=-Wl,-rpath,{}/lib",
        toolchain_path.to_string_lossy()
    );

    #[cfg(debug_assertions)]
    if DYNLINT_DRIVER_MANIFEST_DIR.is_none() {
        warn(opts, "In debug mode building driver from `crates.io`");
    }

    dynlint_internal::cargo::build(&format!("driver for toolchain `{toolchain}`"), opts.quiet)
        .sanitize_environment()
        .envs([(env::RUSTFLAGS, rustflags)])
        .current_dir(package)
        .success()?;

    let binary = metadata
        .target_directory
        .join("debug")
        .join(format!("dynlint_driver-{toolchain}{}", consts::EXE_SUFFIX));
    #[cfg_attr(dynlint_lib = "general", allow(non_thread_safe_call_in_test))]
    copy(&binary, driver).with_context(|| {
        format!(
            "Could not copy `{binary}` to `{}`",
            driver.to_string_lossy()
        )
    })?;

    Ok(())
}

// smoelius: `package` is a temporary directory. So there should be no race here.
#[cfg_attr(dynlint_lib = "general", allow(non_thread_safe_call_in_test))]
fn initialize(toolchain: &str, package: &Path) -> Result<()> {
    let version_spec = format!("version = \"={}\"", env!("CARGO_PKG_VERSION"));

    let path_spec = DYNLINT_DRIVER_MANIFEST_DIR.map_or(String::new(), |path| {
        format!(", path = \"{}\"", path.replace('\\', "\\\\"))
    });

    let dynlint_driver_spec = format!("{version_spec}{path_spec}");

    let cargo_toml_path = package.join("Cargo.toml");
    write(
        &cargo_toml_path,
        cargo_toml(toolchain, &dynlint_driver_spec),
    )
    .with_context(|| format!("`write` failed for `{}`", cargo_toml_path.to_string_lossy()))?;
    let rust_toolchain_path = package.join("rust-toolchain");
    write(&rust_toolchain_path, rust_toolchain(toolchain)).with_context(|| {
        format!(
            "`write` failed for `{}`",
            rust_toolchain_path.to_string_lossy()
        )
    })?;
    let src = package.join("src");
    create_dir_all(&src)
        .with_context(|| format!("`create_dir_all` failed for `{}`", src.to_string_lossy()))?;
    let main_rs = src.join("main.rs");
    write(&main_rs, MAIN_RS)
        .with_context(|| format!("`write` failed for `{}`", main_rs.to_string_lossy()))?;

    Ok(())
}

#[cfg(test)]
mod test {
    #![allow(clippy::unwrap_used)]

    use super::*;

    // smoelius: `tempdir` is a temporary directory. So there should be no race here.
    #[cfg_attr(dynlint_lib = "general", allow(non_thread_safe_call_in_test))]
    #[test]
    fn nightly() {
        let tempdir = tempdir().unwrap();
        build(
            &crate::Dynlint::default(),
            "nightly",
            &tempdir.path().join("dynlint-driver"),
        )
        .unwrap();
    }
}
