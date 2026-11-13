use std::{cell::Cell, rc::Rc};

use tuilane_core::{CommandCallerId, CommandDispatchError, CommandDispatcher};
use tuilane_plugin_api::{
    Command, CommandDescriptor, CommandEffect, CommandPermission,
    tasks::{TaskCommand, TaskCommandResult},
};

const WRITE_PERMISSION: CommandPermission = CommandPermission::new("test.records.write");

struct WriteRecord(String);

impl Command for WriteRecord {
    type Response = String;

    const NAME: &'static str = "records.write";
    const DESCRIPTORS: &'static [CommandDescriptor] = &[CommandDescriptor::new(
        "records.write@1",
        "Write a record.",
        "value: string",
        "string",
        CommandEffect::Mutating,
        WRITE_PERMISSION,
    )];

    fn descriptor(&self) -> &'static CommandDescriptor {
        &Self::DESCRIPTORS[0]
    }
}

struct DuplicateWriteRecord;

impl Command for DuplicateWriteRecord {
    type Response = ();

    const NAME: &'static str = "duplicate.write";
    const DESCRIPTORS: &'static [CommandDescriptor] = &[CommandDescriptor::new(
        "records.write@1",
        "Conflicting operation.",
        "{}",
        "()",
        CommandEffect::Mutating,
        WRITE_PERMISSION,
    )];

    fn descriptor(&self) -> &'static CommandDescriptor {
        &Self::DESCRIPTORS[0]
    }
}

#[test]
fn denied_caller_does_not_reach_the_command_handler() {
    let invoked = Rc::new(Cell::new(false));
    let mut dispatcher = CommandDispatcher::new();
    let handler_invoked = invoked.clone();
    dispatcher
        .register(move |WriteRecord(value)| {
            handler_invoked.set(true);
            Ok::<_, &'static str>(value)
        })
        .unwrap();
    let agent = CommandCallerId::new("agent.test").unwrap();

    let result = dispatcher.execute_as(&agent, WriteRecord("blocked".to_owned()));

    assert_eq!(
        result,
        Err(CommandDispatchError::PermissionDenied {
            command: "records.write@1",
            caller: agent,
            permission: WRITE_PERMISSION,
        })
    );
    assert!(!invoked.get());
}

#[test]
fn registered_operation_descriptors_are_sorted_and_complete() {
    let mut dispatcher = CommandDispatcher::new();
    dispatcher
        .register(|WriteRecord(value)| Ok::<_, &'static str>(value))
        .unwrap();
    dispatcher
        .register(|_: TaskCommand| Ok::<_, &'static str>(TaskCommandResult::Tasks(Vec::new())))
        .unwrap();

    let descriptors = dispatcher.descriptors();
    let ids: Vec<_> = descriptors
        .iter()
        .map(|descriptor| descriptor.id())
        .collect();

    assert_eq!(descriptors.len(), 9);
    assert!(ids.windows(2).all(|pair| pair[0] < pair[1]));
    let write_record = descriptors
        .iter()
        .find(|descriptor| descriptor.id() == "records.write@1")
        .unwrap();
    assert_eq!(write_record.effect(), CommandEffect::Mutating);
    assert_eq!(write_record.required_permission(), WRITE_PERMISSION);
}

#[test]
fn duplicate_operation_ids_are_rejected_across_command_types() {
    let mut dispatcher = CommandDispatcher::new();
    dispatcher
        .register(|WriteRecord(value)| Ok::<_, &'static str>(value))
        .unwrap();

    assert_eq!(
        dispatcher.register(|_: DuplicateWriteRecord| Ok::<_, &'static str>(())),
        Err(tuilane_core::CommandRegistrationError::DuplicateCommandId(
            "records.write@1"
        ))
    );
}

#[test]
fn exact_grants_allow_execution_and_revocation_denies_it_again() {
    let mut dispatcher = CommandDispatcher::new();
    dispatcher
        .register(|WriteRecord(value)| Ok::<_, &'static str>(value))
        .unwrap();
    let agent = CommandCallerId::new("agent.test").unwrap();
    let unrelated_permission = CommandPermission::new("test.records.read");
    dispatcher.grant_permission(&agent, unrelated_permission);

    assert!(matches!(
        dispatcher.execute_as(&agent, WriteRecord("denied".to_owned())),
        Err(CommandDispatchError::PermissionDenied { .. })
    ));

    dispatcher.grant_permission(&agent, WRITE_PERMISSION);
    assert_eq!(
        dispatcher.execute_as(&agent, WriteRecord("allowed".to_owned())),
        Ok("allowed".to_owned())
    );
    assert!(dispatcher.revoke_permission(&agent, WRITE_PERMISSION));
    assert!(matches!(
        dispatcher.execute_as(&agent, WriteRecord("revoked".to_owned())),
        Err(CommandDispatchError::PermissionDenied { .. })
    ));
}

#[test]
fn caller_ids_reject_invalid_characters_and_oversized_values() {
    assert!(CommandCallerId::new("agent.test-1").is_ok());
    assert!(CommandCallerId::new("Agent.Test").is_err());
    assert!(CommandCallerId::new("agent test").is_err());
    assert!(CommandCallerId::new("a".repeat(129)).is_err());
}
