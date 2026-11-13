use std::collections::HashSet;

use tuilane_plugin_api::{
    Command, CommandDescriptor, CommandEffect,
    ideas::IdeaCommand,
    items::ItemRef,
    links::LinkCommand,
    notes::NoteCommand,
    projects::ProjectCommand,
    search::{
        DeleteSavedSearchCommand, ListSavedSearchesCommand, SaveSavedSearchCommand, SavedSearch,
        SavedSearchName, SearchCommand, SearchQuery,
    },
    tasks::TaskCommand,
};

fn permission_name(command: &impl Command) -> &'static str {
    command.required_permission().as_str()
}

#[test]
fn task_operations_use_read_write_and_delete_permissions() {
    let task_id =
        tuilane_plugin_api::tasks::TaskId::new("550e8400-e29b-41d4-a716-446655440000").unwrap();

    assert_eq!(
        permission_name(&TaskCommand::List {
            include_completed: false,
        }),
        "tasks.read"
    );
    assert_eq!(
        permission_name(&TaskCommand::Create {
            title: tuilane_plugin_api::tasks::TaskTitle::new("Write tests").unwrap(),
            due_date: None,
        }),
        "tasks.write"
    );
    assert_eq!(
        permission_name(&TaskCommand::CompleteMany {
            expected_tasks: Vec::new(),
        }),
        "tasks.write"
    );
    assert_eq!(
        permission_name(&TaskCommand::Delete { id: task_id }),
        "tasks.delete"
    );
}

#[test]
fn project_operations_use_read_write_and_delete_permissions() {
    let project_id =
        tuilane_plugin_api::projects::ProjectId::new("550e8400-e29b-41d4-a716-446655440001")
            .unwrap();

    assert_eq!(permission_name(&ProjectCommand::List), "projects.read");
    assert_eq!(
        permission_name(&ProjectCommand::Create {
            title: tuilane_plugin_api::projects::ProjectTitle::new("Workspace").unwrap(),
        }),
        "projects.write"
    );
    assert_eq!(
        permission_name(&ProjectCommand::Delete { id: project_id }),
        "projects.delete"
    );
}

#[test]
fn note_operations_use_read_write_and_delete_permissions() {
    let note_id =
        tuilane_plugin_api::notes::NoteId::new("550e8400-e29b-41d4-a716-446655440002").unwrap();

    assert_eq!(permission_name(&NoteCommand::List), "notes.read");
    assert_eq!(
        permission_name(&NoteCommand::Create {
            title: tuilane_plugin_api::notes::NoteTitle::new("Research").unwrap(),
            body: tuilane_plugin_api::notes::Markdown::new("Notes").unwrap(),
        }),
        "notes.write"
    );
    assert_eq!(
        permission_name(&NoteCommand::Delete { id: note_id }),
        "notes.delete"
    );
}

#[test]
fn idea_operations_use_read_write_and_delete_permissions() {
    let idea_id =
        tuilane_plugin_api::ideas::IdeaId::new("550e8400-e29b-41d4-a716-446655440003").unwrap();

    assert_eq!(permission_name(&IdeaCommand::List), "ideas.read");
    assert_eq!(
        permission_name(&IdeaCommand::Create {
            title: tuilane_plugin_api::ideas::IdeaTitle::new("Prototype").unwrap(),
        }),
        "ideas.write"
    );
    assert_eq!(
        permission_name(&IdeaCommand::Delete { id: idea_id }),
        "ideas.delete"
    );
}

#[test]
fn link_operations_use_distinct_read_write_and_delete_permissions() {
    let item = ItemRef::new("task", "task-1").unwrap();

    assert_eq!(
        permission_name(&LinkCommand::Outgoing { item: item.clone() }),
        "links.read"
    );
    assert_eq!(
        permission_name(&LinkCommand::Add {
            source: item.clone(),
            target: ItemRef::new("note", "note-1").unwrap(),
        }),
        "links.write"
    );
    assert_eq!(
        permission_name(&LinkCommand::Remove {
            source: item,
            target: ItemRef::new("note", "note-1").unwrap(),
        }),
        "links.delete"
    );
}

#[test]
fn search_operations_separate_query_and_saved_search_access() {
    assert_eq!(
        permission_name(&SearchCommand::new(SearchQuery::new("roadmap").unwrap())),
        "search.read"
    );
    assert_eq!(
        permission_name(&SaveSavedSearchCommand::new(
            SavedSearch::new(
                SavedSearchName::new("Roadmap").unwrap(),
                SearchQuery::new("roadmap").unwrap(),
                None,
            )
            .unwrap(),
        )),
        "search.saved.write"
    );
    assert_eq!(
        permission_name(&ListSavedSearchesCommand),
        "search.saved.read"
    );
    assert_eq!(
        permission_name(&DeleteSavedSearchCommand::new(
            SavedSearchName::new("Roadmap").unwrap(),
        )),
        "search.saved.delete"
    );
}

#[test]
fn task_descriptors_expose_stable_ids_shapes_and_effects() {
    let create = TaskCommand::Create {
        title: tuilane_plugin_api::tasks::TaskTitle::new("Write tests").unwrap(),
        due_date: None,
    };
    let descriptor = create.descriptor();

    assert_eq!(descriptor.id(), "tasks.create@1");
    assert_eq!(descriptor.summary(), "Create a task.");
    assert_eq!(descriptor.input(), "title: TaskTitle, due_date?: TaskDate");
    assert_eq!(descriptor.output(), "TaskCommandResult::Task");
    assert_eq!(descriptor.effect(), CommandEffect::Mutating);
    assert_eq!(descriptor.required_permission().as_str(), "tasks.write");

    let delete = TaskCommand::Delete {
        id: tuilane_plugin_api::tasks::TaskId::new("550e8400-e29b-41d4-a716-446655440000").unwrap(),
    };
    assert_eq!(delete.descriptor().effect(), CommandEffect::Destructive);

    let complete_many = TaskCommand::CompleteMany {
        expected_tasks: Vec::new(),
    };
    assert_eq!(complete_many.descriptor().id(), "tasks.complete_many@1");
    assert_eq!(complete_many.descriptor().effect(), CommandEffect::Mutating);
    assert_eq!(
        complete_many.descriptor().required_permission().as_str(),
        "tasks.write"
    );
}

#[test]
fn built_in_command_descriptors_have_unique_ids_and_complete_metadata() {
    let command_types = [
        TaskCommand::DESCRIPTORS,
        ProjectCommand::DESCRIPTORS,
        NoteCommand::DESCRIPTORS,
        IdeaCommand::DESCRIPTORS,
        LinkCommand::DESCRIPTORS,
        SaveSavedSearchCommand::DESCRIPTORS,
        ListSavedSearchesCommand::DESCRIPTORS,
        DeleteSavedSearchCommand::DESCRIPTORS,
        SearchCommand::DESCRIPTORS,
    ];
    let descriptors: Vec<CommandDescriptor> = command_types
        .iter()
        .flat_map(|descriptors| descriptors.iter().copied())
        .collect();
    let ids: HashSet<_> = descriptors
        .iter()
        .map(|descriptor| descriptor.id())
        .collect();

    assert_eq!(descriptors.len(), 36);
    assert_eq!(ids.len(), descriptors.len());
    assert!(descriptors.iter().all(|descriptor| {
        !descriptor.summary().is_empty()
            && !descriptor.input().is_empty()
            && !descriptor.output().is_empty()
            && !descriptor.required_permission().as_str().is_empty()
    }));
}
