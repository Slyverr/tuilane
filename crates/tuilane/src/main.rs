use std::{
    cell::RefCell,
    error::Error,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

use chrono::Local;
use clap::{Parser, Subcommand};
use directories::ProjectDirs;
use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Paragraph, Wrap},
};
use tuilane_calendar::CalendarView;
use tuilane_core::{CommandDispatcher, EventBus, ServiceBroker};
use tuilane_ideas::{IdeasPlugin, IdeasView};
use tuilane_links::{LinksPlugin, LinksView};
use tuilane_notes::{NotesPlugin, NotesView};
use tuilane_plugin_api::ideas::{Idea, IdeaCommand, IdeaCommandResult, IdeaId, IdeaTitle};
use tuilane_plugin_api::items::ItemRef;
use tuilane_plugin_api::links::{ExternalReference, LinkCommand, LinkCommandResult};
use tuilane_plugin_api::notes::{
    Markdown, Note, NoteCommand, NoteCommandResult, NoteId, NoteTitle,
};
use tuilane_plugin_api::projects::{
    Project, ProjectCommand, ProjectCommandResult, ProjectId, ProjectTitle,
};
use tuilane_plugin_api::search::{
    DeleteSavedSearchCommand, ListSavedSearchesCommand, SaveSavedSearchCommand, SavedSearch,
    SavedSearchName, SearchCommand, SearchCommandResult, SearchQuery, SearchResult,
};
use tuilane_plugin_api::tasks::{
    Task, TaskCommand, TaskCommandResult, TaskDate, TaskId, TaskTitle,
};
use tuilane_plugin_api::{ViewDescriptor, ViewId};
use tuilane_projects::{ProjectsPlugin, ProjectsView};
use tuilane_search::{SearchPlugin, SearchView};
use tuilane_storage::SqliteDatabase;
use tuilane_tasks::{TasksPlugin, TasksView};
use tuilane_ui::{View, Workspace, require_interactive_terminal};

#[derive(Debug, Parser)]
#[command(
    name = "tuilane",
    version,
    about = "A local-first, modular, plugin-driven terminal workspace"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<CliCommand>,
}

#[derive(Debug, Subcommand)]
enum CliCommand {
    /// Manage local tasks.
    Task {
        #[command(subcommand)]
        command: TaskCliCommand,
    },
    /// Manage local projects.
    Project {
        #[command(subcommand)]
        command: ProjectCliCommand,
    },
    /// Manage Markdown notes.
    Note {
        #[command(subcommand)]
        command: NoteCliCommand,
    },
    /// Manage captured ideas.
    Idea {
        #[command(subcommand)]
        command: IdeaCliCommand,
    },
    /// Manage local item links and external references.
    Link {
        #[command(subcommand)]
        command: LinkCliCommand,
    },
    /// Create or restore a portable local data backup.
    Backup {
        #[command(subcommand)]
        command: BackupCliCommand,
    },
    /// Search task, project, note, and idea content.
    Search {
        /// Plain-text terms; all terms must match.
        query: String,
        /// Restrict results to an item kind such as task, note, or project.
        #[arg(long)]
        kind: Option<String>,
        /// Save the query and optional kind filter under this name.
        #[arg(long, value_name = "NAME")]
        save: Option<String>,
    },
    /// Manage saved search views.
    SavedSearch {
        #[command(subcommand)]
        command: SavedSearchCliCommand,
    },
}

#[derive(Debug, Subcommand)]
enum BackupCliCommand {
    /// Write a standalone SQLite backup to a new file.
    Create { path: PathBuf },
    /// Replace workspace data from a backup after explicit confirmation.
    Restore {
        path: PathBuf,
        /// Confirm that current workspace data should be replaced.
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Subcommand)]
enum SavedSearchCliCommand {
    /// List saved search views.
    List,
    /// Run a saved search view by name.
    Run { name: String },
    /// Delete a saved search view by name.
    Delete { name: String },
}

#[derive(Debug, Subcommand)]
enum TaskCliCommand {
    /// List open tasks, or all tasks with --all.
    List {
        /// Include completed tasks.
        #[arg(long)]
        all: bool,
    },
    /// Create a task.
    Create {
        /// Task title.
        #[arg(long)]
        title: String,
        /// Due date in YYYY-MM-DD format.
        #[arg(long)]
        due: Option<String>,
    },
    /// Show one task.
    Show { id: String },
    /// Update one task title or due date.
    Update {
        id: String,
        /// Replace the task title.
        #[arg(long, conflicts_with_all = ["due", "clear_due"])]
        title: Option<String>,
        /// Set the due date in YYYY-MM-DD format.
        #[arg(long, conflicts_with = "clear_due")]
        due: Option<String>,
        /// Remove the current due date.
        #[arg(long, conflicts_with_all = ["title", "due"])]
        clear_due: bool,
    },
    /// Mark a task complete.
    Complete { id: String },
    /// Reopen a completed task.
    Reopen { id: String },
    /// Delete a task.
    Delete { id: String },
}

#[derive(Debug, Subcommand)]
enum ProjectCliCommand {
    /// List projects.
    List,
    /// Create a project.
    Create {
        /// Project title.
        #[arg(long)]
        title: String,
    },
    /// Show one project.
    Show { id: String },
    /// Rename one project.
    Rename {
        id: String,
        /// Replacement project title.
        #[arg(long)]
        title: String,
    },
    /// Delete one project.
    Delete { id: String },
}

#[derive(Debug, Subcommand)]
enum NoteCliCommand {
    /// List notes.
    List,
    /// Create a note by opening the configured text editor.
    Create {
        /// Note title.
        #[arg(long)]
        title: String,
    },
    /// Show a note and its Markdown source.
    Show { id: String },
    /// Edit a note's Markdown source in the configured text editor.
    Edit { id: String },
    /// Rename a note.
    Rename {
        id: String,
        /// Replacement note title.
        #[arg(long)]
        title: String,
    },
    /// Delete a note.
    Delete { id: String },
}

#[derive(Debug, Subcommand)]
enum IdeaCliCommand {
    /// List captured ideas.
    List,
    /// Capture an idea.
    Create {
        /// Idea title.
        #[arg(long)]
        title: String,
    },
    /// Show one idea.
    Show { id: String },
    /// Rename one idea.
    Rename {
        id: String,
        /// Replacement idea title.
        #[arg(long)]
        title: String,
    },
    /// Delete one idea.
    Delete { id: String },
}

#[derive(Debug, Subcommand)]
enum LinkCliCommand {
    /// Add a directed link from one item to another.
    Add { source: String, target: String },
    /// Remove a directed item link.
    Remove { source: String, target: String },
    /// List the items linked from one item.
    List { item: String },
    /// List items that link to the selected item.
    Backlinks { item: String },
    /// Attach a local file path or HTTP(S) URL to an item.
    AddReference {
        item: String,
        #[arg(long, conflicts_with = "file")]
        url: Option<String>,
        #[arg(long, conflicts_with = "url")]
        file: Option<String>,
    },
    /// Remove an external reference from an item.
    RemoveReference {
        item: String,
        #[arg(long, conflicts_with = "file")]
        url: Option<String>,
        #[arg(long, conflicts_with = "url")]
        file: Option<String>,
    },
    /// List external references attached to an item.
    References { item: String },
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Some(CliCommand::Task { command }) => run_task_command(command),
        Some(CliCommand::Project { command }) => run_project_command(command),
        Some(CliCommand::Note { command }) => run_note_command(command),
        Some(CliCommand::Idea { command }) => run_idea_command(command),
        Some(CliCommand::Link { command }) => run_link_command(command),
        Some(CliCommand::Backup { command }) => run_backup_command(command),
        Some(CliCommand::Search { query, kind, save }) => run_search_command(query, kind, save),
        Some(CliCommand::SavedSearch { command }) => run_saved_search_command(command),
        None => run_workspace(),
    };

    if let Err(error) = result {
        eprintln!("tuilane: {error}");
        process::exit(1);
    }
}

fn run_backup_command(command: BackupCliCommand) -> Result<(), Box<dyn Error>> {
    let database = SqliteDatabase::open(database_path()?)?;
    match command {
        BackupCliCommand::Create { path } => {
            database.backup_to(&path)?;
            println!("Created backup: {}", path.display());
        }
        BackupCliCommand::Restore { path, yes } => {
            if !yes {
                return Err(io::Error::other(
                    "restoring replaces all workspace data; pass --yes after verifying the backup",
                )
                .into());
            }
            database.restore_from(&path)?;
            println!("Restored workspace data from {}", path.display());
        }
    }
    Ok(())
}

fn run_search_command(
    query: String,
    kind: Option<String>,
    save_name: Option<String>,
) -> Result<(), Box<dyn Error>> {
    let query = SearchQuery::new(query)?;
    let command = SearchCommand::new(query.clone());
    let command = match kind.as_deref() {
        Some(kind) => command.with_kind(kind)?,
        None => command,
    };
    let runtime = Runtime::open()?;
    let response = runtime.commands.borrow_mut().execute(command)?;
    if let Some(name) = save_name {
        let saved = SavedSearch::new(SavedSearchName::new(name)?, query, kind.as_deref())?;
        let saved: SavedSearch = runtime
            .commands
            .borrow_mut()
            .execute(SaveSavedSearchCommand::new(saved))?;
        println!("Saved search: {}", saved.name().as_str());
    }
    print_search_response(response);
    Ok(())
}

fn run_saved_search_command(command: SavedSearchCliCommand) -> Result<(), Box<dyn Error>> {
    let runtime = Runtime::open()?;
    match command {
        SavedSearchCliCommand::List => {
            let saved: Vec<SavedSearch> = runtime
                .commands
                .borrow_mut()
                .execute(ListSavedSearchesCommand)?;
            if saved.is_empty() {
                println!("No saved searches.");
            } else {
                for search in saved {
                    println!(
                        "{}\t{}\t{}",
                        search.name().as_str(),
                        search.kind().unwrap_or("all"),
                        search.query().as_str()
                    );
                }
            }
        }
        SavedSearchCliCommand::Run { name } => {
            let name = SavedSearchName::new(name)?;
            let saved: Vec<SavedSearch> = runtime
                .commands
                .borrow_mut()
                .execute(ListSavedSearchesCommand)?;
            let saved = saved
                .into_iter()
                .find(|search| search.name().as_str().eq_ignore_ascii_case(name.as_str()))
                .ok_or_else(|| {
                    io::Error::other(format!("saved search '{}' was not found", name.as_str()))
                })?;
            let response = runtime.commands.borrow_mut().execute(saved.command()?)?;
            print_search_response(response);
        }
        SavedSearchCliCommand::Delete { name } => {
            let name = SavedSearchName::new(name)?;
            let deleted: bool = runtime
                .commands
                .borrow_mut()
                .execute(DeleteSavedSearchCommand::new(name.clone()))?;
            if !deleted {
                return Err(io::Error::other(format!(
                    "saved search '{}' was not found",
                    name.as_str()
                ))
                .into());
            }
            println!("Deleted saved search: {}", name.as_str());
        }
    }
    Ok(())
}

fn run_workspace() -> Result<(), Box<dyn Error>> {
    require_interactive_terminal()?;
    let runtime = Runtime::open()?;
    let mut workspace = Workspace::new();
    workspace.register_view(TasksView::new(runtime.commands.clone()))?;
    workspace.register_view(CalendarView::new(
        Local::now().date_naive(),
        runtime.services.clone(),
        &runtime.events,
    ))?;
    workspace.register_view(ProjectsView::new(runtime.commands.clone(), &runtime.events))?;
    workspace.register_view(NotesView::new(runtime.commands.clone(), &runtime.events))?;
    workspace.register_view(IdeasView::new(runtime.commands.clone(), &runtime.events))?;
    workspace.register_view(LinksView::new(runtime.commands.clone(), &runtime.events))?;
    workspace.register_view(SearchView::new(runtime.commands.clone()))?;
    workspace.register_view(OverviewView::new())?;
    workspace.register_view(ShortcutsView::new())?;
    tuilane_ui::run(workspace)?;
    Ok(())
}

fn run_task_command(command: TaskCliCommand) -> Result<(), Box<dyn Error>> {
    let command = match command {
        TaskCliCommand::List { all } => TaskCommand::List {
            include_completed: all,
        },
        TaskCliCommand::Create { title, due } => TaskCommand::Create {
            title: TaskTitle::new(title)?,
            due_date: due.map(TaskDate::parse).transpose()?,
        },
        TaskCliCommand::Show { id } => TaskCommand::Get {
            id: TaskId::new(id)?,
        },
        TaskCliCommand::Update {
            id,
            title,
            due,
            clear_due,
        } => {
            let id = TaskId::new(id)?;
            match (title, due, clear_due) {
                (Some(title), None, false) => TaskCommand::Rename {
                    id,
                    title: TaskTitle::new(title)?,
                },
                (None, Some(due), false) => TaskCommand::SetDueDate {
                    id,
                    due_date: Some(TaskDate::parse(due)?),
                },
                (None, None, true) => TaskCommand::SetDueDate { id, due_date: None },
                _ => {
                    return Err(io::Error::other(
                        "choose exactly one of --title, --due, or --clear-due",
                    )
                    .into());
                }
            }
        }
        TaskCliCommand::Complete { id } => TaskCommand::Complete {
            id: TaskId::new(id)?,
        },
        TaskCliCommand::Reopen { id } => TaskCommand::Reopen {
            id: TaskId::new(id)?,
        },
        TaskCliCommand::Delete { id } => TaskCommand::Delete {
            id: TaskId::new(id)?,
        },
    };

    let runtime = Runtime::open()?;
    let response = runtime.commands.borrow_mut().execute(command)?;
    print_task_response(response);
    Ok(())
}

fn run_project_command(command: ProjectCliCommand) -> Result<(), Box<dyn Error>> {
    let command = match command {
        ProjectCliCommand::List => ProjectCommand::List,
        ProjectCliCommand::Create { title } => ProjectCommand::Create {
            title: ProjectTitle::new(title)?,
        },
        ProjectCliCommand::Show { id } => ProjectCommand::Get {
            id: ProjectId::new(id)?,
        },
        ProjectCliCommand::Rename { id, title } => ProjectCommand::Rename {
            id: ProjectId::new(id)?,
            title: ProjectTitle::new(title)?,
        },
        ProjectCliCommand::Delete { id } => ProjectCommand::Delete {
            id: ProjectId::new(id)?,
        },
    };

    let runtime = Runtime::open()?;
    let response = runtime.commands.borrow_mut().execute(command)?;
    print_project_response(response);
    Ok(())
}

fn run_note_command(command: NoteCliCommand) -> Result<(), Box<dyn Error>> {
    match command {
        NoteCliCommand::List => {
            let runtime = Runtime::open()?;
            let response = runtime.commands.borrow_mut().execute(NoteCommand::List)?;
            print_note_response(response);
        }
        NoteCliCommand::Create { title } => {
            let title = NoteTitle::new(title)?;
            let body = Markdown::new(edit_markdown("")?)?;
            let runtime = Runtime::open()?;
            let response = runtime
                .commands
                .borrow_mut()
                .execute(NoteCommand::Create { title, body })?;
            print_note_response(response);
        }
        NoteCliCommand::Show { id } => {
            let runtime = Runtime::open()?;
            let response = runtime.commands.borrow_mut().execute(NoteCommand::Get {
                id: NoteId::new(id)?,
            })?;
            print_note_response(response);
        }
        NoteCliCommand::Edit { id } => {
            let id = NoteId::new(id)?;
            let runtime = Runtime::open()?;
            let note = match runtime
                .commands
                .borrow_mut()
                .execute(NoteCommand::Get { id: id.clone() })?
            {
                NoteCommandResult::Note(note) => note,
                _ => {
                    return Err(
                        io::Error::other("note query returned an unexpected response").into(),
                    );
                }
            };
            let body = Markdown::new(edit_markdown(note.body().as_str())?)?;
            let response = runtime
                .commands
                .borrow_mut()
                .execute(NoteCommand::SetBody { id, body })?;
            print_note_response(response);
        }
        NoteCliCommand::Rename { id, title } => {
            let runtime = Runtime::open()?;
            let response = runtime.commands.borrow_mut().execute(NoteCommand::Rename {
                id: NoteId::new(id)?,
                title: NoteTitle::new(title)?,
            })?;
            print_note_response(response);
        }
        NoteCliCommand::Delete { id } => {
            let runtime = Runtime::open()?;
            let response = runtime.commands.borrow_mut().execute(NoteCommand::Delete {
                id: NoteId::new(id)?,
            })?;
            print_note_response(response);
        }
    }
    Ok(())
}

fn run_idea_command(command: IdeaCliCommand) -> Result<(), Box<dyn Error>> {
    let command = match command {
        IdeaCliCommand::List => IdeaCommand::List,
        IdeaCliCommand::Create { title } => IdeaCommand::Create {
            title: IdeaTitle::new(title)?,
        },
        IdeaCliCommand::Show { id } => IdeaCommand::Get {
            id: IdeaId::new(id)?,
        },
        IdeaCliCommand::Rename { id, title } => IdeaCommand::Rename {
            id: IdeaId::new(id)?,
            title: IdeaTitle::new(title)?,
        },
        IdeaCliCommand::Delete { id } => IdeaCommand::Delete {
            id: IdeaId::new(id)?,
        },
    };

    let runtime = Runtime::open()?;
    let response = runtime.commands.borrow_mut().execute(command)?;
    print_idea_response(response);
    Ok(())
}

fn run_link_command(command: LinkCliCommand) -> Result<(), Box<dyn Error>> {
    let command = match command {
        LinkCliCommand::Add { source, target } => LinkCommand::Add {
            source: source.parse::<ItemRef>()?,
            target: target.parse::<ItemRef>()?,
        },
        LinkCliCommand::Remove { source, target } => LinkCommand::Remove {
            source: source.parse::<ItemRef>()?,
            target: target.parse::<ItemRef>()?,
        },
        LinkCliCommand::List { item } => LinkCommand::Outgoing {
            item: item.parse::<ItemRef>()?,
        },
        LinkCliCommand::Backlinks { item } => LinkCommand::Backlinks {
            item: item.parse::<ItemRef>()?,
        },
        LinkCliCommand::AddReference { item, url, file } => LinkCommand::AddReference {
            owner: item.parse::<ItemRef>()?,
            reference: parse_external_reference(url, file)?,
        },
        LinkCliCommand::RemoveReference { item, url, file } => LinkCommand::RemoveReference {
            owner: item.parse::<ItemRef>()?,
            reference: parse_external_reference(url, file)?,
        },
        LinkCliCommand::References { item } => LinkCommand::References {
            owner: item.parse::<ItemRef>()?,
        },
    };

    let runtime = Runtime::open()?;
    let response = runtime.commands.borrow_mut().execute(command)?;
    print_link_response(response);
    Ok(())
}

fn parse_external_reference(
    url: Option<String>,
    file: Option<String>,
) -> Result<ExternalReference, Box<dyn Error>> {
    match (url, file) {
        (Some(url), None) => Ok(ExternalReference::url(url)?),
        (None, Some(file)) => Ok(ExternalReference::file(file)?),
        _ => Err(io::Error::other("choose exactly one of --url or --file").into()),
    }
}

fn edit_markdown(initial: &str) -> Result<String, Box<dyn Error>> {
    let editor = std::env::var_os("VISUAL")
        .filter(|value| !value.is_empty())
        .or_else(|| std::env::var_os("EDITOR").filter(|value| !value.is_empty()))
        .ok_or_else(|| io::Error::other("set VISUAL or EDITOR to edit Markdown notes"))?;
    let temporary_file = TemporaryMarkdown::new(initial)?;
    let status = process::Command::new(editor)
        .arg(temporary_file.path())
        .status()?;
    if !status.success() {
        return Err(io::Error::other(format!("editor exited with status {status}")).into());
    }
    Ok(fs::read_to_string(temporary_file.path())?)
}

struct TemporaryMarkdown(PathBuf);

impl TemporaryMarkdown {
    fn new(initial: &str) -> io::Result<Self> {
        static NEXT_FILE: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir();
        for _ in 0..16 {
            let serial = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(io::Error::other)?
                .as_nanos();
            let path = directory.join(format!(
                "tuilane-note-{}-{timestamp}-{serial}.md",
                process::id()
            ));
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            match options.open(&path) {
                Ok(mut file) => {
                    if let Err(error) = file.write_all(initial.as_bytes()) {
                        let _ = fs::remove_file(&path);
                        return Err(error);
                    }
                    return Ok(Self(path));
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate a temporary Markdown file",
        ))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TemporaryMarkdown {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn print_task_response(response: TaskCommandResult) {
    match response {
        TaskCommandResult::Task(task) => print_task(&task),
        TaskCommandResult::Tasks(tasks) if tasks.is_empty() => println!("No tasks."),
        TaskCommandResult::Tasks(tasks) => {
            println!("ID\tSTATUS\tDUE\tTITLE");
            for task in &tasks {
                print_task(task);
            }
        }
        TaskCommandResult::Deleted(id) => println!("Deleted task {id}."),
    }
}

fn print_task(task: &Task) {
    let due_date = task.due_date().map_or("-", TaskDate::as_str);
    println!(
        "{}\t{}\t{}\t{}",
        task.id(),
        task.status(),
        due_date,
        task.title()
    );
}

fn print_project_response(response: ProjectCommandResult) {
    match response {
        ProjectCommandResult::Project(project) => print_project(&project),
        ProjectCommandResult::Projects(projects) if projects.is_empty() => println!("No projects."),
        ProjectCommandResult::Projects(projects) => {
            println!("ID\tTITLE\tCREATED\tUPDATED");
            for project in &projects {
                print_project(project);
            }
        }
        ProjectCommandResult::Deleted(id) => println!("Deleted project {id}."),
    }
}

fn print_project(project: &Project) {
    println!(
        "{}\t{}\t{}\t{}",
        project.id(),
        project.title(),
        project.created_at(),
        project.updated_at()
    );
}

fn print_note_response(response: NoteCommandResult) {
    match response {
        NoteCommandResult::Note(note) => print_note(&note),
        NoteCommandResult::Notes(notes) if notes.is_empty() => println!("No notes."),
        NoteCommandResult::Notes(notes) => {
            println!("ID\tTITLE\tUPDATED");
            for note in &notes {
                println!("{}\t{}\t{}", note.id(), note.title(), note.updated_at());
            }
        }
        NoteCommandResult::Deleted(id) => println!("Deleted note {id}."),
    }
}

fn print_note(note: &Note) {
    println!(
        "{}\t{}\t{}\t{}",
        note.id(),
        note.title(),
        note.created_at(),
        note.updated_at()
    );
    println!("---");
    println!("{}", note.body().as_str());
}

fn print_idea_response(response: IdeaCommandResult) {
    match response {
        IdeaCommandResult::Idea(idea) => print_idea(&idea),
        IdeaCommandResult::Ideas(ideas) if ideas.is_empty() => println!("No ideas."),
        IdeaCommandResult::Ideas(ideas) => {
            println!("ID\tTITLE\tUPDATED");
            for idea in &ideas {
                print_idea(idea);
            }
        }
        IdeaCommandResult::Deleted(id) => println!("Deleted idea {id}."),
    }
}

fn print_idea(idea: &Idea) {
    println!(
        "{}\t{}\t{}\t{}",
        idea.id(),
        idea.title(),
        idea.created_at(),
        idea.updated_at()
    );
}

fn print_link_response(response: LinkCommandResult) {
    match response {
        LinkCommandResult::Link(link) => {
            println!("Linked {} -> {}.", link.source(), link.target());
        }
        LinkCommandResult::Removed(link) => {
            println!("Removed link {} -> {}.", link.source(), link.target());
        }
        LinkCommandResult::Items(items) if items.is_empty() => println!("No related items."),
        LinkCommandResult::Items(items) => {
            println!("ITEM\tTITLE");
            for item in items {
                println!("{}\t{}", item.reference(), item.title());
            }
        }
        LinkCommandResult::Reference(reference) => {
            println!(
                "Attached {} reference: {}",
                reference.kind().as_str(),
                reference.value()
            );
        }
        LinkCommandResult::References(references) if references.is_empty() => {
            println!("No external references.");
        }
        LinkCommandResult::References(references) => {
            println!("TYPE\tREFERENCE");
            for reference in references {
                println!("{}\t{}", reference.kind().as_str(), reference.value());
            }
        }
    }
}

fn print_search_response(response: SearchCommandResult) {
    let SearchCommandResult::Results(results) = response;
    if results.is_empty() {
        println!("No matching items.");
        return;
    }
    println!("ITEM\tTITLE\tMATCH");
    for result in results {
        print_search_result(&result);
    }
}

fn print_search_result(result: &SearchResult) {
    println!(
        "{}\t{}\t{}",
        result.item().reference(),
        single_line(result.item().title()),
        single_line(result.excerpt())
    );
}

fn single_line(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect()
}

struct Runtime {
    commands: Rc<RefCell<CommandDispatcher>>,
    services: Rc<RefCell<ServiceBroker>>,
    events: EventBus,
    _search: SearchPlugin,
}

impl Runtime {
    fn open() -> Result<Self, Box<dyn Error>> {
        let database = SqliteDatabase::open(database_path()?)?;
        let tasks = TasksPlugin::new(database.clone())?;
        let projects = ProjectsPlugin::new(database.clone())?;
        let notes = NotesPlugin::new(database.clone())?;
        let ideas = IdeasPlugin::new(database.clone())?;
        let links = LinksPlugin::new(database.clone())?;
        let mut dispatcher = CommandDispatcher::new();
        let services = Rc::new(RefCell::new(ServiceBroker::new()));
        let events = EventBus::new();
        tasks.register(&mut dispatcher, &mut services.borrow_mut(), events.clone())?;
        projects.register(&mut dispatcher, &mut services.borrow_mut(), events.clone())?;
        notes.register(&mut dispatcher, &mut services.borrow_mut(), events.clone())?;
        ideas.register(&mut dispatcher, &mut services.borrow_mut(), events.clone())?;
        links.register(&mut dispatcher, services.clone(), events.clone())?;
        let search = SearchPlugin::register(database, services.clone(), &mut dispatcher, &events)?;
        Ok(Self {
            commands: Rc::new(RefCell::new(dispatcher)),
            services,
            events,
            _search: search,
        })
    }
}

fn database_path() -> Result<PathBuf, Box<dyn Error>> {
    if let Some(data_dir) = std::env::var_os("TUILANE_DATA_DIR") {
        return Ok(PathBuf::from(data_dir).join("tuilane.sqlite3"));
    }
    let directories = ProjectDirs::from("org", "tuilane", "tuilane")
        .ok_or_else(|| io::Error::other("could not determine the application data directory"))?;
    Ok(directories.data_dir().join("tuilane.sqlite3"))
}

struct OverviewView {
    descriptor: ViewDescriptor,
}

impl OverviewView {
    fn new() -> Self {
        Self {
            descriptor: ViewDescriptor::new(
                ViewId::new("overview").expect("static view identifier is valid"),
                "Overview",
                "A small example view registered by the application",
            ),
        }
    }
}

impl View for OverviewView {
    fn descriptor(&self) -> &ViewDescriptor {
        &self.descriptor
    }

    fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_widget(
            Paragraph::new(
                "The tuilane workspace shell is running.\n\nThis view is a minimal example of an independently registered contribution.",
            )
            .wrap(Wrap { trim: true }),
            area,
        );
    }

    fn render_inspector(&mut self, frame: &mut Frame<'_>, area: Rect) -> bool {
        frame.render_widget(
            Paragraph::new("Overview provides this contextual panel.").wrap(Wrap { trim: true }),
            area,
        );
        true
    }
}

struct ShortcutsView {
    descriptor: ViewDescriptor,
}

impl ShortcutsView {
    fn new() -> Self {
        Self {
            descriptor: ViewDescriptor::new(
                ViewId::new("shortcuts").expect("static view identifier is valid"),
                "Shortcuts",
                "Keyboard navigation reference",
            ),
        }
    }
}

impl View for ShortcutsView {
    fn descriptor(&self) -> &ViewDescriptor {
        &self.descriptor
    }

    fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_widget(
            Paragraph::new(
                "Tab / Shift-Tab     Move focus\nh/l                 Select a registered view\nCtrl-P              Open command palette\n                     Search actions and create items\n/ in Search         Search items\n↑/↓ in Search       Select a result\ni                   Toggle inspector\nd                   Toggle drawer\n?                   Show help\nq                   Quit",
            ),
            area,
        );
    }

    fn render_drawer(&mut self, frame: &mut Frame<'_>, area: Rect) -> bool {
        frame.render_widget(
            Paragraph::new("The drawer is an optional view contribution.")
                .wrap(Wrap { trim: true }),
            area,
        );
        true
    }
}
