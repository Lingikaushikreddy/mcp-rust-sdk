//! Resource handler trait and registry.
//!
//! Resources provide data that MCP clients can read. Each resource has a URI,
//! name, description, optional MIME type, and an async handler that returns
//! the resource content.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tracing::{debug, warn};

use crate::protocol::messages::{ResourceInfo, ResourceTemplateInfo};
use crate::types::content::ResourceContent;
use crate::types::error::ResourceError;

/// Trait that must be implemented by all MCP resource handlers.
///
/// Implementations provide metadata about the resource and the logic
/// to read its content when requested.
#[allow(
    clippy::double_must_use,
    reason = "async_trait adds must_use to its generated Future-returning methods"
)]
#[async_trait]
pub trait ResourceHandler: Send + Sync + 'static {
    /// Returns metadata about this resource.
    fn info(&self) -> ResourceInfo;

    /// Returns template info if this is a URI template resource.
    fn template_info(&self) -> Option<ResourceTemplateInfo> {
        None
    }

    /// Returns whether this handler uses a URI template.
    fn is_template(&self) -> bool {
        false
    }

    /// Reads the resource content for the given URI.
    async fn read(&self, uri: &str) -> Result<ResourceContent, ResourceError>;
}

/// A registered resource entry containing both the handler and cached metadata.
struct ResourceEntry {
    handler: Arc<dyn ResourceHandler>,
    #[allow(dead_code)]
    info: ResourceInfo,
}

/// A registered template entry containing both the handler and cached metadata.
struct TemplateEntry {
    handler: Arc<dyn ResourceHandler>,
    template_info: ResourceTemplateInfo,
}

/// Registry that holds all registered resource handlers and dispatches reads.
///
/// Resource metadata is cached at registration time.
#[derive(Default)]
pub struct ResourceRegistry {
    /// Static resources keyed by their URI.
    resources: HashMap<String, ResourceEntry>,
    /// Template resources keyed by their URI template.
    templates: HashMap<String, TemplateEntry>,
    /// Cached list of resource infos.
    cached_infos: Vec<ResourceInfo>,
    /// Cached list of template infos.
    cached_template_infos: Vec<ResourceTemplateInfo>,
}

impl ResourceRegistry {
    /// Creates a new empty resource registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            resources: HashMap::new(),
            templates: HashMap::new(),
            cached_infos: Vec::new(),
            cached_template_infos: Vec::new(),
        }
    }

    /// Registers a resource handler.
    ///
    /// Resource metadata is cached at registration time so that `list()`
    /// does not need to call `handler.info()` on every request.
    ///
    /// # Errors
    ///
    /// Returns an error if a resource with the same URI or template already exists.
    pub fn register(&mut self, handler: Arc<dyn ResourceHandler>) -> Result<(), String> {
        if handler.is_template() {
            if let Some(template_info) = handler.template_info() {
                let key = template_info.uri_template.clone();
                if self.templates.contains_key(&key) {
                    return Err(format!("Duplicate resource template: {key}"));
                }
                debug!(uri_template = %key, "Registered resource template");
                self.cached_template_infos.push(template_info.clone());
                self.templates.insert(
                    key,
                    TemplateEntry {
                        handler,
                        template_info,
                    },
                );
            }
        } else {
            let info = handler.info();
            let key = info.uri.clone();
            if self.resources.contains_key(&key) {
                return Err(format!("Duplicate resource URI: {key}"));
            }
            debug!(uri = %key, "Registered resource");
            self.cached_infos.push(info.clone());
            self.resources.insert(key, ResourceEntry { handler, info });
        }
        Ok(())
    }

    /// Returns cached metadata for all registered static resources.
    #[must_use]
    pub fn list(&self) -> Vec<ResourceInfo> {
        self.cached_infos.clone()
    }

    /// Returns cached metadata for all registered resource templates.
    #[must_use]
    pub fn list_templates(&self) -> Vec<ResourceTemplateInfo> {
        self.cached_template_infos.clone()
    }

    /// Returns the total number of registered resources (static + templates).
    #[must_use]
    pub fn len(&self) -> usize {
        self.resources.len() + self.templates.len()
    }

    /// Returns true if no resources are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.resources.is_empty() && self.templates.is_empty()
    }

    /// Reads a resource by URI.
    ///
    /// First checks static resources for an exact match, then tries
    /// template resources by matching the URI against registered templates.
    ///
    /// # Errors
    ///
    /// Returns a `ResourceError` if no matching resource is found or reading fails.
    pub async fn read(&self, uri: &str) -> Result<ResourceContent, ResourceError> {
        // Try exact match first
        if let Some(entry) = self.resources.get(uri) {
            debug!(uri = %uri, "Reading static resource");
            return entry.handler.read(uri).await;
        }

        // Try template matching
        for entry in self.templates.values() {
            if uri_matches_template(uri, &entry.template_info.uri_template) {
                debug!(uri = %uri, template = %entry.template_info.uri_template, "Reading template resource");
                return entry.handler.read(uri).await;
            }
        }

        warn!(uri = %uri, "Resource not found");
        Err(ResourceError::NotFound(format!(
            "No resource found for URI: {uri}"
        )))
    }
}

/// Simple URI template matching.
///
/// Matches a URI against a template like `file:///{path}` by checking
/// that the static parts match and extracting variables from the dynamic parts.
fn uri_matches_template(uri: &str, template: &str) -> bool {
    let mut uri_parts = uri.chars().peekable();
    let mut tmpl_parts = template.chars().peekable();

    while tmpl_parts.peek().is_some() {
        if tmpl_parts.peek() == Some(&'{') {
            // Skip the template variable
            while tmpl_parts.peek().is_some() && tmpl_parts.peek() != Some(&'}') {
                tmpl_parts.next();
            }
            tmpl_parts.next(); // consume '}'

            // The URI should have some content here; consume until the next
            // static character in the template (or end)
            if let Some(&next_tmpl_char) = tmpl_parts.peek() {
                while uri_parts.peek().is_some() && uri_parts.peek() != Some(&next_tmpl_char) {
                    uri_parts.next();
                }
            } else {
                // Template ends with a variable, consume rest of URI
                while uri_parts.next().is_some() {}
            }
        } else {
            // Static character must match
            let tmpl_c = tmpl_parts.next();
            let uri_c = uri_parts.next();
            if tmpl_c != uri_c {
                return false;
            }
        }
    }

    // Both should be exhausted
    uri_parts.peek().is_none()
}

impl std::fmt::Debug for ResourceRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResourceRegistry")
            .field("resource_count", &self.resources.len())
            .field("template_count", &self.templates.len())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uri_matches_template() {
        assert!(uri_matches_template("file:///hello.txt", "file:///{path}"));
        assert!(uri_matches_template("db://users/42", "db://users/{id}"));
        assert!(!uri_matches_template("file:///hello.txt", "http:///{path}"));
    }

    #[test]
    fn test_empty_registry() {
        let registry = ResourceRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
        assert_eq!(registry.list(), Vec::<ResourceInfo>::new());
        assert_eq!(
            registry.list_templates(),
            Vec::<ResourceTemplateInfo>::new()
        );
    }

    struct StaticResource;

    #[async_trait]
    impl ResourceHandler for StaticResource {
        fn info(&self) -> ResourceInfo {
            ResourceInfo {
                uri: "test://data".to_string(),
                name: "test-data".to_string(),
                description: Some("Test resource".to_string()),
                mime_type: Some("text/plain".to_string()),
            }
        }

        async fn read(&self, _uri: &str) -> Result<ResourceContent, ResourceError> {
            Ok(ResourceContent::text("test://data", "hello world"))
        }
    }

    #[test]
    fn test_register_static_resource() {
        let mut registry = ResourceRegistry::new();
        registry
            .register(Arc::new(StaticResource))
            .expect("register");
        assert_eq!(registry.len(), 1);
        assert_eq!(registry.list().len(), 1);
        assert_eq!(registry.list()[0].uri, "test://data");
    }

    #[tokio::test]
    async fn test_read_static_resource() {
        let mut registry = ResourceRegistry::new();
        registry
            .register(Arc::new(StaticResource))
            .expect("register");
        let content = registry.read("test://data").await.expect("read");
        assert_eq!(content.text.as_deref(), Some("hello world"));
    }

    #[tokio::test]
    async fn test_read_unknown_resource() {
        let registry = ResourceRegistry::new();
        let result = registry.read("unknown://thing").await;
        assert!(result.is_err());
    }
}
