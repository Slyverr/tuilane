use std::{any::Any, collections::HashMap, fmt};

use tuilane_plugin_api::{CapabilityId, ServiceRequest};

type ErasedHandler = Box<dyn FnMut(Box<dyn Any>) -> Result<Box<dyn Any>, ServiceBrokerError>>;

/// Host-managed registry and router for versioned typed service capabilities.
#[derive(Default)]
pub struct ServiceBroker {
    providers: HashMap<CapabilityId, ErasedHandler>,
}

/// Error returned when a service provider cannot be registered.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceRegistrationError {
    DuplicateProvider(CapabilityId),
}

impl fmt::Display for ServiceRegistrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateProvider(capability) => {
                write!(
                    formatter,
                    "service provider '{capability}' is already registered"
                )
            }
        }
    }
}

impl std::error::Error for ServiceRegistrationError {}

/// Error returned while discovering or invoking a host-managed service.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServiceBrokerError {
    MissingProvider(CapabilityId),
    ProviderFailed {
        capability: CapabilityId,
        message: String,
    },
    RequestTypeMismatch(CapabilityId),
    ResponseTypeMismatch(CapabilityId),
}

impl fmt::Display for ServiceBrokerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingProvider(capability) => {
                write!(formatter, "service provider '{capability}' is unavailable")
            }
            Self::ProviderFailed {
                capability,
                message,
            } => {
                write!(formatter, "service '{capability}' failed: {message}")
            }
            Self::RequestTypeMismatch(capability) => {
                write!(
                    formatter,
                    "service '{capability}' received an invalid request"
                )
            }
            Self::ResponseTypeMismatch(capability) => {
                write!(
                    formatter,
                    "service '{capability}' returned an invalid response"
                )
            }
        }
    }
}

impl std::error::Error for ServiceBrokerError {}

impl ServiceBroker {
    /// Creates an empty service registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a provider for a typed, versioned request contract.
    pub fn register<R, F, E>(&mut self, mut handler: F) -> Result<(), ServiceRegistrationError>
    where
        R: ServiceRequest,
        F: FnMut(R) -> Result<R::Response, E> + 'static,
        E: fmt::Display,
    {
        let capability = R::CAPABILITY;
        if self.providers.contains_key(&capability) {
            return Err(ServiceRegistrationError::DuplicateProvider(capability));
        }

        self.providers.insert(
            capability,
            Box::new(move |request| {
                let request = request
                    .downcast::<R>()
                    .map_err(|_| ServiceBrokerError::RequestTypeMismatch(capability))?;
                let response =
                    handler(*request).map_err(|error| ServiceBrokerError::ProviderFailed {
                        capability,
                        message: error.to_string(),
                    })?;
                Ok(Box::new(response))
            }),
        );
        Ok(())
    }

    /// Calls the compatible provider for a typed service request.
    pub fn call<R: ServiceRequest>(
        &mut self,
        request: R,
    ) -> Result<R::Response, ServiceBrokerError> {
        let capability = R::CAPABILITY;
        let provider = self
            .providers
            .get_mut(&capability)
            .ok_or(ServiceBrokerError::MissingProvider(capability))?;
        let response = provider(Box::new(request))?;
        response
            .downcast::<R::Response>()
            .map(|response| *response)
            .map_err(|_| ServiceBrokerError::ResponseTypeMismatch(capability))
    }

    /// Returns registered capabilities in stable name/version order.
    pub fn discover(&self) -> Vec<CapabilityId> {
        let mut capabilities = self.providers.keys().copied().collect::<Vec<_>>();
        capabilities.sort_unstable();
        capabilities
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Echo(String);

    impl ServiceRequest for Echo {
        type Response = String;
        const CAPABILITY: CapabilityId = CapabilityId::new("test.echo", 1);
    }

    #[test]
    fn discovers_and_calls_typed_service_providers() {
        let mut broker = ServiceBroker::new();
        broker
            .register(|Echo(value)| Ok::<_, &'static str>(value))
            .unwrap();

        assert_eq!(broker.discover(), [CapabilityId::new("test.echo", 1)]);
        assert_eq!(broker.call(Echo("hello".to_owned())).unwrap(), "hello");
    }

    #[test]
    fn reports_missing_and_duplicate_providers() {
        let mut broker = ServiceBroker::new();
        assert_eq!(
            broker.call(Echo(String::new())),
            Err(ServiceBrokerError::MissingProvider(CapabilityId::new(
                "test.echo",
                1,
            )))
        );
        broker
            .register(|Echo(value)| Ok::<_, &'static str>(value))
            .unwrap();
        assert_eq!(
            broker.register(|Echo(value)| Ok::<_, &'static str>(value)),
            Err(ServiceRegistrationError::DuplicateProvider(
                CapabilityId::new("test.echo", 1,)
            ))
        );
    }

    #[test]
    fn reports_provider_errors() {
        let mut broker = ServiceBroker::new();
        broker
            .register(|_: Echo| Err::<String, _>("invalid request"))
            .unwrap();

        assert_eq!(
            broker.call(Echo(String::new())),
            Err(ServiceBrokerError::ProviderFailed {
                capability: CapabilityId::new("test.echo", 1),
                message: "invalid request".to_owned(),
            })
        );
    }
}
