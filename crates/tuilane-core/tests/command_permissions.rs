use std::{cell::Cell, rc::Rc};

use tuilane_core::{CommandCallerId, CommandDispatchError, CommandDispatcher};
use tuilane_plugin_api::{Command, CommandPermission};

const WRITE_PERMISSION: CommandPermission = CommandPermission::new("test.records.write");

struct WriteRecord(String);

impl Command for WriteRecord {
    type Response = String;

    const NAME: &'static str = "records.write";

    fn required_permission(&self) -> CommandPermission {
        WRITE_PERMISSION
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
            command: "records.write",
            caller: agent,
            permission: WRITE_PERMISSION,
        })
    );
    assert!(!invoked.get());
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
