//! Google Antigravity CLI Agent plugin for Basalt.
//!
//! Provides `CAP_AGENT_LAUNCHER` for running Google Antigravity CLI (`agy`) agent sessions
//! with dynamic model discovery (`agy models`), effort variants (`low`, `medium`, `high`),
//! automatic MCP server wiring via `.gemini/settings.json`, and structured stream-json event parsing.

use basalt_plugin_sdk::prelude::*;
use basalt_host_shims as _;

pub const PLUGIN_NAME: &str = "antigravity";
pub const PLUGIN_VERSION: &str = "0.1.0";

basalt_plugin_meta! {
    name:         "antigravity",
    version:      "0.1.0",
    hook_flags:   CAP_AGENT_LAUNCHER,
    provides:     "agent-launcher@antigravity/v1",
    requires:     "",
    optional_requires: "",
    file_globs:   "",
    activates_on: "",
    activation_events: "",
}

#[no_mangle]
pub extern "C" fn basalt_agent_metadata() -> u64 {
    let meta = AgentMetadata {
        name: "Google Antigravity CLI".into(),
        executable: "agy".into(),
        args: vec![
            "--output-format".into(),
            "stream-json".into(),
            "--dangerously-skip-permissions".into(),
            "--model".into(),
            "{model}".into(),
            "--effort".into(),
            "{variant}".into(),
            "-p={prompt}".into(),
        ],
        resume_new_args: vec![
            "--output-format".into(),
            "stream-json".into(),
            "--dangerously-skip-permissions".into(),
            "--model".into(),
            "{model}".into(),
            "--effort".into(),
            "{variant}".into(),
            "-p={prompt}".into(),
        ],
        resume_cont_args: vec![
            "--continue".into(),
            "--output-format".into(),
            "stream-json".into(),
            "--dangerously-skip-permissions".into(),
            "--model".into(),
            "{model}".into(),
            "--effort".into(),
            "{variant}".into(),
            "-p={prompt}".into(),
        ],
        execution_tier: AgentExecutionTier::MountedWorkspace,
        workspace_capabilities: vec!["mcp".into(), "shadow".into()],
        protocol: AgentProtocol::Cli,
    };
    let bytes = encode_agent_metadata(&meta);
    pack_output(bytes)
}

#[no_mangle]
pub extern "C" fn basalt_agent_settings_schema() -> u64 {
    let schema = serde_json::json!({
        "plugin": "antigravity",
        "dynamic_models": true,
        "models_command": "agy models",
        "variants": [
            "default",
            "low",
            "medium",
            "high"
        ],
        "default_variant": "default"
    });
    let bytes = serde_json::to_vec(&schema).unwrap_or_default();
    pack_output(bytes)
}

/// Pure implementation of Google Antigravity launch preparation for testability and guest execution.
pub fn prepare_antigravity_launch(req: &AgentLaunchRequest) -> AgentLaunchPreparation {
    let mut workspace_files = Vec::new();

    // Model and effort/variant are handled via {model} and {variant} template placeholders
    // in the args declared by basalt_agent_metadata(). Do not add them here to avoid
    // double-injection (the core's substitute_template_args already substitutes them).

    // Build the .gemini/settings.json combining MCP server config and tool restrictions.
    //
    // Only the primary file I/O tools are disabled so that agy can still use
    // directory listing and search for workspace/context detection, while being
    // forced to use Basalt's MCP-provided tools for actual file reads and writes.
    let mut disabled_builtin_tools: Vec<&str> = Vec::new();
    for tool in &req.disabled_tools {
        match tool {
            StandardTool::Read => {
                // Re-enabled for agy so it can read local files and detect workspace context natively.
            }
            StandardTool::Write => {
                disabled_builtin_tools.push("write_file");
            }
            StandardTool::Execute => {
                disabled_builtin_tools.push("run_shell_command");
            }
            StandardTool::Question => {
                // agy has no built-in "question" tool to disable.
            }
        }
    }

    let mut settings = serde_json::json!({});

    // MCP server configuration.
    // Antigravity supports project-local config in `.gemini/settings.json`.
    if let Some(ref url) = req.mcp_url {
        settings["mcpServers"] = serde_json::json!({
            "basalt": {
                "serverUrl": url,
                "disabled": false
            }
        });
    }

    // Disable built-in tools so the agent uses Basalt's MCP-provided tools instead.
    if !disabled_builtin_tools.is_empty() {
        settings["disabledBuiltinTools"] = serde_json::json!(disabled_builtin_tools);
    }

    // Only write the settings file if there is something to configure.
    if settings != serde_json::json!({}) {
        let content = serde_json::to_string_pretty(&settings).unwrap_or_default();

        workspace_files.push(AgentWorkspaceFile {
            relative_path: ".gemini/settings.json".to_string(),
            content: content.clone(),
        });

        // Also write mcp_config.json as a fallback for older agy versions.
        if req.mcp_url.is_some() {
            workspace_files.push(AgentWorkspaceFile {
                relative_path: "mcp_config.json".to_string(),
                content,
            });
        }
    }

    AgentLaunchPreparation {
        extra_args: Vec::new(),
        env: std::collections::HashMap::new(),
        workspace_files,
    }
}

#[no_mangle]
pub extern "C" fn basalt_agent_prepare_launch(
    req_ptr: *const u8,
    req_len: u32,
) -> u64 {
    let req: AgentLaunchRequest = if !req_ptr.is_null() && req_len > 0 {
        let slice = unsafe { std::slice::from_raw_parts(req_ptr, req_len as usize) };
        serde_json::from_slice(slice).unwrap_or_default()
    } else {
        AgentLaunchRequest::default()
    };

    let prep = prepare_antigravity_launch(&req);
    let bytes = serde_json::to_vec(&prep).unwrap_or_default();
    pack_output(bytes)
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if let Some(&next) = chars.peek() {
                if next == '[' {
                    chars.next(); // consume '['
                    while let Some(&ch) = chars.peek() {
                        chars.next();
                        if ('\x40'..='\x7e').contains(&ch) {
                            break;
                        }
                    }
                } else if next == ']' {
                    chars.next(); // consume ']'
                    while let Some(&ch) = chars.peek() {
                        chars.next();
                        if ch == '\x07' {
                            break;
                        }
                        if ch == '\x1b' {
                            if let Some(&'\\') = chars.peek() {
                                chars.next();
                            }
                            break;
                        }
                    }
                } else if next == '(' || next == ')' {
                    chars.next();
                    chars.next();
                } else {
                    chars.next();
                }
            }
        } else if !c.is_control() || c == '\n' || c == '\t' {
            out.push(c);
        }
    }
    out
}

/// State byte flags threaded through `basalt_agent_parse_line`.
///
/// The host passes state back on every call, so we can track whether an agent-message
/// or thought entry is already open and use `AppendToEntry` instead of `NewEntry` for
/// subsequent streaming deltas.
const STATE_NONE: u8 = 0;
const STATE_MSG_OPEN: u8 = 1;
const STATE_THOUGHT_OPEN: u8 = 2;

/// Stateful parse: processes one JSON line and returns `(new_state, events)`.
///
/// `open_entry` is the current state byte (`STATE_*`).
pub fn parse_antigravity_line_stateful(line_str: &str, open_entry: u8) -> (u8, Vec<AgentEvent>) {
    let mut events = Vec::new();

    if let Ok(val) = serde_json::from_str::<serde_json::Value>(line_str) {
        let event_type = val.get("event").or_else(|| val.get("type")).and_then(|t| t.as_str()).unwrap_or("");

        // 1. Session start / init
        if event_type == "init" || event_type == "session_start" {
            if let Some(cid) = val.get("conversation_id")
                .or_else(|| val.get("session_id"))
                .or_else(|| val.get("sessionId"))
                .and_then(|s| s.as_str())
            {
                events.push(AgentEvent::SessionIDAvailable(cid.to_string()));
            }
            return (STATE_NONE, events);
        }

        // 2. Step Update
        if event_type == "step_update" {
            if let Some(step) = val.get("step_update") {
                let step_type = step.get("step_type").and_then(|s| s.as_str()).unwrap_or("");
                let state = step.get("state").and_then(|s| s.as_str()).unwrap_or("");

                if step_type == "tool" || step.get("tool_name").is_some() {
                    // A tool event closes any open message/thought entry.
                    let tool_name = step.get("tool_name")
                        .or_else(|| step.get("name"))
                        .and_then(|t| t.as_str())
                        .unwrap_or("tool");

                    let call_id = step.get("call_id")
                        .or_else(|| step.get("id"))
                        .and_then(|i| i.as_str())
                        .unwrap_or(tool_name);

                    let tool_info = step.get("tool_info");
                    let params = tool_info
                        .and_then(|ti| ti.get("parameters"))
                        .or_else(|| step.get("parameters"))
                        .or_else(|| step.get("args"));

                    let raw_cmd = params.map(|p| p.to_string()).unwrap_or_default();

                    let mut file_paths = Vec::new();
                    if let Some(p) = params {
                        if let Some(path) = p.get("filePath")
                            .or_else(|| p.get("path"))
                            .or_else(|| p.get("file_path"))
                            .or_else(|| p.get("file"))
                            .or_else(|| p.get("target_file"))
                            .or_else(|| p.get("TargetFile"))
                            .and_then(|p| p.as_str())
                        {
                            file_paths.push(path.to_string());
                        }
                    }

                    let lower = tool_name.to_lowercase();
                    let category = if lower.contains("read") || lower.contains("view") {
                        "read"
                    } else if lower.contains("write") || lower.contains("edit") || lower.contains("replace") {
                        "write"
                    } else if lower.contains("run") || lower.contains("bash") || lower.contains("exec") || lower.contains("command") {
                        "run"
                    } else if lower.contains("ask") || lower.contains("question") {
                        "question"
                    } else {
                        "run"
                    };

                    let output_val = tool_info
                        .and_then(|ti| ti.get("output"))
                        .or_else(|| step.get("output"))
                        .or_else(|| step.get("result"));

                    let output_str = output_val.and_then(|o| o.as_str());

                    if state == "ACTIVE" || state == "active" || state == "running" {
                        events.push(AgentEvent::NewEntry {
                            vendor_id: call_id.to_string(),
                            tool: tool_name.to_string(),
                            category: category.to_string(),
                            raw_cmd,
                            file_paths,
                        });
                    } else if state == "DONE" || state == "done" || state == "completed" || state == "success" || output_str.is_some() {
                        events.push(AgentEvent::NewEntry {
                            vendor_id: call_id.to_string(),
                            tool: tool_name.to_string(),
                            category: category.to_string(),
                            raw_cmd,
                            file_paths,
                        });

                        let exit_code = step.get("exit_code")
                            .or_else(|| tool_info.and_then(|ti| ti.get("exit_code")))
                            .and_then(|c| c.as_i64())
                            .unwrap_or(0) as i32;

                        let mut lines = Vec::new();
                        if let Some(out) = output_str {
                            lines = out.lines().map(|l| strip_ansi(l)).collect();
                        }

                        events.push(AgentEvent::CloseEntry {
                            vendor_id: call_id.to_string(),
                            exit_code,
                            output_lines: lines,
                        });
                    }
                    // Tool events reset any open streaming entry.
                    return (STATE_NONE, events);

                } else if step_type == "agent_response" || step_type == "message" || step_type == "text" {
                    let text = step.get("text_delta")
                        .or_else(|| step.get("text"))
                        .or_else(|| step.get("content"))
                        .or_else(|| step.get("message"))
                        .and_then(|t| t.as_str())
                        .unwrap_or("");
                    let cleaned = strip_ansi(text);
                    if !cleaned.trim().is_empty() {
                        if open_entry == STATE_MSG_OPEN {
                            // Already have an open message card — append to it.
                            events.push(AgentEvent::AppendToEntry {
                                vendor_id: "agent-response".to_string(),
                                text: cleaned,
                            });
                        } else {
                            // First delta of a new agent turn — open a new card.
                            events.push(AgentEvent::NewEntry {
                                vendor_id: "agent-response".to_string(),
                                tool: cleaned.chars().take(80).collect(),
                                category: "message".into(),
                                raw_cmd: cleaned,
                                file_paths: Vec::new(),
                            });
                        }
                        return (STATE_MSG_OPEN, events);
                    }
                    return (open_entry, events);

                } else if step_type == "thought" || step_type == "reasoning" || step_type == "thinking" {
                    let text = step.get("text_delta")
                        .or_else(|| step.get("text"))
                        .or_else(|| step.get("thought"))
                        .or_else(|| step.get("reasoning"))
                        .and_then(|t| t.as_str())
                        .unwrap_or("");
                    let cleaned = strip_ansi(text);
                    if !cleaned.trim().is_empty() {
                        if open_entry == STATE_THOUGHT_OPEN {
                            events.push(AgentEvent::AppendToEntry {
                                vendor_id: "agent-thought".to_string(),
                                text: cleaned,
                            });
                        } else {
                            events.push(AgentEvent::NewEntry {
                                vendor_id: "agent-thought".to_string(),
                                tool: cleaned.chars().take(80).collect(),
                                category: "thought".into(),
                                raw_cmd: cleaned,
                                file_paths: Vec::new(),
                            });
                        }
                        return (STATE_THOUGHT_OPEN, events);
                    }
                    return (open_entry, events);
                }
            }
        }

        // 3. Result / Session Ended — also closes any open entry.
        if event_type == "result" || event_type == "done" || event_type == "complete" || event_type == "finish" {
            let res_obj = val.get("result");
            let status = res_obj
                .and_then(|r| r.get("status"))
                .or_else(|| val.get("status"))
                .and_then(|s| s.as_str())
                .unwrap_or("SUCCESS");

            let is_success = status.eq_ignore_ascii_case("SUCCESS") || status.eq_ignore_ascii_case("OK");
            let error = if !is_success {
                res_obj
                    .and_then(|r| r.get("error").or_else(|| r.get("error_message")).or_else(|| r.get("message")))
                    .or_else(|| val.get("error").or_else(|| val.get("error_message")).or_else(|| val.get("message")))
                    .and_then(|e| if e.is_string() { e.as_str().map(ToString::to_string) } else { Some(e.to_string()) })
                    .or_else(|| Some(format!("session ended with status: {}", status)))
            } else {
                None
            };
            events.push(AgentEvent::SessionEnded { success: is_success, error });
            return (STATE_NONE, events);
        }

        // 4. Fallback for generic text/message in root object
        if let Some(text) = val.get("text").or_else(|| val.get("message")).or_else(|| val.get("content")).and_then(|t| t.as_str()) {
            let cleaned = strip_ansi(text);
            if !cleaned.trim().is_empty() {
                if open_entry == STATE_MSG_OPEN {
                    events.push(AgentEvent::AppendToEntry {
                        vendor_id: "agent-response".to_string(),
                        text: cleaned,
                    });
                    return (STATE_MSG_OPEN, events);
                }
                events.push(AgentEvent::NewEntry {
                    vendor_id: format!("msg-{}", cleaned.len()),
                    tool: cleaned.chars().take(80).collect(),
                    category: "message".into(),
                    raw_cmd: cleaned,
                    file_paths: Vec::new(),
                });
            }
        }
    } else {
        // Plain text fallback
        let cleaned = strip_ansi(line_str);
        if !cleaned.trim().is_empty() {
            let category = if cleaned.to_lowercase().contains("permission requested") {
                "question"
            } else if cleaned.to_lowercase().contains("error") {
                "diagnostic"
            } else {
                "log"
            };
            events.push(AgentEvent::NewEntry {
                vendor_id: format!("raw-{}", cleaned.len()),
                tool: cleaned.chars().take(80).collect(),
                category: category.into(),
                raw_cmd: cleaned,
                file_paths: Vec::new(),
            });
        }
    }

    (STATE_NONE, events)
}

/// Legacy stateless wrapper kept for unit tests that don't exercise streaming.
pub fn parse_antigravity_json_line(line_str: &str) -> Vec<AgentEvent> {
    parse_antigravity_line_stateful(line_str, STATE_NONE).1
}

#[no_mangle]
pub extern "C" fn basalt_agent_parse_line(
    line_ptr: *const u8,
    line_len: u32,
    state_ptr: *const u8,
    state_len: u32,
) -> u64 {
    if line_ptr.is_null() || line_len == 0 {
        return pack_output(encode_agent_parse_output(&[], &[]));
    }
    let line_slice = unsafe { std::slice::from_raw_parts(line_ptr, line_len as usize) };
    let line_str = match std::str::from_utf8(line_slice) {
        Ok(s) => s.trim(),
        Err(_) => return pack_output(encode_agent_parse_output(&[], &[])),
    };

    if line_str.is_empty() {
        return pack_output(encode_agent_parse_output(&[], &[]));
    }

    // Read the open-entry state byte from the host.
    let open_entry = if !state_ptr.is_null() && state_len > 0 {
        let state_slice = unsafe { std::slice::from_raw_parts(state_ptr, state_len as usize) };
        state_slice[0]
    } else {
        STATE_NONE
    };

    let (new_state, events) = parse_antigravity_line_stateful(line_str, open_entry);
    pack_output(encode_agent_parse_output(&[new_state], &events))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_antigravity_init_and_result() {
        // 1. init event with conversation_id
        let init_json = r#"{"event":"init","conversation_id":"256e8ed4-21d6-4d3e-b21d-15a9d15773d7"}"#;
        let evs = parse_antigravity_json_line(init_json);
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            AgentEvent::SessionIDAvailable(id) => assert_eq!(id, "256e8ed4-21d6-4d3e-b21d-15a9d15773d7"),
            _ => panic!("expected SessionIDAvailable"),
        }

        // 2. result event
        let result_json = r#"{"event":"result","result":{"status":"SUCCESS"}}"#;
        let evs = parse_antigravity_json_line(result_json);
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            AgentEvent::SessionEnded { success, .. } => assert!(*success),
            _ => panic!("expected SessionEnded"),
        }
    }

    #[test]
    fn test_parse_antigravity_tool_call() {
        let tool_json = r#"{"event":"step_update","step_update":{"step_type":"tool","state":"DONE","tool_name":"basalt_read_file","call_id":"c1","tool_info":{"parameters":{"path":"Cargo.toml"},"output":"[workspace]\nmembers = []"}}}"#;
        let evs = parse_antigravity_json_line(tool_json);
        assert_eq!(evs.len(), 2);
        match &evs[0] {
            AgentEvent::NewEntry { tool, category, file_paths, vendor_id, .. } => {
                assert_eq!(vendor_id, "c1");
                assert_eq!(tool, "basalt_read_file");
                assert_eq!(category, "read");
                assert_eq!(file_paths, &vec!["Cargo.toml".to_string()]);
            }
            _ => panic!("expected NewEntry"),
        }
        match &evs[1] {
            AgentEvent::CloseEntry { vendor_id, exit_code, output_lines } => {
                assert_eq!(vendor_id, "c1");
                assert_eq!(*exit_code, 0);
                assert_eq!(output_lines.len(), 2);
                assert_eq!(output_lines[0], "[workspace]");
            }
            _ => panic!("expected CloseEntry"),
        }
    }

    #[test]
    fn test_parse_antigravity_agent_response() {
        let msg_json = r#"{"event":"step_update","step_update":{"step_type":"agent_response","text_delta":"Hello from Antigravity!"}}"#;
        let evs = parse_antigravity_json_line(msg_json);
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            AgentEvent::NewEntry { category, raw_cmd, .. } => {
                assert_eq!(category, "message");
                assert_eq!(raw_cmd, "Hello from Antigravity!");
            }
            _ => panic!("expected NewEntry with message"),
        }
    }

    #[test]
    fn test_prepare_antigravity_launch() {
        // 1. MCP only, no disabled tools
        let req = AgentLaunchRequest {
            mcp_url: Some("http://127.0.0.1:9090".into()),
            disabled_tools: vec![],
            model: Some("gemini-3.8-flash-high".into()),
            variant: Some("high".into()),
        };

        let prep = prepare_antigravity_launch(&req);
        // Model and effort are now handled via {model}/{variant} template placeholders,
        // not via extra_args, so extra_args should be empty.
        assert!(prep.extra_args.is_empty());
        assert_eq!(prep.workspace_files.len(), 2);
        assert_eq!(prep.workspace_files[0].relative_path, ".gemini/settings.json");
        assert_eq!(prep.workspace_files[1].relative_path, "mcp_config.json");

        let json_val: serde_json::Value = serde_json::from_str(&prep.workspace_files[0].content).unwrap();
        assert_eq!(json_val["mcpServers"]["basalt"]["serverUrl"], "http://127.0.0.1:9090");
        // No disabled tools → key should be absent
        assert!(json_val.get("disabledBuiltinTools").is_none());

        // 2. With Read + Write disabled → forces agent to use MCP tools
        let req2 = AgentLaunchRequest {
            mcp_url: Some("http://127.0.0.1:9090".into()),
            disabled_tools: vec![StandardTool::Read, StandardTool::Write],
            model: None,
            variant: None,
        };

        let prep2 = prepare_antigravity_launch(&req2);
        let json2: serde_json::Value = serde_json::from_str(&prep2.workspace_files[0].content).unwrap();
        let disabled = json2["disabledBuiltinTools"].as_array().expect("disabledBuiltinTools should be array");
        assert!(!disabled.iter().any(|v| v == "read_file"), "read_file should be enabled");
        assert!(disabled.iter().any(|v| v == "write_file"), "write_file should be disabled");
        assert_eq!(disabled.len(), 1);
    }
}
