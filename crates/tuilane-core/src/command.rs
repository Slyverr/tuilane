use std::{
    any::Any,
    any::TypeId,
    collections::{HashMap, HashSet},
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use tuilane_plugin_api::{Command, CommandDescriptor, CommandEffect, CommandPermission};

type ErasedHandler =
    Box<dyn FnMut(Box<dyn Any>, &CommandContext) -> Result<Box<dyn Any>, CommandDispatchError>>;

struct RegisteredCommandHandler {
    handler: ErasedHandler,
    descriptors: &'static [CommandDescriptor],
}

/// Host command router shared by CLI and TUI callers.
#[derive(Default)]
pub struct CommandDispatcher {
    handlers: HashMap<TypeId, RegisteredCommandHandler>,
    permissions: HashMap<CommandCallerId, HashSet<CommandPermission>>,
    next_receipt_sequence: u64,
}

/// User-reviewable information for one prepared operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandPreview {
    descriptor: CommandDescriptor,
    request_details: Box<str>,
}

impl CommandPreview {
    /// Returns the operation metadata shown during review.
    pub const fn descriptor(&self) -> CommandDescriptor {
        self.descriptor
    }

    /// Returns an escaped debug representation of the exact typed request.
    ///
    /// This is for local review only and is not a stable serialization format.
    pub fn request_details(&self) -> &str {
        &self.request_details
    }
}

/// A permission-checked command that has not executed and can be explicitly confirmed.
pub struct PreparedCommand<C: Command> {
    caller: CommandCallerId,
    command: C,
    preview: CommandPreview,
}

impl<C: Command> PreparedCommand<C> {
    /// Returns the preview associated with this exact, unmodified request.
    pub fn preview(&self) -> &CommandPreview {
        &self.preview
    }
}

/// Outcome of an authorized command handler attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandOutcome {
    /// The registered command handler returned successfully.
    Succeeded,
    /// The handler or response contract failed.
    Failed,
    /// Cancellation was requested while the handler was running.
    Cancelled,
}

/// Metadata receipt for one authorized handler attempt.
///
/// Receipts contain no request or response payload. Sequence numbers are unique
/// only within one dispatcher instance and are not persisted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandReceipt {
    sequence: u64,
    caller: CommandCallerId,
    command_id: &'static str,
    effect: CommandEffect,
    outcome: CommandOutcome,
}

impl CommandReceipt {
    /// Returns the receipt sequence within its dispatcher instance.
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Returns the caller assigned by the host.
    pub fn caller(&self) -> &CommandCallerId {
        &self.caller
    }

    /// Returns the stable operation identifier.
    pub const fn command_id(&self) -> &'static str {
        self.command_id
    }

    /// Returns the declared operation effect.
    pub const fn effect(&self) -> CommandEffect {
        self.effect
    }

    /// Returns whether the handler completed successfully.
    pub const fn outcome(&self) -> CommandOutcome {
        self.outcome
    }
}

/// Result and optional receipt for an explicit caller execution attempt.
#[derive(Debug)]
pub struct CommandExecution<T> {
    result: Result<T, CommandDispatchError>,
    receipt: Option<CommandReceipt>,
}

impl<T> CommandExecution<T> {
    fn rejected(error: CommandDispatchError) -> Self {
        Self {
            result: Err(error),
            receipt: None,
        }
    }

    /// Returns the handler result by reference.
    pub fn result(&self) -> Result<&T, &CommandDispatchError> {
        self.result.as_ref()
    }

    /// Returns the receipt when an authorized handler attempt occurred.
    pub fn receipt(&self) -> Option<&CommandReceipt> {
        self.receipt.as_ref()
    }

    /// Consumes the execution and returns its result, discarding any receipt.
    pub fn into_result(self) -> Result<T, CommandDispatchError> {
        self.result
    }

    /// Consumes the execution and returns both result and optional receipt.
    pub fn into_parts(self) -> (Result<T, CommandDispatchError>, Option<CommandReceipt>) {
        (self.result, self.receipt)
    }
}

/// A one-way cancellation request for a single dispatch.
///
/// Clone the token to share it with a caller that may request cancellation. Use a
/// distinct token for each dispatch.
#[derive(Clone, Debug, Default)]
pub struct CommandCancellation(Arc<AtomicBool>);

impl CommandCancellation {
    /// Creates a cancellation token that has not been cancelled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests cancellation. Returns `true` only for the first request.
    pub fn cancel(&self) -> bool {
        self.0
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    /// Returns whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Host execution controls made available to a command handler.
#[derive(Clone, Debug)]
pub struct CommandContext {
    cancellation: CommandCancellation,
}

impl CommandContext {
    /// Returns whether the caller requested that this command stop.
    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }
}

/// Identity assigned by trusted host code to a command caller.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CommandCallerId(Box<str>);

impl CommandCallerId {
    /// Creates a caller identity from lowercase ASCII letters, digits, `.`, `-`, or `_`.
    pub fn new(value: impl AsRef<str>) -> Result<Self, CommandCallerIdError> {
        let value = value.as_ref();
        if value.is_empty()
            || value.len() > 128
            || !value.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
            })
        {
            return Err(CommandCallerIdError);
        }

        Ok(Self(value.into()))
    }

    /// Returns the stable caller identifier.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Error returned when a command caller identity is empty, oversized, or malformed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommandCallerIdError;

impl fmt::Display for CommandCallerIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
            "caller identifiers must contain 1 to 128 lowercase ASCII letters, digits, '.', '-' or '_'",
        )
    }
}

impl std::error::Error for CommandCallerIdError {}

/// Error returned when a command handler cannot be registered.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandRegistrationError {
    DuplicateCommand(&'static str),
    DuplicateCommandId(&'static str),
    MissingDescriptors(&'static str),
}

impl fmt::Display for CommandRegistrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateCommand(name) => {
                write!(formatter, "command handler '{name}' is already registered")
            }
            Self::DuplicateCommandId(id) => {
                write!(formatter, "command ID '{id}' is already registered")
            }
            Self::MissingDescriptors(name) => {
                write!(
                    formatter,
                    "command handler '{name}' has no operation descriptors"
                )
            }
        }
    }
}

impl std::error::Error for CommandRegistrationError {}

/// Error returned while dispatching a command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandDispatchError {
    MissingHandler(&'static str),
    ConfirmationRequired {
        command: &'static str,
    },
    ReceiptSequenceExhausted,
    Cancelled {
        command: &'static str,
    },
    PermissionDenied {
        command: &'static str,
        caller: CommandCallerId,
        permission: CommandPermission,
    },
    HandlerFailed {
        command: &'static str,
        message: String,
    },
    HandlerTypeMismatch(&'static str),
    ResponseTypeMismatch(&'static str),
}

impl fmt::Display for CommandDispatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingHandler(command) => {
                write!(formatter, "command handler '{command}' is unavailable")
            }
            Self::ConfirmationRequired { command } => write!(
                formatter,
                "command '{command}' requires preview and explicit confirmation"
            ),
            Self::ReceiptSequenceExhausted => {
                formatter.write_str("command receipt sequence is exhausted")
            }
            Self::Cancelled { command } => write!(formatter, "command '{command}' was cancelled"),
            Self::PermissionDenied {
                command,
                caller,
                permission,
            } => write!(
                formatter,
                "caller '{}' is not permitted to execute command '{command}' (requires '{}')",
                caller.as_str(),
                permission.as_str()
            ),
            Self::HandlerFailed { command, message } => {
                write!(formatter, "command '{command}' failed: {message}")
            }
            Self::HandlerTypeMismatch(command) => {
                write!(
                    formatter,
                    "command handler '{command}' received an invalid request"
                )
            }
            Self::ResponseTypeMismatch(command) => {
                write!(
                    formatter,
                    "command handler '{command}' returned an invalid response"
                )
            }
        }
    }
}

impl std::error::Error for CommandDispatchError {}

impl CommandDispatcher {
    /// Creates an empty host command router.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a typed command handler.
    pub fn register<C, F, E>(&mut self, mut handler: F) -> Result<(), CommandRegistrationError>
    where
        C: Command,
        F: FnMut(C) -> Result<C::Response, E> + 'static,
        E: fmt::Display,
    {
        self.register_with_context(move |command, _context| handler(command))
    }

    /// Registers a typed handler that can observe cooperative cancellation.
    pub fn register_with_context<C, F, E>(
        &mut self,
        mut handler: F,
    ) -> Result<(), CommandRegistrationError>
    where
        C: Command,
        F: FnMut(C, &CommandContext) -> Result<C::Response, E> + 'static,
        E: fmt::Display,
    {
        let type_id = TypeId::of::<C>();
        if self.handlers.contains_key(&type_id) {
            return Err(CommandRegistrationError::DuplicateCommand(C::NAME));
        }
        if C::DESCRIPTORS.is_empty() {
            return Err(CommandRegistrationError::MissingDescriptors(C::NAME));
        }

        let mut command_ids = HashSet::new();
        for descriptor in C::DESCRIPTORS {
            if !command_ids.insert(descriptor.id())
                || self.handlers.values().any(|registered| {
                    registered
                        .descriptors
                        .iter()
                        .any(|existing| existing.id() == descriptor.id())
                })
            {
                return Err(CommandRegistrationError::DuplicateCommandId(
                    descriptor.id(),
                ));
            }
        }

        self.handlers.insert(
            type_id,
            RegisteredCommandHandler {
                handler: Box::new(move |command, context| {
                    let command = command
                        .downcast::<C>()
                        .map_err(|_| CommandDispatchError::HandlerTypeMismatch(C::NAME))?;
                    let response = handler(*command, context).map_err(|error| {
                        CommandDispatchError::HandlerFailed {
                            command: C::NAME,
                            message: error.to_string(),
                        }
                    })?;
                    Ok(Box::new(response))
                }),
                descriptors: C::DESCRIPTORS,
            },
        );
        Ok(())
    }

    /// Returns all registered operation descriptors in stable ID order.
    pub fn descriptors(&self) -> Vec<CommandDescriptor> {
        let mut descriptors: Vec<_> = self
            .handlers
            .values()
            .flat_map(|handler| handler.descriptors.iter().copied())
            .collect();
        descriptors.sort_by_key(|descriptor| descriptor.id());
        descriptors
    }

    /// Grants one command permission to a caller identity.
    pub fn grant_permission(&mut self, caller: &CommandCallerId, permission: CommandPermission) {
        self.permissions
            .entry(caller.clone())
            .or_default()
            .insert(permission);
    }

    /// Revokes one command permission from a caller identity.
    pub fn revoke_permission(
        &mut self,
        caller: &CommandCallerId,
        permission: CommandPermission,
    ) -> bool {
        let Some(permissions) = self.permissions.get_mut(caller) else {
            return false;
        };
        let removed = permissions.remove(&permission);
        let is_empty = permissions.is_empty();
        if is_empty {
            self.permissions.remove(caller);
        }
        removed
    }

    /// Executes a command through its registered host handler.
    ///
    /// This trusted in-process entry point bypasses caller permissions and the
    /// confirmation step. It is intended only for first-party host code. External
    /// or agent callers must use [`Self::execute_as`].
    pub fn execute<C: Command>(&mut self, command: C) -> Result<C::Response, CommandDispatchError> {
        let context = CommandContext {
            cancellation: CommandCancellation::new(),
        };
        self.execute_registered(command, &context)
    }

    /// Authorizes and executes a read-only command for a caller known to the host.
    ///
    /// The caller identity must be assigned by trusted host code and must not be
    /// copied from untrusted command input. Mutating commands require
    /// [`Self::preview_as`] followed by [`Self::confirm`].
    pub fn execute_as<C: Command>(
        &mut self,
        caller: &CommandCallerId,
        command: C,
    ) -> CommandExecution<C::Response> {
        let cancellation = CommandCancellation::new();
        self.execute_as_with_cancellation(caller, command, &cancellation)
    }

    /// Authorizes and executes a read-only command with cooperative cancellation.
    ///
    /// A running handler must check its [`CommandContext`] and stop at safe points.
    /// Cancellation cannot roll back side effects that a handler has already made.
    pub fn execute_as_with_cancellation<C: Command>(
        &mut self,
        caller: &CommandCallerId,
        command: C,
        cancellation: &CommandCancellation,
    ) -> CommandExecution<C::Response> {
        let descriptor = command.descriptor();
        if let Err(error) = self.authorize(caller, *descriptor) {
            return CommandExecution::rejected(error);
        }
        if descriptor.effect() != CommandEffect::ReadOnly {
            return CommandExecution::rejected(CommandDispatchError::ConfirmationRequired {
                command: descriptor.id(),
            });
        }

        self.execute_with_receipt(caller, *descriptor, command, cancellation)
    }

    /// Checks permission and prepares an operation for review without invoking its handler.
    ///
    /// The host must present [`PreparedCommand::preview`] to its trusted
    /// confirmation flow before calling [`Self::confirm`].
    pub fn preview_as<C: Command>(
        &self,
        caller: &CommandCallerId,
        command: C,
    ) -> Result<PreparedCommand<C>, CommandDispatchError> {
        let descriptor = *command.descriptor();
        self.authorize(caller, descriptor)?;
        let preview = CommandPreview {
            descriptor,
            request_details: sanitized_debug(&command).into(),
        };
        Ok(PreparedCommand {
            caller: caller.clone(),
            command,
            preview,
        })
    }

    /// Rechecks permission and executes the exact command returned by [`Self::preview_as`].
    ///
    /// Call this only after the host's confirmation flow accepts the preview.
    pub fn confirm<C: Command>(
        &mut self,
        prepared: PreparedCommand<C>,
    ) -> CommandExecution<C::Response> {
        let cancellation = CommandCancellation::new();
        self.confirm_with_cancellation(prepared, &cancellation)
    }

    /// Rechecks permission and executes a confirmed command with cooperative cancellation.
    ///
    /// A running handler must check its [`CommandContext`] and stop at safe points.
    /// Cancellation cannot roll back side effects that a handler has already made.
    pub fn confirm_with_cancellation<C: Command>(
        &mut self,
        prepared: PreparedCommand<C>,
        cancellation: &CommandCancellation,
    ) -> CommandExecution<C::Response> {
        let descriptor = prepared.preview.descriptor;
        if let Err(error) = self.authorize(&prepared.caller, descriptor) {
            return CommandExecution::rejected(error);
        }
        self.execute_with_receipt(&prepared.caller, descriptor, prepared.command, cancellation)
    }

    fn authorize(
        &self,
        caller: &CommandCallerId,
        descriptor: CommandDescriptor,
    ) -> Result<(), CommandDispatchError> {
        let permission = descriptor.required_permission();
        let permitted = self
            .permissions
            .get(caller)
            .is_some_and(|permissions| permissions.contains(&permission));
        if permitted {
            Ok(())
        } else {
            Err(CommandDispatchError::PermissionDenied {
                command: descriptor.id(),
                caller: caller.clone(),
                permission,
            })
        }
    }

    fn execute_with_receipt<C: Command>(
        &mut self,
        caller: &CommandCallerId,
        descriptor: CommandDescriptor,
        command: C,
        cancellation: &CommandCancellation,
    ) -> CommandExecution<C::Response> {
        if cancellation.is_cancelled() {
            return CommandExecution::rejected(CommandDispatchError::Cancelled {
                command: descriptor.id(),
            });
        }
        let Some(sequence) = self.next_receipt_sequence.checked_add(1) else {
            return CommandExecution::rejected(CommandDispatchError::ReceiptSequenceExhausted);
        };
        self.next_receipt_sequence = sequence;

        let context = CommandContext {
            cancellation: cancellation.clone(),
        };
        let result = self.execute_registered(command, &context);
        let cancellation_requested = cancellation.is_cancelled();
        let result = if cancellation_requested {
            Err(CommandDispatchError::Cancelled {
                command: descriptor.id(),
            })
        } else {
            result
        };
        let outcome = if cancellation_requested {
            CommandOutcome::Cancelled
        } else if result.is_ok() {
            CommandOutcome::Succeeded
        } else {
            CommandOutcome::Failed
        };
        let receipt = CommandReceipt {
            sequence,
            caller: caller.clone(),
            command_id: descriptor.id(),
            effect: descriptor.effect(),
            outcome,
        };
        CommandExecution {
            result,
            receipt: Some(receipt),
        }
    }

    fn execute_registered<C: Command>(
        &mut self,
        command: C,
        context: &CommandContext,
    ) -> Result<C::Response, CommandDispatchError> {
        let handler = self
            .handlers
            .get_mut(&TypeId::of::<C>())
            .ok_or(CommandDispatchError::MissingHandler(C::NAME))?;
        let response = (handler.handler)(Box::new(command), context)?;
        response
            .downcast::<C::Response>()
            .map(|response| *response)
            .map_err(|_| CommandDispatchError::ResponseTypeMismatch(C::NAME))
    }

    /// Returns the number of registered command handlers.
    pub fn len(&self) -> usize {
        self.handlers.len()
    }

    /// Returns whether no command handlers are registered.
    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }
}

fn sanitized_debug<C: Command>(command: &C) -> String {
    use std::fmt::Write as _;

    let details = format!("{command:#?}");
    let mut sanitized = String::with_capacity(details.len());
    for character in details.chars() {
        if character.is_control() && !matches!(character, '\n' | '\t') {
            let _ = write!(sanitized, "\\u{{{:x}}}", character as u32);
        } else {
            sanitized.push(character);
        }
    }
    sanitized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Echo(String);

    const ECHO_WRITE: CommandPermission = CommandPermission::new("test.echo.write");

    impl Command for Echo {
        type Response = String;
        const NAME: &'static str = "echo";
        const DESCRIPTORS: &'static [CommandDescriptor] = &[CommandDescriptor::new(
            "test.echo@1",
            "Echo text.",
            "text: string",
            "string",
            tuilane_plugin_api::CommandEffect::Mutating,
            ECHO_WRITE,
        )];

        fn descriptor(&self) -> &'static CommandDescriptor {
            &Self::DESCRIPTORS[0]
        }
    }

    #[test]
    fn dispatches_registered_typed_commands() {
        let mut dispatcher = CommandDispatcher::new();
        dispatcher
            .register(|Echo(value)| Ok::<_, &'static str>(value))
            .unwrap();

        assert_eq!(
            dispatcher.execute(Echo("hello".to_owned())).unwrap(),
            "hello"
        );
    }

    #[test]
    fn rejects_duplicate_handlers_and_reports_missing_handlers() {
        let mut dispatcher = CommandDispatcher::new();
        dispatcher
            .register(|Echo(value)| Ok::<_, &'static str>(value))
            .unwrap();
        assert_eq!(
            dispatcher.register(|Echo(value)| Ok::<_, &'static str>(value)),
            Err(CommandRegistrationError::DuplicateCommand("echo"))
        );

        let mut empty = CommandDispatcher::new();
        assert_eq!(
            empty.execute(Echo(String::new())),
            Err(CommandDispatchError::MissingHandler("echo"))
        );
    }

    #[test]
    fn reports_handler_failures_without_hiding_the_reason() {
        let mut dispatcher = CommandDispatcher::new();
        dispatcher
            .register(|_: Echo| Err::<String, _>("invalid request"))
            .unwrap();

        assert_eq!(
            dispatcher.execute(Echo(String::new())),
            Err(CommandDispatchError::HandlerFailed {
                command: "echo",
                message: "invalid request".to_owned(),
            })
        );
    }
}
