use std::{error::Error, process};

use clap::Parser;
use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Paragraph, Wrap},
};
use tuilane_plugin_api::{ViewDescriptor, ViewId};
use tuilane_ui::{View, Workspace};

#[derive(Debug, Parser)]
#[command(
    name = "tuilane",
    version,
    about = "A local-first, modular, plugin-driven terminal workspace"
)]
struct Cli;

fn main() {
    let _ = Cli::parse();

    if let Err(error) = run_workspace() {
        eprintln!("tuilane: {error}");
        process::exit(1);
    }
}

fn run_workspace() -> Result<(), Box<dyn Error>> {
    let mut workspace = Workspace::new();
    workspace.register_view(OverviewView::new())?;
    workspace.register_view(ShortcutsView::new())?;
    tuilane_ui::run(workspace)?;
    Ok(())
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
}
