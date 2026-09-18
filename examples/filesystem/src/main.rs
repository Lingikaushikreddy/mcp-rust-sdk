//! Filesystem MCP Server
//!
//! An MCP server that provides file system operations as tools and resources.
//! Demonstrates tool handlers for file reading, directory listing, and
//! text search, plus a resource handler for file contents.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use mcp_sdk::context::ToolContext;
use mcp_sdk::handler::resources::ResourceHandler;
use mcp_sdk::handler::tools::ToolHandler;
use mcp_sdk::protocol::messages::{ResourceInfo, ResourceTemplateInfo, ToolInfo};
use mcp_sdk::schema::JsonSchemaBuilder;
use mcp_sdk::server::McpServer;
use mcp_sdk::transport::stdio::StdioTransport;
use mcp_sdk::types::content::{CallToolResult, ResourceContent};
use mcp_sdk::types::error::{ResourceError, ToolError};
use tracing_subscriber::EnvFilter;

// ---------------------------------------------------------------------------
// Path traversal protection
// ---------------------------------------------------------------------------

/// Validates that the given path resolves to a location within the allowed
/// base directory. This prevents path traversal attacks (e.g., `../../../etc/passwd`).
///
/// The function canonicalizes both the base directory and the requested path,
/// then checks that the canonical requested path starts with the canonical
/// base directory. Symlinks are resolved by `std::fs::canonicalize`.
fn validate_path(requested: &str, base_dir: &Path) -> Result<PathBuf, String> {
    let canonical_base = std::fs::canonicalize(base_dir).map_err(|e| {
        format!(
            "Failed to resolve base directory '{}': {e}",
            base_dir.display()
        )
    })?;

    let requested_path = if Path::new(requested).is_absolute() {
        PathBuf::from(requested)
    } else {
        canonical_base.join(requested)
    };

    let canonical_requested = std::fs::canonicalize(&requested_path)
        .map_err(|e| format!("Failed to resolve path '{}': {e}", requested_path.display()))?;

    if canonical_requested.starts_with(&canonical_base) {
        Ok(canonical_requested)
    } else {
        Err("Access denied: path is outside the allowed directory".to_string())
    }
}

// ---------------------------------------------------------------------------
// Tool: read_file
// ---------------------------------------------------------------------------

/// Tool that reads the contents of a file.
struct ReadFileTool {
    base_dir: Arc<PathBuf>,
}

#[async_trait]
impl ToolHandler for ReadFileTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "read_file".to_string(),
            description: Some("Read the contents of a file at the given path".to_string()),
            input_schema: JsonSchemaBuilder::new()
                .property(
                    "path",
                    JsonSchemaBuilder::string().description("Absolute or relative file path"),
                )
                .required("path")
                .build(),
        }
    }

    async fn call(
        &self,
        arguments: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<CallToolResult, ToolError> {
        let path = arguments
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing 'path'".to_string()))?;

        let safe_path = validate_path(path, &self.base_dir).map_err(ToolError::ExecutionError)?;

        let content = tokio::fs::read_to_string(&safe_path)
            .await
            .map_err(|e| ToolError::ExecutionError(format!("Failed to read file: {e}")))?;

        Ok(CallToolResult::text(content))
    }
}

// ---------------------------------------------------------------------------
// Tool: list_directory
// ---------------------------------------------------------------------------

/// Tool that lists the contents of a directory.
struct ListDirectoryTool {
    base_dir: Arc<PathBuf>,
}

#[async_trait]
impl ToolHandler for ListDirectoryTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "list_directory".to_string(),
            description: Some("List files and directories in the given path".to_string()),
            input_schema: JsonSchemaBuilder::new()
                .property(
                    "path",
                    JsonSchemaBuilder::string().description("Directory path to list"),
                )
                .required("path")
                .build(),
        }
    }

    async fn call(
        &self,
        arguments: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<CallToolResult, ToolError> {
        let path = arguments
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing 'path'".to_string()))?;

        let safe_path = validate_path(path, &self.base_dir).map_err(ToolError::ExecutionError)?;

        let mut entries = tokio::fs::read_dir(&safe_path)
            .await
            .map_err(|e| ToolError::ExecutionError(format!("Failed to read directory: {e}")))?;

        let mut items = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| ToolError::ExecutionError(format!("Failed to read entry: {e}")))?
        {
            let file_type = entry
                .file_type()
                .await
                .map_err(|e| ToolError::ExecutionError(format!("Failed to get file type: {e}")))?;

            let kind = if file_type.is_dir() {
                "directory"
            } else if file_type.is_file() {
                "file"
            } else {
                "symlink"
            };

            let name = entry.file_name().to_string_lossy().to_string();
            items.push(format!("[{kind}] {name}"));
        }

        items.sort();
        Ok(CallToolResult::text(items.join("\n")))
    }
}

// ---------------------------------------------------------------------------
// Tool: search_files
// ---------------------------------------------------------------------------

/// Tool that searches for files matching a pattern.
struct SearchFilesTool {
    base_dir: Arc<PathBuf>,
}

#[async_trait]
impl ToolHandler for SearchFilesTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "search_files".to_string(),
            description: Some(
                "Search for files containing the given text in a directory".to_string(),
            ),
            input_schema: JsonSchemaBuilder::new()
                .property(
                    "path",
                    JsonSchemaBuilder::string().description("Directory to search in"),
                )
                .property(
                    "query",
                    JsonSchemaBuilder::string().description("Text to search for"),
                )
                .required("path")
                .required("query")
                .build(),
        }
    }

    async fn call(
        &self,
        arguments: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<CallToolResult, ToolError> {
        let path = arguments
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing 'path'".to_string()))?;
        let query = arguments
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing 'query'".to_string()))?;

        let safe_path = validate_path(path, &self.base_dir).map_err(ToolError::ExecutionError)?;

        let mut results = Vec::new();
        search_recursive(&safe_path, query, &mut results)
            .await
            .map_err(|e| ToolError::ExecutionError(format!("Search failed: {e}")))?;

        if results.is_empty() {
            Ok(CallToolResult::text("No matches found."))
        } else {
            Ok(CallToolResult::text(results.join("\n")))
        }
    }
}

/// Recursively searches for text in files.
async fn search_recursive(
    dir: &Path,
    query: &str,
    results: &mut Vec<String>,
) -> Result<(), std::io::Error> {
    let mut entries = tokio::fs::read_dir(dir).await?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        let file_type = entry.file_type().await?;

        if file_type.is_dir() {
            // Recurse into subdirectories (skip hidden dirs)
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with('.') {
                Box::pin(search_recursive(&path, query, results)).await?;
            }
        } else if file_type.is_file() {
            // Search file contents
            if let Ok(content) = tokio::fs::read_to_string(&path).await {
                for (line_num, line) in content.lines().enumerate() {
                    if line.contains(query) {
                        results.push(format!(
                            "{}:{}: {}",
                            path.display(),
                            line_num + 1,
                            line.trim()
                        ));
                    }
                }
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Resource: file contents
// ---------------------------------------------------------------------------

/// Resource handler that reads file contents by URI template.
struct FileResource {
    base_dir: Arc<PathBuf>,
}

#[async_trait]
impl ResourceHandler for FileResource {
    fn info(&self) -> ResourceInfo {
        ResourceInfo {
            uri: "file:///{path}".to_string(),
            name: "file".to_string(),
            description: Some("Read file contents by path".to_string()),
            mime_type: Some("text/plain".to_string()),
        }
    }

    fn template_info(&self) -> Option<ResourceTemplateInfo> {
        Some(ResourceTemplateInfo {
            uri_template: "file:///{path}".to_string(),
            name: "file".to_string(),
            description: Some("Read file contents by path".to_string()),
            mime_type: Some("text/plain".to_string()),
        })
    }

    fn is_template(&self) -> bool {
        true
    }

    async fn read(&self, uri: &str) -> Result<ResourceContent, ResourceError> {
        let path = uri
            .strip_prefix("file:///")
            .ok_or_else(|| ResourceError::InvalidUri(format!("Invalid file URI: {uri}")))?;

        let safe_path = validate_path(path, &self.base_dir).map_err(ResourceError::InvalidUri)?;

        let content = tokio::fs::read_to_string(&safe_path)
            .await
            .map_err(ResourceError::Io)?;

        Ok(ResourceContent::text(uri.to_string(), content))
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    // Determine the allowed base directory from the first CLI argument,
    // falling back to the current working directory.
    let base_dir = Arc::new(
        std::env::args()
            .nth(1)
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().expect("Failed to get current directory")),
    );

    tracing::info!(base_dir = %base_dir.display(), "Allowed base directory");

    let server = McpServer::builder()
        .name("filesystem")
        .version("1.0.0")
        .instructions("A filesystem server that provides file reading, directory listing, and text search tools.")
        .tool(ReadFileTool {
            base_dir: Arc::clone(&base_dir),
        })
        .tool(ListDirectoryTool {
            base_dir: Arc::clone(&base_dir),
        })
        .tool(SearchFilesTool {
            base_dir: Arc::clone(&base_dir),
        })
        .resource(FileResource {
            base_dir: Arc::clone(&base_dir),
        })
        .build();

    tracing::info!("Filesystem MCP server starting on stdio");
    server.run(StdioTransport::new()).await?;

    Ok(())
}
