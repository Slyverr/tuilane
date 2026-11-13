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

#[test]
fn backup_roundtrip_restores_workspace_data_and_requires_confirmation() {
    let data_directory = TestDataDirectory::new();
    let task = run_tuilane_with_data(
        &["task", "create", "--title", "Backup candidate"],
        &data_directory.0,
    );
    let task_id = output_id(&task);
    let project = run_tuilane_with_data(
        &["project", "create", "--title", "Backup project"],
        &data_directory.0,
    );
    let project_id = output_id(&project);
    let task_ref = format!("task:{task_id}");
    let project_ref = format!("project:{project_id}");
    let link = run_tuilane_with_data(&["link", "add", &task_ref, &project_ref], &data_directory.0);
    assert!(link.status.success());

    let backup_path = data_directory.0.join("workspace-backup.sqlite3");
    let backup = run_tuilane_with_data(
        &["backup", "create", backup_path.to_str().unwrap()],
        &data_directory.0,
    );
    assert!(backup.status.success());
    assert!(backup_path.is_file());

    let update = run_tuilane_with_data(
        &[
            "task",
            "update",
            &task_id,
            "--title",
            "Changed after backup",
        ],
        &data_directory.0,
    );
    assert!(update.status.success());
    let additional = run_tuilane_with_data(
        &[
            "idea",
            "create",
            "--title",
            "Should disappear after restore",
        ],
        &data_directory.0,
    );
    assert!(additional.status.success());

    let unconfirmed = run_tuilane_with_data(
        &["backup", "restore", backup_path.to_str().unwrap()],
        &data_directory.0,
    );
    assert_eq!(unconfirmed.status.code(), Some(1));
    assert!(
        String::from_utf8(unconfirmed.stderr)
            .unwrap()
            .contains("pass --yes")
    );
    let current = run_tuilane_with_data(&["task", "show", &task_id], &data_directory.0);
    assert!(
        String::from_utf8(current.stdout)
            .unwrap()
            .contains("Changed after backup")
    );

    let restore = run_tuilane_with_data(
        &["backup", "restore", backup_path.to_str().unwrap(), "--yes"],
        &data_directory.0,
    );
    assert!(restore.status.success());
    let restored = run_tuilane_with_data(&["task", "show", &task_id], &data_directory.0);
    assert!(
        String::from_utf8(restored.stdout)
            .unwrap()
            .contains("Backup candidate")
    );
    let relations = run_tuilane_with_data(&["link", "list", &task_ref], &data_directory.0);
    assert!(
        String::from_utf8(relations.stdout)
            .unwrap()
            .contains("Backup project")
    );
    let ideas = run_tuilane_with_data(&["idea", "list"], &data_directory.0);
    assert_eq!(ideas.stdout, b"No ideas.\n");
}

#[test]
fn saved_searches_persist_across_cli_processes_and_preserve_filters() {
    let data_directory = TestDataDirectory::new();
    let idea = run_tuilane_with_data(
        &["idea", "create", "--title", "Plan offline research"],
        &data_directory.0,
    );
    assert!(idea.status.success());

    let search = run_tuilane_with_data(
        &[
            "search",
            "offline research",
            "--kind",
            "idea",
            "--save",
            "Research ideas",
        ],
        &data_directory.0,
    );
    assert!(
        search.status.success(),
        "{}",
        String::from_utf8_lossy(&search.stderr)
    );
    let search = String::from_utf8(search.stdout).unwrap();
    assert!(search.contains("Saved search: Research ideas"));
    assert!(search.contains("Plan offline research"));

    let list = run_tuilane_with_data(&["saved-search", "list"], &data_directory.0);
    assert!(list.status.success());
    assert_eq!(list.stdout, b"Research ideas\tidea\toffline research\n");

    let run = run_tuilane_with_data(
        &["saved-search", "run", "research IDEAS"],
        &data_directory.0,
    );
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        String::from_utf8(run.stdout)
            .unwrap()
            .contains("Plan offline research")
    );

    let delete = run_tuilane_with_data(
        &["saved-search", "delete", "Research ideas"],
        &data_directory.0,
    );
    assert!(delete.status.success());
    let list = run_tuilane_with_data(&["saved-search", "list"], &data_directory.0);
    assert_eq!(list.stdout, b"No saved searches.\n");
}

#[cfg(unix)]
#[test]
fn search_cli_finds_owner_content_and_filters_by_kind() {
    use std::os::unix::fs::PermissionsExt;

    let data_directory = TestDataDirectory::new();
    let editor = data_directory.0.join("editor");
    fs::write(
        &editor,
        "#!/bin/sh\nprintf 'distinctive fieldwork phrase' > \"$1\"\n",
    )
    .expect("write temporary editor");
    fs::set_permissions(&editor, fs::Permissions::from_mode(0o700))
        .expect("make temporary editor executable");

    let task = run_tuilane_with_data(
        &["task", "create", "--title", "Distinctive fieldwork task"],
        &data_directory.0,
    );
    let project = run_tuilane_with_data(
        &[
            "project",
            "create",
            "--title",
            "Distinctive fieldwork project",
        ],
        &data_directory.0,
    );
    let idea = run_tuilane_with_data(
        &["idea", "create", "--title", "Distinctive fieldwork idea"],
        &data_directory.0,
    );
    assert!(task.status.success());
    assert!(project.status.success());
    assert!(idea.status.success());
    let note = run_tuilane_with_editor(
        &["note", "create", "--title", "Fieldwork record"],
        &data_directory.0,
        &editor,
    );
    assert!(note.status.success());

    let all = run_tuilane_with_data(&["search", "fieldwork"], &data_directory.0);
    assert!(all.status.success());
    let output = String::from_utf8(all.stdout).expect("search output should be UTF-8");
    for item in ["task:", "project:", "idea:", "note:"] {
        assert!(output.contains(item), "missing {item} from {output}");
    }

    let notes = run_tuilane_with_data(
        &["search", "--kind", "note", "distinctive"],
        &data_directory.0,
    );
    assert!(notes.status.success());
    let output = String::from_utf8(notes.stdout).expect("search output should be UTF-8");
    assert!(output.contains("note:"));
    assert!(!output.contains("task:"));
    assert!(!output.contains("project:"));
    assert!(!output.contains("idea:"));
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
#[test]
fn connected_knowledge_workflow_survives_edits_restarts_and_deletions() {
    use std::os::unix::fs::PermissionsExt;

    let data_directory = TestDataDirectory::new();
    let editor_path = data_directory.0.join("editor");
    fs::write(&editor_path, "#!/bin/sh\nprintf '# Research' > \"$1\"\n")
        .expect("write temporary editor");
    fs::set_permissions(&editor_path, fs::Permissions::from_mode(0o700))
        .expect("make temporary editor executable");

    let idea = run_tuilane_with_data(
        &["idea", "create", "--title", "Connect work and research"],
        &data_directory.0,
    );
    let idea_id = output_id(&idea);
    let project = run_tuilane_with_data(
        &["project", "create", "--title", "Research program"],
        &data_directory.0,
    );
    let project_id = output_id(&project);
    let task = run_tuilane_with_data(
        &["task", "create", "--title", "Write a summary"],
        &data_directory.0,
    );
    let task_id = output_id(&task);
    let note = run_tuilane_with_editor(
        &["note", "create", "--title", "Research"],
        &data_directory.0,
        &editor_path,
    );
    let note_id = output_id(&note);

    let idea_ref = format!("idea:{idea_id}");
    let note_ref = format!("note:{note_id}");
    let task_ref = format!("task:{task_id}");
    let project_ref = format!("project:{project_id}");
    for target in [&note_ref, &task_ref, &project_ref] {
        let linked = run_tuilane_with_data(
            &["link", "add", idea_ref.as_str(), target.as_str()],
            &data_directory.0,
        );
        assert!(linked.status.success());
    }

    let backlinks =
        run_tuilane_with_data(&["link", "backlinks", note_ref.as_str()], &data_directory.0);
    assert!(backlinks.status.success());
    assert!(
        String::from_utf8(backlinks.stdout)
            .unwrap()
            .contains("Connect work and research")
    );

    fs::write(
        &editor_path,
        "#!/bin/sh\nprintf '\\n\\nEdited after linking' >> \"$1\"\n",
    )
    .expect("update temporary editor");
    let edited = run_tuilane_with_editor(
        &["note", "edit", note_id.as_str()],
        &data_directory.0,
        &editor_path,
    );
    assert!(edited.status.success());
    let note_rename = run_tuilane_with_data(
        &[
            "note",
            "rename",
            note_id.as_str(),
            "--title",
            "Updated research",
        ],
        &data_directory.0,
    );
    assert!(note_rename.status.success());
    let project_rename = run_tuilane_with_data(
        &[
            "project",
            "rename",
            project_id.as_str(),
            "--title",
            "Updated research program",
        ],
        &data_directory.0,
    );
    assert!(project_rename.status.success());
    let idea_rename = run_tuilane_with_data(
        &[
            "idea",
            "rename",
            idea_id.as_str(),
            "--title",
            "Connect updated work and research",
        ],
        &data_directory.0,
    );
    assert!(idea_rename.status.success());

    let outgoing = run_tuilane_with_data(&["link", "list", idea_ref.as_str()], &data_directory.0);
    let outgoing = String::from_utf8(outgoing.stdout).expect("link output should be UTF-8");
    assert!(outgoing.contains("Updated research"));
    assert!(outgoing.contains("Write a summary"));
    assert!(outgoing.contains("Updated research program"));

    let file_reference = run_tuilane_with_data(
        &[
            "link",
            "add-reference",
            idea_ref.as_str(),
            "--file",
            "research/meeting notes.md",
        ],
        &data_directory.0,
    );
    assert!(file_reference.status.success());
    let url_reference = run_tuilane_with_data(
        &[
            "link",
            "add-reference",
            idea_ref.as_str(),
            "--url",
            "https://example.org/research",
        ],
        &data_directory.0,
    );
    assert!(url_reference.status.success());
    let references = run_tuilane_with_data(
        &["link", "references", idea_ref.as_str()],
        &data_directory.0,
    );
    let references =
        String::from_utf8(references.stdout).expect("reference output should be UTF-8");
    assert!(references.contains("research/meeting notes.md"));
    assert!(references.contains("https://example.org/research"));

    let delete_task =
        run_tuilane_with_data(&["task", "delete", task_id.as_str()], &data_directory.0);
    assert!(delete_task.status.success());
    let delete_note =
        run_tuilane_with_data(&["note", "delete", note_id.as_str()], &data_directory.0);
    assert!(delete_note.status.success());
    let after_target_removal =
        run_tuilane_with_data(&["link", "list", idea_ref.as_str()], &data_directory.0);
    let after_target_removal =
        String::from_utf8(after_target_removal.stdout).expect("link output should be UTF-8");
    assert!(!after_target_removal.contains(&task_ref));
    assert!(!after_target_removal.contains(&note_ref));
    assert!(after_target_removal.contains(&project_ref));

    let delete_idea =
        run_tuilane_with_data(&["idea", "delete", idea_id.as_str()], &data_directory.0);
    assert!(delete_idea.status.success());
    let project_backlinks = run_tuilane_with_data(
        &["link", "backlinks", project_ref.as_str()],
        &data_directory.0,
    );
    assert_eq!(project_backlinks.stdout, b"No related items.\n");
}

fn output_id(output: &Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout.clone())
        .expect("command output should be UTF-8")
        .lines()
        .next()
        .and_then(|line| line.split('\t').next())
        .expect("command output includes a stable ID")
        .to_owned()
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
