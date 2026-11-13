use std::process::{Command, Output, Stdio};

fn run_tuilane(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tuilane"))
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("failed to run tuilane")
}

#[test]
fn no_arguments_requires_an_interactive_terminal() {
    let output = run_tuilane(&[]);
    let stderr = String::from_utf8(output.stderr).expect("error output should be UTF-8");

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(stderr.contains("an interactive terminal is required"));
}

#[test]
fn help_flag_displays_usage_and_application_description() {
    let output = run_tuilane(&["--help"]);
    let stdout = String::from_utf8(output.stdout).expect("help output should be UTF-8");

    assert!(output.status.success());
    assert!(stdout.contains("Usage: tuilane"));
    assert!(stdout.contains("A local-first, modular, plugin-driven terminal workspace"));
    assert!(output.stderr.is_empty());
}

#[test]
fn version_flag_displays_the_package_version() {
    let output = run_tuilane(&["--version"]);
    let stdout = String::from_utf8(output.stdout).expect("version output should be UTF-8");

    assert!(output.status.success());
    assert_eq!(stdout, format!("tuilane {}\n", env!("CARGO_PKG_VERSION")));
    assert!(output.stderr.is_empty());
}

#[test]
fn unknown_argument_returns_a_usage_error() {
    let output = run_tuilane(&["--unknown"]);
    let stderr = String::from_utf8(output.stderr).expect("error output should be UTF-8");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(stderr.contains("--unknown"));
    assert!(stderr.contains("Usage: tuilane"));
}
