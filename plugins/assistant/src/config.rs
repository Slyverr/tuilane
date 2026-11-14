use std::{error::Error, fmt, net::IpAddr};

use reqwest::Url;

/// Default address for the local Ollama-compatible chat endpoint.
pub const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:11434";

/// Validated local assistant provider settings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssistantConfig {
    model: Box<str>,
    endpoint: Url,
}

/// Invalid assistant model or endpoint configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssistantConfigError {
    /// The model name is blank, too long, or contains control characters.
    InvalidModel,
    /// The endpoint is not a plain HTTP loopback base URL.
    InvalidEndpoint,
}

impl fmt::Display for AssistantConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidModel => formatter.write_str(
                "assistant model names must contain 1 to 128 non-control characters",
            ),
            Self::InvalidEndpoint => formatter.write_str(
                "assistant endpoint must be an HTTP loopback base URL without credentials or a path",
            ),
        }
    }
}

impl Error for AssistantConfigError {}

impl AssistantConfig {
    /// Creates settings for an Ollama-compatible provider bound to this machine.
    pub fn new(
        model: impl AsRef<str>,
        endpoint: impl AsRef<str>,
    ) -> Result<Self, AssistantConfigError> {
        let model = model.as_ref().trim();
        if model.is_empty() || model.chars().count() > 128 || model.chars().any(char::is_control) {
            return Err(AssistantConfigError::InvalidModel);
        }
        let endpoint = parse_endpoint(endpoint.as_ref())?;
        Ok(Self {
            model: model.into(),
            endpoint,
        })
    }

    /// Returns the configured local model name.
    pub fn model(&self) -> &str {
        &self.model
    }

    pub(crate) fn endpoint(&self) -> &Url {
        &self.endpoint
    }

    /// Checks an endpoint even when the assistant is disabled in application config.
    pub fn validate_endpoint(endpoint: &str) -> Result<(), AssistantConfigError> {
        parse_endpoint(endpoint).map(|_| ())
    }
}

fn parse_endpoint(value: &str) -> Result<Url, AssistantConfigError> {
    let url = Url::parse(value).map_err(|_| AssistantConfigError::InvalidEndpoint)?;
    let Some(host) = url.host_str() else {
        return Err(AssistantConfigError::InvalidEndpoint);
    };
    let loopback = host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback());
    if url.scheme() != "http"
        || !loopback
        || !url.username().is_empty()
        || url.password().is_some()
        || !matches!(url.path(), "" | "/")
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(AssistantConfigError::InvalidEndpoint);
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_default_loopback_provider() {
        assert!(AssistantConfig::new("qwen3:8b", DEFAULT_ENDPOINT).is_ok());
    }

    #[test]
    fn rejects_remote_or_credentialed_provider_endpoints() {
        for endpoint in [
            "https://127.0.0.1:11434",
            "http://example.com:11434",
            "http://user:password@127.0.0.1:11434",
            "http://127.0.0.1:11434/private",
        ] {
            assert_eq!(
                AssistantConfig::validate_endpoint(endpoint),
                Err(AssistantConfigError::InvalidEndpoint),
                "endpoint should be rejected: {endpoint}"
            );
        }
    }

    #[test]
    fn rejects_empty_and_oversized_model_names() {
        assert_eq!(
            AssistantConfig::new("  ", DEFAULT_ENDPOINT),
            Err(AssistantConfigError::InvalidModel)
        );
        assert_eq!(
            AssistantConfig::new("m".repeat(129), DEFAULT_ENDPOINT),
            Err(AssistantConfigError::InvalidModel)
        );
    }
}
