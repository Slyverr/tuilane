use std::{cell::RefCell, error::Error, io, path::PathBuf, process, rc::Rc};

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
use tuilane_plugin_api::projects::{
    Project, ProjectCommand, ProjectCommandResult, ProjectId, ProjectTitle,
};
use tuilane_plugin_api::tasks::{
    Task, TaskCommand, TaskCommandResult, TaskDate, TaskId, TaskTitle,
};
use tuilane_plugin_api::{ViewDescriptor, ViewId};
use tuilane_projects::{ProjectsPlugin, ProjectsView};
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

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Some(CliCommand::Task { command }) => run_task_command(command),
        Some(CliCommand::Project { command }) => run_project_command(command),
        None => run_workspace(),
    };

    if let Err(error) = result {
        eprintln!("tuilane: {error}");
        process::exit(1);
    }
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

struct Runtime {
    commands: Rc<RefCell<CommandDispatcher>>,
    services: Rc<RefCell<ServiceBroker>>,
    events: EventBus,
}

impl Runtime {
    fn open() -> Result<Self, Box<dyn Error>> {
        let database = SqliteDatabase::open(database_path()?)?;
        let tasks = TasksPlugin::new(database.clone())?;
        let projects = ProjectsPlugin::new(database)?;
        let mut dispatcher = CommandDispatcher::new();
        let mut services = ServiceBroker::new();
        let events = EventBus::new();
        tasks.register(&mut dispatcher, &mut services, events.clone())?;
        projects.register(&mut dispatcher, &mut services, events.clone())?;
        Ok(Self {
            commands: Rc::new(RefCell::new(dispatcher)),
            services: Rc::new(RefCell::new(services)),
            events,
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
                "Tab / Shift-Tab     Move focus\nh/l                 Select a registered view\n↑/↓ in Views panel  Select a registered view\ni                   Toggle inspector\nd                   Toggle drawer\n?                   Show help\nq                   Quit",
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
