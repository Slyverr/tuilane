use std::{any::Any, any::TypeId, collections::HashMap, fmt};

use tuilane_plugin_api::Command;

type ErasedHandler = Box<dyn FnMut(Box<dyn Any>) -> Result<Box<dyn Any>, CommandDispatchError>>;

/// Host command router shared by CLI and TUI callers.
#[derive(Default)]
pub struct CommandDispatcher {
    handlers: HashMap<TypeId, ErasedHandler>,
}

/// Error returned when a command handler cannot be registered.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandRegistrationError {
    DuplicateCommand(&'static str),
}

impl fmt::Display for CommandRegistrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateCommand(name) => {
                write!(formatter, "command handler '{name}' is already registered")
            }
        }
    }
}

impl std::error::Error for CommandRegistrationError {}

/// Error returned while dispatching a command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandDispatchError {
    MissingHandler(&'static str),
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

        self.handlers.insert(
            type_id,
            Box::new(move |command| {
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
        );
        Ok(())
    }

    /// Executes a command through its registered host handler.
    pub fn execute<C: Command>(&mut self, command: C) -> Result<C::Response, CommandDispatchError> {
        let handler = self
            .handlers
            .get_mut(&TypeId::of::<C>())
            .ok_or(CommandDispatchError::MissingHandler(C::NAME))?;
        let response = handler(Box::new(command))?;
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

    impl Command for Echo {
        type Response = String;
        const NAME: &'static str = "echo";
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
