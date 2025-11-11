use anyhow::Result;
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use tracing::{debug, error, info};

use super::protocol::{JsonRpcRequest, JsonRpcResponse};
use super::tools::ToolRegistry;

pub struct Server {
    tools: ToolRegistry,
}

impl Server {
    pub fn new() -> Self {
        Self {
            tools: ToolRegistry::new(),
        }
    }

    pub async fn run(self) -> Result<()> {
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();
        let reader = stdin.lock();

        info!("MCP server ready, waiting for requests");

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }

            debug!("Received: {}", line);

            let response = match serde_json::from_str::<JsonRpcRequest>(&line) {
                Ok(request) => self.handle_request(request).await,
                Err(e) => JsonRpcResponse::error(
                    None,
                    -32700,
                    format!("Parse error: {}", e),
                ),
            };

            let response_json = serde_json::to_string(&response)?;
            debug!("Sending: {}", response_json);
            writeln!(stdout, "{}", response_json)?;
            stdout.flush()?;
        }

        Ok(())
    }

    async fn handle_request(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        debug!("Handling method: {}", request.method);

        match request.method.as_str() {
            "initialize" => self.handle_initialize(request),
            "tools/list" => self.handle_tools_list(request),
            "tools/call" => self.handle_tools_call(request).await,
            _ => JsonRpcResponse::method_not_found(request.id),
        }
    }

    fn handle_initialize(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        info!("Client initializing");

        let result = json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "rs-hack-mcp",
                "version": env!("CARGO_PKG_VERSION")
            },
            "instructions": "rs-hack provides AST-aware refactoring tools for Rust code.\n\n\
                Key principles:\n\
                - All operations are DRY-RUN by default - use apply=true to make actual changes\n\
                - Operations are idempotent - safe to run multiple times\n\
                - Use inspect tools first to see what will be affected\n\
                - All operations support glob patterns for multi-file edits\n\
                - Changes are tracked with unique run IDs for easy revert\n\n\
                Workflow:\n\
                1. Use inspect_* tools to explore code\n\
                2. Preview changes (dry-run)\n\
                3. Apply changes with apply=true\n\
                4. Use history/revert if needed"
        });

        JsonRpcResponse::success(request.id, result)
    }

    fn handle_tools_list(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        debug!("Listing tools");

        let tools_list: Vec<Value> = self.tools.list().iter().map(|tool| {
            json!({
                "name": tool.name,
                "description": tool.description,
                "inputSchema": tool.input_schema
            })
        }).collect();

        JsonRpcResponse::success(request.id, json!({ "tools": tools_list }))
    }

    async fn handle_tools_call(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        let params = match request.params {
            Some(p) => p,
            None => return JsonRpcResponse::invalid_params(request.id.clone(), "Missing params".to_string()),
        };

        let tool_name = match params.get("name").and_then(|v| v.as_str()) {
            Some(name) => name,
            None => return JsonRpcResponse::invalid_params(request.id.clone(), "Missing tool name".to_string()),
        };

        let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

        debug!("Calling tool: {} with args: {:?}", tool_name, arguments);

        match self.tools.call(tool_name, arguments).await {
            Ok(result) => {
                info!("Tool {} completed successfully", tool_name);
                JsonRpcResponse::success(request.id, json!({ "content": [{ "type": "text", "text": result }] }))
            }
            Err(e) => {
                error!("Tool {} failed: {}", tool_name, e);
                JsonRpcResponse::internal_error(request.id, e.to_string())
            }
        }
    }
}
