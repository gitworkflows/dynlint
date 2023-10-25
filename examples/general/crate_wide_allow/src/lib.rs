#![feature(rustc_private)]
#![warn(unused_extern_crates)]

extern crate rustc_ast;
extern crate rustc_span;

use clippy_utils::diagnostics::span_lint_and_help;
use if_chain::if_chain;
use rustc_ast::{AttrStyle, Crate, MetaItem, MetaItemKind};
use rustc_lint::{EarlyContext, EarlyLintPass};
use rustc_span::sym;

dynlint_linting::declare_early_lint! {
    /// ### What it does
    /// Checks for use of `#![allow(...)]` at the crate level.
    ///
    /// ### Why is this bad?
    /// Such uses cannot be overridden with `--warn` or `--deny` from the command line. They _can_
    /// be overridden with `--force-warn` or `--forbid`, but one must know the `#![allow(...)]`
    /// are present to use these unconventional options.
    ///
    /// ### Example
    /// ```rust
    /// #![allow(clippy::assertions_on_constants)] // in code
    /// ```
    /// Use instead:
    /// ```rust
    /// // Pass `--allow clippy::assertions-on-constants` on the command line.
    /// ```
    pub CRATE_WIDE_ALLOW,
    Warn,
    "use of `#![allow(...)]` at the crate level"
}

impl EarlyLintPass for CrateWideAllow {
    fn check_crate(&mut self, cx: &EarlyContext, krate: &Crate) {
        for attr in &krate.attrs {
            assert_eq!(AttrStyle::Inner, attr.style);
            if_chain! {
                if attr.has_name(sym::allow);
                if let Some([arg]) = attr.meta_item_list().as_deref();
                if let Some(MetaItem {
                    path,
                    kind: MetaItemKind::Word,
                    ..
                }) = arg.meta_item();
                then {
                    let path = path
                        .segments
                        .iter()
                        .map(|segment| segment.ident.as_str())
                        .collect::<Vec<_>>()
                        .join("::")
                        .replace('_', "-");
                    span_lint_and_help(
                        cx,
                        CRATE_WIDE_ALLOW,
                        attr.span,
                        &format!("silently overrides `--warn {path}` and `--deny {path}`"),
                        None,
                        &format!("pass `--allow {path}` on the command line"),
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use assert_cmd::{assert::Assert, Command};
    use cargo_metadata::MetadataCommand;
    use dynlint_internal::env;
    use std::{env::consts, path::Path, sync::Mutex};

    static MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn ui() {
        let _lock = MUTEX.lock().unwrap();

        dynlint_testing::ui_test_example(env!("CARGO_PKG_NAME"), "ui");
    }

    #[test]
    fn premise_warn() {
        test("--warn=clippy::assertions-on-constants", Assert::success);
    }

    #[test]
    fn premise_deny() {
        test("--deny=clippy::assertions-on-constants", Assert::success);
    }

    #[test]
    fn premise_forbid() {
        test("--forbid=clippy::assertions-on-constants", Assert::failure);
    }

    // smoelius: Here is why the below uses of `env_remove` and `env` are needed:
    // - `dynlint_testing::ui_test_example` above sets `DYNLINT_LIBRARY_PATH`. Having this
    //   environment variable set causes "found multiple libraries" errors when Dynlint is run
    //   directly. Hence, the variable must be unset before Dynlint can be run directly.
    // - Setting `RUSTFLAGS` forces `cargo check` to be re-run. Unfortunately, this also forces
    //   `cargo-dynlint` to be rebuilt, which causes problems on Windows, hence the need for the
    //   mutex.
    // smoelius: Invoking `cargo-dynlint` directly by path, rather than through `cargo run`, avoids
    // the rebuilding problem. But oddly enough, the tests are faster with the mutex than without.
    // smoelius: The real reason this test is slow is that setting `RUSTFLAGS` causes the metadata
    // entries to be rebuilt. Running `clippy` once and passing `--no-build` thereafter avoids this
    // problem.
    // smoelius: Metadata entries are no longer rebuilt when `RUSTFLAGS` changes.

    fn test(rustflags: &str, assert: impl Fn(Assert) -> Assert) {
        let _lock = MUTEX.lock().unwrap();

        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");

        Command::new("cargo")
            .current_dir(&manifest_dir)
            .args(["build", "--bin", "cargo-dynlint"])
            .assert()
            .success();

        let metadata = MetadataCommand::new()
            .current_dir(manifest_dir)
            .no_deps()
            .exec()
            .unwrap();
        let cargo_dynlint = metadata
            .target_directory
            .join("debug")
            .join(format!("cargo-dynlint{}", consts::EXE_SUFFIX));

        let cargo_dynlint = |example_rustflags: Option<&str>| {
            let mut command = Command::new(&cargo_dynlint);
            command
                .env_remove(env::DYNLINT_LIBRARY_PATH)
                .args(["dynlint", "--lib", "clippy"]);
            if let Some(rustflags) = example_rustflags {
                command.env(
                    env::RUSTFLAGS,
                    "--cfg no_dev_dependencies ".to_owned() + rustflags,
                );
                command.args(["--no-build"]);
            }
            command.args(["--", "--examples"]);
            command.assert()
        };

        cargo_dynlint(None).success();

        assert(cargo_dynlint(Some(rustflags)));
    }
}
