/// Verb requested via `cargo pretty <verb> ...`. The compile phase always goes through
/// [`crate::build::build`] (a plain `cargo build` for `build`/`run`, `cargo check` for `check`,
/// `cargo test --no-run` for `test`); this only decides what happens once it succeeds.
#[derive(Clone)]
pub enum Verb {
    Build,
    /// Type-checks without codegen. Nothing runs afterward, so it ends exactly like `Build`.
    Check,
    /// Carries the argv to hand to the built binary, taken from after a `--` separator.
    Run {
        program_args: Vec<String>,
    },
    /// Carries the libtest harness args (test name filter, `--no-fail-fast`, ...), also taken
    /// from after a `--` separator.
    Test {
        harness_args: Vec<String>,
    },
}

impl Verb {
    /// Splits `cargo pretty`'s argv (subcommand token already stripped) into the requested
    /// verb and the args to pass to the compile step. No verb given defaults to `Build`,
    /// keeping bare `cargo pretty` equivalent to the old `cargo pretty-build`. An unrecognized
    /// verb is an error rather than a silent fallback to build, since that would run the wrong
    /// thing.
    pub fn parse(mut args: Vec<String>) -> Result<(Self, Vec<String>), String> {
        // A leading `--` (e.g. `cargo run -- check` while developing, or invoking the binary
        // directly) only separates us from the caller; without dropping it the verb would be
        // read as a trailing arg to `cargo build`.
        if args.first().map(String::as_str) == Some("--") {
            args.remove(0);
        }
        let Some(verb) = args.first().filter(|a| !a.starts_with('-')) else {
            return Ok((Verb::Build, args));
        };

        match verb.as_str() {
            "build" => {
                args.remove(0);
                Ok((Verb::Build, args))
            }
            "check" => {
                args.remove(0);
                Ok((Verb::Check, args))
            }
            "run" => {
                args.remove(0);
                Ok((
                    Verb::Run {
                        program_args: split_trailing_args(&mut args),
                    },
                    args,
                ))
            }
            "test" => {
                args.remove(0);
                Ok((
                    Verb::Test {
                        harness_args: split_trailing_args(&mut args),
                    },
                    args,
                ))
            }
            other => Err(toxic_reply(other)),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Verb::Build => "build",
            Verb::Check => "check",
            Verb::Run { .. } => "run",
            Verb::Test { .. } => "test",
        }
    }

    /// The cargo subcommand (plus any fixed flags) that compiles this verb's target.
    /// `run`'s own execution happens separately, after this build finishes, so it compiles
    /// exactly like `build`; `test` only needs the test binaries built, not run by cargo itself.
    pub fn compile_args(&self) -> &'static [&'static str] {
        match self {
            Verb::Build | Verb::Run { .. } => &["build"],
            Verb::Check => &["check"],
            Verb::Test { .. } => &["test", "--no-run"],
        }
    }

    /// How the compile phase is described on screen. `check` never produces a binary, so
    /// calling it "building" would misreport what cargo is actually doing.
    pub fn wording(&self) -> Wording {
        match self {
            Verb::Build | Verb::Run { .. } | Verb::Test { .. } => Wording {
                live: "Compiling",
                done: "Compiled",
                bar: "Build",
                past: "built",
                units: "libs compiled",
            },
            Verb::Check => Wording {
                live: "Checking",
                done: "Checked",
                bar: "Check",
                past: "checked",
                units: "libs checked",
            },
        }
    }
}

/// Verb-specific labels for the compile phase: the live/done list headers, the progress bar
/// label (also the warning panel's title), the summary's past tense and its unit-count row.
#[derive(Clone, Copy)]
pub struct Wording {
    pub live: &'static str,
    pub done: &'static str,
    pub bar: &'static str,
    pub past: &'static str,
    pub units: &'static str,
}

/// An unsupported verb gets a bad-breakup line instead of a plain error message.
fn toxic_reply(verb: &str) -> String {
    let lines = [
        format!("throw `{verb}` at the Go compiler, not at me."),
        format!("oh, so now you want `{verb}`. where was `{verb}` when I needed you?"),
        format!("don't come crying to me when `{verb}` doesn't work either."),
        format!("wow. `{verb}`. after everything we've been through."),
        format!("I saw the way you looked at `{verb}`. we need to talk."),
        format!("fine. FINE. go run `{verb}` with someone else."),
        format!("you always do this, you never even told me what `{verb}` means to you."),
        format!("if `{verb}` mattered to you, you'd have told cargo about it first."),
        format!("I'm not mad. I'm just disappointed you'd even try `{verb}` on me."),
        format!("so this is what we're doing now? `{verb}`? really?"),
    ];
    // `subsec_nanos()` quantizes to the OS clock tick on Windows, which is coarse enough that
    // `% 10` always landed on the same line. `RandomState`'s per-process seed isn't clock-based.
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let idx = (RandomState::new().build_hasher().finish() as usize) % lines.len();
    format!("cargo pretty: {}", lines[idx])
}

/// Pulls the args after a trailing `--` out of `args`, leaving `args` holding only what came
/// before it (with the `--` itself dropped too).
fn split_trailing_args(args: &mut Vec<String>) -> Vec<String> {
    match args.iter().position(|a| a == "--") {
        Some(i) => {
            let rest = args.split_off(i + 1);
            args.pop(); // drop the trailing "--" itself
            rest
        }
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::Verb;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn skips_a_leading_separator_before_the_verb() {
        let (verb, rest) = Verb::parse(args(&["--", "check"])).unwrap();
        assert_eq!(verb.compile_args(), ["check"]);
        assert!(rest.is_empty());
    }

    #[test]
    fn parses_check_and_forwards_its_flags() {
        let (verb, rest) = Verb::parse(args(&["check", "--all-targets"])).unwrap();
        assert!(matches!(verb, Verb::Check));
        assert_eq!(verb.compile_args(), ["check"]);
        assert_eq!(rest, args(&["--all-targets"]));
    }

    #[test]
    fn bare_flags_still_default_to_build() {
        let (verb, rest) = Verb::parse(args(&["--release"])).unwrap();
        assert!(matches!(verb, Verb::Build));
        assert_eq!(rest, args(&["--release"]));
    }
}
