//! Prompt handler trait and registry.
//!
//! Prompts are reusable message templates that MCP clients can discover and
//! fill with arguments. Each prompt has a name, description, a list of
//! expected arguments, and an async handler that generates prompt messages.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tracing::{debug, warn};

use crate::protocol::messages::PromptInfo;
use crate::types::content::PromptMessage;
use crate::types::error::PromptError;

/// Trait that must be implemented by all MCP prompt handlers.
///
/// Implementations provide the prompt metadata and the logic to generate
/// prompt messages given a set of arguments.
#[allow(
    clippy::double_must_use,
    reason = "async_trait adds must_use to its generated Future-returning methods"
)]
#[async_trait]
pub trait PromptHandler: Send + Sync + 'static {
    /// Returns metadata about this prompt (name, description, arguments).
    fn info(&self) -> PromptInfo;

    /// Generates prompt messages given the provided arguments.
    async fn get(
        &self,
        arguments: HashMap<String, String>,
    ) -> Result<Vec<PromptMessage>, PromptError>;
}

/// A registered prompt entry containing both the handler and cached metadata.
struct PromptEntry {
    handler: Arc<dyn PromptHandler>,
    #[allow(dead_code)]
    info: PromptInfo,
}

/// Registry that holds all registered prompt handlers.
///
/// Prompt metadata is cached at registration time.
#[derive(Default)]
pub struct PromptRegistry {
    prompts: HashMap<String, PromptEntry>,
    /// Cached list of prompt infos.
    cached_infos: Vec<PromptInfo>,
}

impl PromptRegistry {
    /// Creates a new empty prompt registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            prompts: HashMap::new(),
            cached_infos: Vec::new(),
        }
    }

    /// Registers a prompt handler.
    ///
    /// Prompt metadata is cached at registration time so that `list()`
    /// does not need to call `handler.info()` on every request.
    ///
    /// # Errors
    ///
    /// Returns an error if a prompt with the same name already exists.
    pub fn register(&mut self, handler: Arc<dyn PromptHandler>) -> Result<(), String> {
        let info = handler.info();
        let name = info.name.clone();
        if self.prompts.contains_key(&name) {
            return Err(format!("Duplicate prompt name: {name}"));
        }
        debug!(prompt_name = %name, "Registered prompt");
        self.cached_infos.push(info.clone());
        self.prompts.insert(name, PromptEntry { handler, info });
        Ok(())
    }

    /// Returns cached metadata for all registered prompts.
    #[must_use]
    pub fn list(&self) -> Vec<PromptInfo> {
        self.cached_infos.clone()
    }

    /// Returns the number of registered prompts.
    #[must_use]
    pub fn len(&self) -> usize {
        self.prompts.len()
    }

    /// Returns true if no prompts are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.prompts.is_empty()
    }

    /// Gets a prompt by name, filling it with the given arguments.
    ///
    /// # Errors
    ///
    /// Returns a `PromptError` if the prompt is not found or generation fails.
    pub async fn get(
        &self,
        name: &str,
        arguments: HashMap<String, String>,
    ) -> Result<Vec<PromptMessage>, PromptError> {
        let entry = self.prompts.get(name).ok_or_else(|| {
            warn!(prompt_name = %name, "Unknown prompt requested");
            PromptError::InvalidArguments(format!("Unknown prompt: {name}"))
        })?;

        debug!(prompt_name = %name, "Getting prompt");
        entry.handler.get(arguments).await
    }
}

impl std::fmt::Debug for PromptRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PromptRegistry")
            .field("prompt_count", &self.prompts.len())
            .field("prompts", &self.prompts.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::messages::PromptArgument;

    struct GreetPrompt;

    #[async_trait]
    impl PromptHandler for GreetPrompt {
        fn info(&self) -> PromptInfo {
            PromptInfo {
                name: "greet".to_string(),
                description: Some("Generate a greeting".to_string()),
                arguments: Some(vec![PromptArgument {
                    name: "name".to_string(),
                    description: Some("The name to greet".to_string()),
                    required: Some(true),
                }]),
            }
        }

        async fn get(
            &self,
            arguments: HashMap<String, String>,
        ) -> Result<Vec<PromptMessage>, PromptError> {
            let name = arguments
                .get("name")
                .ok_or_else(|| PromptError::InvalidArguments("Missing 'name'".to_string()))?;
            Ok(vec![PromptMessage::user(format!("Hello, {name}!"))])
        }
    }

    #[test]
    fn test_register_prompt() {
        let mut registry = PromptRegistry::new();
        let result = registry.register(Arc::new(GreetPrompt));
        assert!(result.is_ok());
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn test_list_prompts() {
        let mut registry = PromptRegistry::new();
        registry.register(Arc::new(GreetPrompt)).expect("register");
        let prompts = registry.list();
        assert_eq!(prompts.len(), 1);
        assert_eq!(prompts[0].name, "greet");
    }

    #[tokio::test]
    async fn test_get_prompt() {
        let mut registry = PromptRegistry::new();
        registry.register(Arc::new(GreetPrompt)).expect("register");
        let mut args = HashMap::new();
        args.insert("name".to_string(), "World".to_string());
        let messages = registry.get("greet", args).await.expect("get");
        assert_eq!(messages.len(), 1);
    }

    #[tokio::test]
    async fn test_get_unknown_prompt() {
        let registry = PromptRegistry::new();
        let result = registry.get("nonexistent", HashMap::new()).await;
        assert!(result.is_err());
    }
}
