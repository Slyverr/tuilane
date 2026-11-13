use std::{cell::Cell, rc::Rc};

use tuilane_core::{
    CommandCallerId, CommandCancellation, CommandDispatchError, CommandDispatcher, CommandOutcome,
};
use tuilane_plugin_api::{
    Command, CommandDescriptor, CommandEffect, CommandPermission,
    tasks::{TaskCommand, TaskCommandResult},
};

const WRITE_PERMISSION: CommandPermission = CommandPermission::new("test.records.write");

#[derive(Debug)]
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

#[derive(Debug)]
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

    let (result, receipt) = dispatcher
        .execute_as(&agent, WriteRecord("blocked".to_owned()))
        .into_parts();

    assert_eq!(
        result,
        Err(CommandDispatchError::PermissionDenied {
            command: "records.write@1",
            caller: agent,
            permission: WRITE_PERMISSION,
        })
    );
    assert!(receipt.is_none());
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
    let unrelated_permission = CommandPermission::new("test.records.read");
    dispatcher.grant_permission(&agent, unrelated_permission);

    assert!(matches!(
        dispatcher
            .execute_as(&agent, WriteRecord("denied".to_owned()))
            .into_result(),
        Err(CommandDispatchError::PermissionDenied { .. })
    ));

    dispatcher.grant_permission(&agent, WRITE_PERMISSION);
    let prepared = dispatcher
        .preview_as(&agent, WriteRecord("allowed".to_owned()))
        .unwrap();
    assert_eq!(prepared.preview().descriptor().id(), "records.write@1");
    assert!(prepared.preview().request_details().contains("allowed"));
    assert!(!invoked.get());
    assert!(matches!(
        dispatcher
            .execute_as(&agent, WriteRecord("unconfirmed".to_owned()))
            .into_result(),
        Err(CommandDispatchError::ConfirmationRequired { .. })
    ));
    assert!(!invoked.get());
    let (result, receipt) = dispatcher.confirm(prepared).into_parts();
    assert_eq!(result, Ok("allowed".to_owned()));
    let receipt = receipt.expect("authorized handler attempts include a receipt");
    assert_eq!(receipt.sequence(), 1);
    assert_eq!(receipt.caller(), &agent);
    assert_eq!(receipt.command_id(), "records.write@1");
    assert_eq!(receipt.effect(), CommandEffect::Mutating);
    assert_eq!(receipt.outcome(), tuilane_core::CommandOutcome::Succeeded);
    assert!(invoked.get());
    assert!(dispatcher.revoke_permission(&agent, WRITE_PERMISSION));
    assert!(matches!(
        dispatcher
            .execute_as(&agent, WriteRecord("revoked".to_owned()))
            .into_result(),
        Err(CommandDispatchError::PermissionDenied { .. })
    ));
}

#[test]
fn revoked_permission_prevents_confirmation_of_a_prepared_command() {
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
    dispatcher.grant_permission(&agent, WRITE_PERMISSION);
    let prepared = dispatcher
        .preview_as(&agent, WriteRecord("pending".to_owned()))
        .unwrap();
    assert!(dispatcher.revoke_permission(&agent, WRITE_PERMISSION));

    let (result, receipt) = dispatcher.confirm(prepared).into_parts();
    assert!(matches!(
        result,
        Err(CommandDispatchError::PermissionDenied { .. })
    ));
    assert!(receipt.is_none());
    assert!(!invoked.get());
}

#[test]
fn cancellation_before_confirmation_prevents_handler_execution() {
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
    dispatcher.grant_permission(&agent, WRITE_PERMISSION);
    let prepared = dispatcher
        .preview_as(&agent, WriteRecord("cancelled".to_owned()))
        .unwrap();
    let cancellation = CommandCancellation::new();
    assert!(cancellation.cancel());

    let (result, receipt) = dispatcher
        .confirm_with_cancellation(prepared, &cancellation)
        .into_parts();

    assert_eq!(
        result,
        Err(CommandDispatchError::Cancelled {
            command: "records.write@1"
        })
    );
    assert!(receipt.is_none());
    assert!(!invoked.get());
}

#[test]
fn handler_observes_cooperative_cancellation_and_returns_a_cancelled_receipt() {
    let cancellation = CommandCancellation::new();
    let handler_cancellation = cancellation.clone();
    let mut dispatcher = CommandDispatcher::new();
    dispatcher
        .register_with_context(move |WriteRecord(value), context| {
            assert!(!context.is_cancelled());
            assert!(handler_cancellation.cancel());
            assert!(context.is_cancelled());
            assert!(context.acknowledge_cancellation());
            Ok::<_, &'static str>(value)
        })
        .unwrap();
    let agent = CommandCallerId::new("agent.test").unwrap();
    dispatcher.grant_permission(&agent, WRITE_PERMISSION);
    let prepared = dispatcher
        .preview_as(&agent, WriteRecord("cancel during handler".to_owned()))
        .unwrap();

    let (result, receipt) = dispatcher
        .confirm_with_cancellation(prepared, &cancellation)
        .into_parts();

    assert_eq!(
        result,
        Err(CommandDispatchError::Cancelled {
            command: "records.write@1"
        })
    );
    let receipt = receipt.expect("started command execution includes a receipt");
    assert_eq!(receipt.outcome(), CommandOutcome::Cancelled);
    assert_eq!(receipt.sequence(), 1);
}

#[test]
fn handler_completion_is_not_discarded_when_it_does_not_acknowledge_cancellation() {
    let cancellation = CommandCancellation::new();
    let handler_cancellation = cancellation.clone();
    let mut dispatcher = CommandDispatcher::new();
    dispatcher
        .register_with_context(move |WriteRecord(value), _context| {
            assert!(handler_cancellation.cancel());
            Ok::<_, &'static str>(value)
        })
        .unwrap();
    let agent = CommandCallerId::new("agent.test").unwrap();
    dispatcher.grant_permission(&agent, WRITE_PERMISSION);
    let prepared = dispatcher
        .preview_as(&agent, WriteRecord("completed write".to_owned()))
        .unwrap();

    let (result, receipt) = dispatcher
        .confirm_with_cancellation(prepared, &cancellation)
        .into_parts();

    assert_eq!(result, Ok("completed write".to_owned()));
    let receipt = receipt.expect("completed handler includes a receipt");
    assert_eq!(receipt.outcome(), CommandOutcome::Succeeded);
}

#[test]
fn handler_failure_returns_a_failed_receipt_without_error_payloads() {
    let mut dispatcher = CommandDispatcher::new();
    dispatcher
        .register(|_: WriteRecord| Err::<String, _>("private storage detail"))
        .unwrap();
    let agent = CommandCallerId::new("agent.test").unwrap();
    dispatcher.grant_permission(&agent, WRITE_PERMISSION);
    let prepared = dispatcher
        .preview_as(&agent, WriteRecord("value".to_owned()))
        .unwrap();

    let (result, receipt) = dispatcher.confirm(prepared).into_parts();

    assert!(matches!(
        result,
        Err(CommandDispatchError::HandlerFailed { ref message, .. })
            if message == "private storage detail"
    ));
    let receipt = receipt.expect("handler attempts include a receipt");
    assert_eq!(receipt.outcome(), tuilane_core::CommandOutcome::Failed);
    assert_eq!(receipt.command_id(), "records.write@1");
}

#[test]
fn request_preview_escapes_terminal_control_characters() {
    let mut dispatcher = CommandDispatcher::new();
    dispatcher
        .register(|WriteRecord(value)| Ok::<_, &'static str>(value))
        .unwrap();
    let agent = CommandCallerId::new("agent.test").unwrap();
    dispatcher.grant_permission(&agent, WRITE_PERMISSION);

    let prepared = dispatcher
        .preview_as(&agent, WriteRecord("\u{1b}[2J".to_owned()))
        .unwrap();
    let details = prepared.preview().request_details();

    assert!(!details.contains('\u{1b}'));
    assert!(details.contains("\\u{1b}"));
}

#[test]
fn caller_ids_reject_invalid_characters_and_oversized_values() {
    assert!(CommandCallerId::new("agent.test-1").is_ok());
    assert!(CommandCallerId::new("Agent.Test").is_err());
    assert!(CommandCallerId::new("agent test").is_err());
    assert!(CommandCallerId::new("a".repeat(129)).is_err());
}
