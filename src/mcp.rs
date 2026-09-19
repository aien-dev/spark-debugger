
pub fn get_tools_definition() -> Value {
    json!([
        {
            "name": "debug_scan",
            "description": "Perform complete diagnostic scan of services, workspace compilation, and vault security invariants.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        },
        {
            "name": "debug_check_services",
            "description": "Probe status and latencies of SparkOS native services (Cortex, Cockpit, Encoder, LLM Seat, Conduit).",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        },
        {
            "name": "debug_audit_security",
            "description": "Audit zero-disk-secrets invariant and scan for unmasked API key signatures.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }
    ])
}
use reqwest::Client;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::info;

use crate::audit::{audit_services, audit_vault_and_unslop, run_full_audit};

pub async fn run_mcp_server(client: Client, workspace_dir: String) -> Result<(), Box<dyn std::error::Error>> {
    info!("[MCP] Starting spark-debugger MCP server on stdio...");

    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin).lines();

    while let Ok(Some(line)) = reader.next_line().await {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let parsed: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let id = parsed.get("id").cloned().unwrap_or(Value::Null);
        let method = parsed.get("method").and_then(|m| m.as_str()).unwrap_or("");

        match method {
            "initialize" => {
                let resp = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "protocolVersion": "2024-11-05",
                        "capabilities": {
                            "tools": {}
                        },
                        "serverInfo": {
                            "name": "spark-debugger",
                            "version": "0.1.0"
                        }
                    }
                });
                let mut out = resp.to_string();
                out.push_str("\n");
                let _ = stdout.write_all(out.as_bytes()).await;
                let _ = stdout.flush().await;
            }
            "notifications/initialized" => {
                // Client acknowledgment, no response required
            }
            "tools/list" => {
                let resp = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "tools": [
                            {
                                "name": "debug_scan",
                                "description": "Perform complete diagnostic scan of services, workspace compilation, and vault security invariants.",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {}
                                }
                            },
                            {
                                "name": "debug_check_services",
                                "description": "Probe status and latencies of SparkOS native services (Cortex, Cockpit, Encoder, LLM Seat, Conduit).",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {}
                                }
                            },
                            {
                                "name": "debug_audit_invariants",
                                "description": "Audit workspaces for plaintext .env secrets, private key leaks, and unslop voice violations.",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {}
                                }
                            }
                        ]
                    }
                });
                let mut out = resp.to_string();
                out.push_str("\n");
                let _ = stdout.write_all(out.as_bytes()).await;
                let _ = stdout.flush().await;
            }
            "tools/call" => {
                let tool_name = parsed.pointer("/params/name").and_then(|n| n.as_str()).unwrap_or("");
                let result_text = match tool_name {
                    "debug_scan" => {
                        let report = run_full_audit(&client, &workspace_dir).await;
                        serde_json::to_string_pretty(&report).unwrap_or_else(|_| "Failed to serialize report".to_string())
                    }
                    "debug_check_services" => {
                        let services = audit_services(&client).await;
                        serde_json::to_string_pretty(&services).unwrap_or_else(|_| "Failed to serialize services".to_string())
                    }
                    "debug_audit_invariants" => {
                        let home = std::env::var("HOME")
                            .or_else(|_| std::env::var("USERPROFILE"))
                            .unwrap_or_else(|_| ".".to_string());
                        let cockpit_dir = format!("{}/spark-cockpit-rs", home);
                        let inv = audit_vault_and_unslop(&[&workspace_dir, &cockpit_dir]);
                        serde_json::to_string_pretty(&inv).unwrap_or_else(|_| "Failed to serialize invariants".to_string())
                    }
                    _ => format!("Unknown tool: {}", tool_name),
                };

                let resp = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "content": [
                            {
                                "type": "text",
                                "text": result_text
                            }
                        ]
                    }
                });
                let mut out = resp.to_string();
                out.push_str("\n");
                let _ = stdout.write_all(out.as_bytes()).await;
                let _ = stdout.flush().await;
            }
            _ => {
                let resp = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32601,
                        "message": "Method not found"
                    }
                });
                let mut out = resp.to_string();
                out.push_str("\n");
                let _ = stdout.write_all(out.as_bytes()).await;
                let _ = stdout.flush().await;
            }
        }
    }

    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_tools_definition() {
        let tools = get_tools_definition();
        let list = tools.as_array().expect("array of tools");
        assert_eq!(list.len(), 3);
        let names: Vec<&str> = list.iter().filter_map(|t| t.get("name").and_then(Value::as_str)).collect();
        assert!(names.contains(&"debug_scan"));
        assert!(names.contains(&"debug_check_services"));
        assert!(names.contains(&"debug_audit_security"));
    }
}
