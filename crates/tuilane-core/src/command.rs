use std::{
    any::Any,
    any::TypeId,
    collections::{HashMap, HashSet},
    fmt,
};

use tuilane_plugin_api::{Command, CommandDescriptor, CommandPermission};

type ErasedHandler = Box<dyn FnMut(Box<dyn Any>) -> Result<Box<dyn Any>, CommandDispatchError>>;

struct RegisteredCommandHandler {
    handler: ErasedHandler,
    descriptors: &'static [CommandDescriptor],
}

/// Host command router shared by CLI and TUI callers.
#[derive(Default)]
pub struct CommandDispatcher {
    handlers: HashMap<TypeId, RegisteredCommandHandler>,
    permissions: HashMap<CommandCallerId, HashSet<CommandPermission>>,
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
                handler: Box::new(move |command| {
                    let command = command
                        .downcast::<C>()
                        .map_err(|_| CommandDispatchError::HandlerTypeMismatch(C::NAME))?;
                    let response =
                        handler(*command).map_err(|error| CommandDispatchError::HandlerFailed {
                            command: C::NAME,
                            message: error.to_string(),
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
    /// This trusted in-process entry point is intended for first-party host code.
    /// External or agent callers must use [`Self::execute_as`].
    pub fn execute<C: Command>(&mut self, command: C) -> Result<C::Response, CommandDispatchError> {
        self.execute_registered(command)
    }

    /// Authorizes and executes a command for a caller known to the host.
    ///
    /// The caller identity must be assigned by trusted host code and must not be
    /// copied from untrusted command input.
    pub fn execute_as<C: Command>(
        &mut self,
        caller: &CommandCallerId,
        command: C,
    ) -> Result<C::Response, CommandDispatchError> {
        let descriptor = command.descriptor();
        let permission = descriptor.required_permission();
        let permitted = self
            .permissions
            .get(caller)
            .is_some_and(|permissions| permissions.contains(&permission));
        if !permitted {
            return Err(CommandDispatchError::PermissionDenied {
                command: descriptor.id(),
                caller: caller.clone(),
                permission,
            });
        }

        self.execute_registered(command)
    }

    fn execute_registered<C: Command>(
        &mut self,
        command: C,
    ) -> Result<C::Response, CommandDispatchError> {
        let handler = self
            .handlers
            .get_mut(&TypeId::of::<C>())
            .ok_or(CommandDispatchError::MissingHandler(C::NAME))?;
        let response = (handler.handler)(Box::new(command))?;
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

#[cfg(test)]
mod tests {
    use super::*;

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
