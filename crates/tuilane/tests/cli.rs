use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_DATA_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TestDataDirectory(PathBuf);

impl TestDataDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "tuilane-cli-test-{}-{}",
            std::process::id(),
            NEXT_DATA_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("create temporary data directory");
        Self(path)
    }
}

impl Drop for TestDataDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run_tuilane(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tuilane"))
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("failed to run tuilane")
}

fn run_tuilane_with_data(args: &[&str], data_directory: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tuilane"))
        .args(args)
        .env("TUILANE_DATA_DIR", data_directory)
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

#[test]
fn task_commands_share_persistent_storage_across_processes() {
    let data_directory = TestDataDirectory::new();
    let create = run_tuilane_with_data(
        &[
            "task",
            "create",
            "--title",
            "Write integration tests",
            "--due",
            "2026-11-20",
        ],
        &data_directory.0,
    );
    assert!(create.status.success());
    let created = String::from_utf8(create.stdout).expect("create output should be UTF-8");
    assert!(created.contains("\topen\t2026-11-20\tWrite integration tests"));
    let id = created
        .trim()
        .split('\t')
        .next()
        .expect("create output includes an id")
        .to_owned();

    let list = run_tuilane_with_data(&["task", "list"], &data_directory.0);
    assert!(list.status.success());
    assert!(
        String::from_utf8(list.stdout)
            .expect("list output should be UTF-8")
            .contains("Write integration tests")
    );

    let complete = run_tuilane_with_data(&["task", "complete", &id], &data_directory.0);
    assert!(complete.status.success());
    assert!(
        String::from_utf8(complete.stdout)
            .expect("complete output should be UTF-8")
            .contains("\tcompleted\t")
    );

    let open_only = run_tuilane_with_data(&["task", "list"], &data_directory.0);
    assert_eq!(open_only.stdout, b"No tasks.\n");
    let all = run_tuilane_with_data(&["task", "list", "--all"], &data_directory.0);
    assert!(
        String::from_utf8(all.stdout)
            .expect("all-task output should be UTF-8")
            .contains("Write integration tests")
    );

    let update = run_tuilane_with_data(
        &["task", "update", &id, "--title", "Review integration tests"],
        &data_directory.0,
    );
    assert!(update.status.success());
    let clear_due =
        run_tuilane_with_data(&["task", "update", &id, "--clear-due"], &data_directory.0);
    assert!(clear_due.status.success());

    let reopen = run_tuilane_with_data(&["task", "reopen", &id], &data_directory.0);
    assert!(reopen.status.success());
    let show = run_tuilane_with_data(&["task", "show", &id], &data_directory.0);
    let shown = String::from_utf8(show.stdout).expect("show output should be UTF-8");
    assert!(show.status.success());
    assert!(shown.contains("\topen\t-\tReview integration tests"));

    let delete = run_tuilane_with_data(&["task", "delete", &id], &data_directory.0);
    assert!(delete.status.success());
    assert!(
        String::from_utf8(delete.stdout)
            .expect("delete output should be UTF-8")
            .contains("Deleted task")
    );
}

#[test]
fn task_cli_rejects_invalid_dates_and_incomplete_updates() {
    let data_directory = TestDataDirectory::new();
    let invalid_date = run_tuilane_with_data(
        &["task", "create", "--title", "Review", "--due", "2026-02-30"],
        &data_directory.0,
    );
    assert_eq!(invalid_date.status.code(), Some(1));
    assert!(
        String::from_utf8(invalid_date.stderr)
            .expect("error output should be UTF-8")
            .contains("valid calendar dates")
    );

    let create = run_tuilane_with_data(&["task", "create", "--title", "Review"], &data_directory.0);
    assert!(create.status.success());
    let id = String::from_utf8(create.stdout)
        .expect("create output should be UTF-8")
        .trim()
        .split('\t')
        .next()
        .expect("create output includes an id")
        .to_owned();

    let incomplete_update = run_tuilane_with_data(&["task", "update", &id], &data_directory.0);
    assert_eq!(incomplete_update.status.code(), Some(1));
    assert!(
        String::from_utf8(incomplete_update.stderr)
            .expect("error output should be UTF-8")
            .contains("choose exactly one")
    );
}
