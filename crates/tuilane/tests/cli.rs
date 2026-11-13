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

#[cfg(unix)]
#[test]
fn note_cli_edits_markdown_and_keeps_content_when_editor_fails() {
    use std::os::unix::fs::PermissionsExt;

    let data_directory = TestDataDirectory::new();
    let editor_path = data_directory.0.join("editor");
    fs::write(
        &editor_path,
        "#!/bin/sh\nprintf '# Research\\n\\nFirst draft' > \"$1\"\n",
    )
    .expect("write temporary editor");
    fs::set_permissions(&editor_path, fs::Permissions::from_mode(0o700))
        .expect("make temporary editor executable");

    let create = run_tuilane_with_editor(
        &["note", "create", "--title", "Research"],
        &data_directory.0,
        &editor_path,
    );
    assert!(create.status.success());
    let created = String::from_utf8(create.stdout).expect("create output should be UTF-8");
    assert!(created.contains("First draft"));
    let id = created
        .lines()
        .next()
        .and_then(|line| line.split('\t').next())
        .expect("create output includes the note ID");

    fs::write(
        &editor_path,
        "#!/bin/sh\nprintf '\\n\\nRevised draft' >> \"$1\"\n",
    )
    .expect("update temporary editor");
    let edit = run_tuilane_with_editor(&["note", "edit", id], &data_directory.0, &editor_path);
    assert!(edit.status.success());
    assert!(
        String::from_utf8(edit.stdout)
            .unwrap()
            .contains("Revised draft")
    );

    fs::write(&editor_path, "#!/bin/sh\nexit 2\n").expect("replace with failing editor");
    let failed_edit =
        run_tuilane_with_editor(&["note", "edit", id], &data_directory.0, &editor_path);
    assert_eq!(failed_edit.status.code(), Some(1));

    let show = run_tuilane_with_data(&["note", "show", id], &data_directory.0);
    let shown = String::from_utf8(show.stdout).expect("show output should be UTF-8");
    assert!(show.status.success());
    assert!(shown.contains("First draft"));
    assert!(shown.contains("Revised draft"));
}

#[test]
fn idea_cli_commands_share_persistent_storage_across_processes() {
    let data_directory = TestDataDirectory::new();
    let create = run_tuilane_with_data(
        &[
            "idea",
            "create",
            "--title",
            "Connect project reviews to notes",
        ],
        &data_directory.0,
    );
    assert!(create.status.success());
    let created = String::from_utf8(create.stdout).expect("create output should be UTF-8");
    let id = created
        .trim()
        .split('\t')
        .next()
        .expect("create output includes the idea ID")
        .to_owned();

    let rename = run_tuilane_with_data(
        &[
            "idea",
            "rename",
            &id,
            "--title",
            "Connect project reviews with notes",
        ],
        &data_directory.0,
    );
    assert!(rename.status.success());

    let show = run_tuilane_with_data(&["idea", "show", &id], &data_directory.0);
    let shown = String::from_utf8(show.stdout).expect("show output should be UTF-8");
    assert!(show.status.success());
    assert!(shown.contains("Connect project reviews with notes"));

    let list = run_tuilane_with_data(&["idea", "list"], &data_directory.0);
    assert!(list.status.success());
    assert!(String::from_utf8(list.stdout).unwrap().contains(&id));

    let delete = run_tuilane_with_data(&["idea", "delete", &id], &data_directory.0);
    assert!(delete.status.success());
    let empty = run_tuilane_with_data(&["idea", "list"], &data_directory.0);
    assert_eq!(empty.stdout, b"No ideas.\n");
}

#[cfg(unix)]
fn run_tuilane_with_editor(args: &[&str], data_directory: &Path, editor: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tuilane"))
        .args(args)
        .env("TUILANE_DATA_DIR", data_directory)
        .env_remove("VISUAL")
        .env("EDITOR", editor)
        .stdin(Stdio::null())
        .output()
        .expect("failed to run tuilane")
}
