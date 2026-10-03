//! Database MCP Server
//!
//! An MCP server that provides a simple in-memory key-value database
//! with tools for CRUD operations and a prompt for SQL-like query help.
//! This demonstrates tools, resources, and prompts working together.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use mcp_sdk::context::ToolContext;
use mcp_sdk::handler::prompts::PromptHandler;
use mcp_sdk::handler::resources::ResourceHandler;
use mcp_sdk::handler::tools::ToolHandler;
use mcp_sdk::protocol::messages::{
    PromptArgument, PromptInfo, ResourceInfo, ToolAnnotations, ToolInfo,
};
use mcp_sdk::schema::JsonSchemaBuilder;
use mcp_sdk::server::McpServer;
use mcp_sdk::transport::stdio::StdioTransport;
use mcp_sdk::types::content::{CallToolResult, PromptMessage, ResourceContent};
use mcp_sdk::types::error::{PromptError, ResourceError, ToolError};
use tokio::sync::RwLock;
use tracing_subscriber::EnvFilter;

/// Shared in-memory database.
type Db = Arc<RwLock<HashMap<String, serde_json::Value>>>;

// ---------------------------------------------------------------------------
// Tool: db_set
// ---------------------------------------------------------------------------

/// Tool that sets a key-value pair in the database.
struct DbSetTool {
    db: Db,
}

#[async_trait]
impl ToolHandler for DbSetTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "db_set".to_string(),
            description: Some("Set a key-value pair in the database".to_string()),
            input_schema: JsonSchemaBuilder::new()
                .property(
                    "key",
                    JsonSchemaBuilder::string().description("The key to set"),
                )
                .property(
                    "value",
                    JsonSchemaBuilder::string().description("The value to store (JSON string)"),
                )
                .required("key")
                .required("value")
                .build(),
            annotations: Some(ToolAnnotations {
                read_only_hint: Some(false),
                // Setting a key can overwrite its previous value.
                destructive_hint: Some(true),
                idempotent_hint: Some(true),
                open_world_hint: Some(false),
                ..Default::default()
            }),
        }
    }

    async fn call(
        &self,
        arguments: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<CallToolResult, ToolError> {
        let key = arguments
            .get("key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing 'key'".to_string()))?
            .to_string();

        let value = arguments
            .get("value")
            .ok_or_else(|| ToolError::InvalidParams("Missing 'value'".to_string()))?
            .clone();

        let mut db = self.db.write().await;
        db.insert(key.clone(), value);

        Ok(CallToolResult::text(format!(
            "Set key '{key}' successfully"
        )))
    }
}

// ---------------------------------------------------------------------------
// Tool: db_get
// ---------------------------------------------------------------------------

/// Tool that gets a value by key from the database.
struct DbGetTool {
    db: Db,
}

#[async_trait]
impl ToolHandler for DbGetTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "db_get".to_string(),
            description: Some("Get a value by key from the database".to_string()),
            input_schema: JsonSchemaBuilder::new()
                .property(
                    "key",
                    JsonSchemaBuilder::string().description("The key to look up"),
                )
                .required("key")
                .build(),
            annotations: Some(ToolAnnotations {
                read_only_hint: Some(true),
                destructive_hint: Some(false),
                idempotent_hint: Some(true),
                open_world_hint: Some(false),
                ..Default::default()
            }),
        }
    }

    async fn call(
        &self,
        arguments: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<CallToolResult, ToolError> {
        let key = arguments
            .get("key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing 'key'".to_string()))?;

        let db = self.db.read().await;
        match db.get(key) {
            Some(value) => {
                let text = serde_json::to_string_pretty(value)
                    .map_err(|e| ToolError::Internal(format!("Serialization error: {e}")))?;
                Ok(CallToolResult::text(text))
            }
            None => Ok(CallToolResult::text(format!("Key '{key}' not found"))),
        }
    }
}

// ---------------------------------------------------------------------------
// Tool: db_delete
// ---------------------------------------------------------------------------

/// Tool that deletes a key from the database.
struct DbDeleteTool {
    db: Db,
}

#[async_trait]
impl ToolHandler for DbDeleteTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "db_delete".to_string(),
            description: Some("Delete a key from the database".to_string()),
            input_schema: JsonSchemaBuilder::new()
                .property(
                    "key",
                    JsonSchemaBuilder::string().description("The key to delete"),
                )
                .required("key")
                .build(),
            annotations: Some(ToolAnnotations {
                read_only_hint: Some(false),
                destructive_hint: Some(true),
                // Repeating a deletion leaves the same key absent.
                idempotent_hint: Some(true),
                open_world_hint: Some(false),
                ..Default::default()
            }),
        }
    }

    async fn call(
        &self,
        arguments: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<CallToolResult, ToolError> {
        let key = arguments
            .get("key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing 'key'".to_string()))?;

        let mut db = self.db.write().await;
        match db.remove(key) {
            Some(_) => Ok(CallToolResult::text(format!("Deleted key '{key}'"))),
            None => Ok(CallToolResult::text(format!("Key '{key}' not found"))),
        }
    }
}

// ---------------------------------------------------------------------------
// Tool: db_list
// ---------------------------------------------------------------------------

/// Tool that lists all keys in the database.
struct DbListTool {
    db: Db,
}

#[async_trait]
impl ToolHandler for DbListTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "db_list".to_string(),
            description: Some("List all keys in the database".to_string()),
            input_schema: JsonSchemaBuilder::new().build(),
            annotations: Some(ToolAnnotations {
                read_only_hint: Some(true),
                destructive_hint: Some(false),
                idempotent_hint: Some(true),
                open_world_hint: Some(false),
                ..Default::default()
            }),
        }
    }

    async fn call(
        &self,
        _arguments: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<CallToolResult, ToolError> {
        let db = self.db.read().await;
        if db.is_empty() {
            Ok(CallToolResult::text("Database is empty"))
        } else {
            let mut keys: Vec<&String> = db.keys().collect();
            keys.sort();
            let listing = keys
                .iter()
                .map(|k| format!("- {k}"))
                .collect::<Vec<_>>()
                .join("\n");
            Ok(CallToolResult::text(format!(
                "Keys ({} total):\n{listing}",
                db.len()
            )))
        }
    }
}

// ---------------------------------------------------------------------------
// Resource: database schema
// ---------------------------------------------------------------------------

/// Resource that returns the current database schema (list of keys and types).
struct DbSchemaResource {
    db: Db,
}

#[async_trait]
impl ResourceHandler for DbSchemaResource {
    fn info(&self) -> ResourceInfo {
        ResourceInfo {
            uri: "db://schema".to_string(),
            name: "database-schema".to_string(),
            description: Some(
                "Current database schema showing all keys and value types".to_string(),
            ),
            mime_type: Some("application/json".to_string()),
        }
    }

    async fn read(&self, uri: &str) -> Result<ResourceContent, ResourceError> {
        let db = self.db.read().await;
        let schema: HashMap<&String, &str> = db
            .iter()
            .map(|(k, v)| {
                let type_name = match v {
                    serde_json::Value::Null => "null",
                    serde_json::Value::Bool(_) => "boolean",
                    serde_json::Value::Number(_) => "number",
                    serde_json::Value::String(_) => "string",
                    serde_json::Value::Array(_) => "array",
                    serde_json::Value::Object(_) => "object",
                };
                (k, type_name)
            })
            .collect();

        let json = serde_json::to_string_pretty(&schema)
            .map_err(|e| ResourceError::Other(format!("Serialization error: {e}")))?;

        Ok(ResourceContent::text_with_mime(
            uri.to_string(),
            json,
            "application/json",
        ))
    }
}

// ---------------------------------------------------------------------------
// Prompt: query helper
// ---------------------------------------------------------------------------

/// Prompt that helps users write database queries.
struct QueryHelperPrompt;

#[async_trait]
impl PromptHandler for QueryHelperPrompt {
    fn info(&self) -> PromptInfo {
        PromptInfo {
            name: "query-helper".to_string(),
            description: Some("Help write database operations".to_string()),
            arguments: Some(vec![
                PromptArgument {
                    name: "operation".to_string(),
                    description: Some(
                        "The operation to perform: get, set, delete, or list".to_string(),
                    ),
                    required: Some(true),
                },
                PromptArgument {
                    name: "key".to_string(),
                    description: Some("The key to operate on (optional for list)".to_string()),
                    required: Some(false),
                },
            ]),
        }
    }

    async fn get(
        &self,
        arguments: HashMap<String, String>,
    ) -> Result<Vec<PromptMessage>, PromptError> {
        let operation = arguments
            .get("operation")
            .ok_or_else(|| PromptError::InvalidArguments("Missing 'operation'".to_string()))?;

        let key = arguments.get("key").cloned().unwrap_or_default();

        let prompt_text = match operation.as_str() {
            "get" => format!(
                "Please use the db_get tool to retrieve the value for key '{key}'. \
                 If the key doesn't exist, let me know."
            ),
            "set" => format!(
                "Please use the db_set tool to store a value for key '{key}'. \
                 Ask me what value I'd like to store if not specified."
            ),
            "delete" => format!(
                "Please use the db_delete tool to remove the key '{key}' from the database. \
                 Confirm the deletion was successful."
            ),
            "list" => "Please use the db_list tool to show all keys currently in the database. \
                       Format the output nicely."
                .to_string(),
            _ => format!(
                "Unknown operation '{operation}'. Supported operations: get, set, delete, list."
            ),
        };

        Ok(vec![PromptMessage::user(prompt_text)])
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

    // Create shared database
    let db: Db = Arc::new(RwLock::new(HashMap::new()));

    // Pre-populate with sample data
    {
        let mut store = db.write().await;
        store.insert(
            "config.app_name".to_string(),
            serde_json::json!("My Application"),
        );
        store.insert("config.version".to_string(), serde_json::json!("2.1.0"));
        store.insert("users.count".to_string(), serde_json::json!(42));
    }

    let server = McpServer::builder()
        .name("database")
        .version("1.0.0")
        .instructions("An in-memory key-value database server. Use db_set, db_get, db_delete, and db_list tools to manage data.")
        .tool(DbSetTool { db: db.clone() })
        .tool(DbGetTool { db: db.clone() })
        .tool(DbDeleteTool { db: db.clone() })
        .tool(DbListTool { db: db.clone() })
        .resource(DbSchemaResource { db: db.clone() })
        .prompt(QueryHelperPrompt)
        .build();

    tracing::info!("Database MCP server starting on stdio");
    server.run(StdioTransport::new()).await?;

    Ok(())
}
