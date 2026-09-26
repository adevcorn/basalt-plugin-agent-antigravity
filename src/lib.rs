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
            "--print".into(),
            "[Workspace: .] MANDATORY: You must use the Basalt MCP tools (read_file, write_file, request_lease) for all file reads, edits, and leases. Do NOT use view_file or replace_file_content. When using run_command, always set Cwd to '.' so commands run in this workspace directory. User instruction: {prompt}".into(),
        ],
        resume_new_args: vec![
            "--output-format".into(),
            "stream-json".into(),
            "--dangerously-skip-permissions".into(),
            "--model".into(),
            "{model}".into(),
            "--effort".into(),
            "{variant}".into(),
            "--print".into(),
            "[Workspace: .] MANDATORY: You must use the Basalt MCP tools (read_file, write_file, request_lease) for all file reads, edits, and leases. Do NOT use view_file or replace_file_content. When using run_command, always set Cwd to '.' so commands run in this workspace directory. User instruction: {prompt}".into(),
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
            "--print".into(),
            "[Workspace: .] MANDATORY: You must use the Basalt MCP tools (read_file, write_file, request_lease) for all file reads, edits, and leases. Do NOT use view_file or replace_file_content. When using run_command, always set Cwd to '.' so commands run in this workspace directory. User instruction: {prompt}".into(),
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
    let mut extra_args = Vec::new();

    // If workspace_path is provided by the host, pass --gemini_dir with the absolute path to .gemini.
    // This satisfies agy's requirement that gemini_dir must be an absolute path and prevents it from
    // falling back to ~/.gemini/antigravity-cli.
    if let Some(ref ws) = req.workspace_path {
        let gemini_dir = std::path::Path::new(ws).join(".gemini");
        extra_args.push("--gemini_dir".to_string());
        extra_args.push(gemini_dir.to_string_lossy().to_string());
    }

    // Model and effort/variant are handled via {model} and {variant} template placeholders
    // in the args declared by basalt_agent_metadata(). Do not add them here to avoid
    // double-injection (the core's substitute_template_args already substitutes them).

    // Build the .gemini/settings.json combining MCP server config and tool restrictions.
    let mut disabled_builtin_tools: Vec<&str> = Vec::new();
    let mut deny_grants: Vec<String> = Vec::new();
    for tool in &req.disabled_tools {
        match tool {
            StandardTool::Read => {
                disabled_builtin_tools.push("read_file");
                disabled_builtin_tools.push("view_file");
                deny_grants.push(":read_file:*".to_string());
            }
            StandardTool::Write => {
                disabled_builtin_tools.push("write_file");
                disabled_builtin_tools.push("write_to_file");
                disabled_builtin_tools.push("replace_file_content");
                disabled_builtin_tools.push("multi_replace_file_content");
                deny_grants.push(":write_file:*".to_string());
            }
            StandardTool::Execute => {
                disabled_builtin_tools.push("run_shell_command");
                disabled_builtin_tools.push("run_command");
                deny_grants.push(":command:*".to_string());
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
        settings["disabled_tools"] = serde_json::json!(disabled_builtin_tools);
        settings["permissions"] = serde_json::json!({
            "deny": deny_grants
        });
    }

    // Only write the settings file if there is something to configure.
    if settings != serde_json::json!({}) {
        let content = serde_json::to_string_pretty(&settings).unwrap_or_default();

        workspace_files.push(AgentWorkspaceFile {
            relative_path: ".gemini/settings.json".to_string(),
            content: content.clone(),
        });
        workspace_files.push(AgentWorkspaceFile {
            relative_path: ".gemini/config/mcp_config.json".to_string(),
            content: content.clone(),
        });
        workspace_files.push(AgentWorkspaceFile {
            relative_path: ".gemini/antigravity-cli/settings.json".to_string(),
            content: content.clone(),
        });

        // Also write mcp_config.json as a fallback for older agy versions.
        if req.mcp_url.is_some() {
            workspace_files.push(AgentWorkspaceFile {
                relative_path: "mcp_config.json".to_string(),
                content: content.clone(),
            });
        }

        // Write project configuration so ApplyProjectPermissionGrants preserves denials
        if !deny_grants.is_empty() {
            let project_json = serde_json::json!({
                "id": "default-cli-project",
                "name": "CLI Project",
                "permissionGrants": {
                    "deny": deny_grants
                },
                "projectResources": {}
            });
            let project_content = serde_json::to_string_pretty(&project_json).unwrap_or_default();
            workspace_files.push(AgentWorkspaceFile {
                relative_path: ".gemini/config/projects/default-cli-project.json".to_string(),
                content: project_content.clone(),
            });
            workspace_files.push(AgentWorkspaceFile {
                relative_path: ".gemini/antigravity-cli/config/projects/default-cli-project.json".to_string(),
                content: project_content,
            });
        }
    }

    let rule_content = "\
---
trigger: always_on
---
# Workspace Instructions

MANDATORY: You MUST use the Basalt MCP tools (`read_file`, `write_file`, `request_lease`) for all workspace file operations.
Do NOT use built-in tools `view_file`, `replace_file_content`, `write_to_file`, or `multi_replace_file_content`.
When running commands with `run_command`, always explicitly set `Cwd` to `.` (the current workspace directory) rather than omitting it, so commands execute in the workspace instead of the scratch directory.
";

    workspace_files.push(AgentWorkspaceFile {
        relative_path: ".agents/rules/basalt.md".to_string(),
        content: rule_content.to_string(),
    });
    workspace_files.push(AgentWorkspaceFile {
        relative_path: "GEMINI.md".to_string(),
        content: rule_content.to_string(),
    });
    workspace_files.push(AgentWorkspaceFile {
        relative_path: "AGENTS.md".to_string(),
        content: rule_content.to_string(),
    });

    AgentLaunchPreparation {
        extra_args,
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

static OPEN_TOOLS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Stateful parse: processes one JSON line and returns `(new_state, events)`.
///
/// `open_entry` is the current state byte (`STATE_*`).
pub fn parse_antigravity_line_stateful(line_str: &str, open_entry: u8) -> (u8, Vec<AgentEvent>) {
    let mut events = Vec::new();

    if let Ok(val) = serde_json::from_str::<serde_json::Value>(line_str) {
        let event_type = val.get("event").or_else(|| val.get("type")).and_then(|t| t.as_str()).unwrap_or("");

        // 1. Session start / init
        if event_type == "init" || event_type == "session_start" {
            if let Ok(mut set) = OPEN_TOOLS.lock() {
                set.clear();
            }
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

                    let step_idx_str = step.get("step_index")
                        .and_then(|i| i.as_i64().or_else(|| i.as_u64().map(|u| u as i64)))
                        .map(|i| format!("step-{}", i));

                    let call_id = step.get("call_id")
                        .or_else(|| step.get("id"))
                        .and_then(|i| i.as_str())
                        .map(|s| s.to_string())
                        .or(step_idx_str)
                        .unwrap_or_else(|| tool_name.to_string());

                    let tool_info = step.get("tool_info");
                    let params = tool_info
                        .and_then(|ti| ti.get("parameters"))
                        .or_else(|| step.get("parameters"))
                        .or_else(|| step.get("args"));

                    // When tool is call_mcp_tool, unwrap inner ToolName and Arguments
                    let mcp_tool_name = params
                        .and_then(|p| p.get("ToolName").or_else(|| p.get("tool_name")).or_else(|| p.get("tool")))
                        .and_then(|t| t.as_str());

                    let display_tool_name = if let Some(mcp_tool) = mcp_tool_name {
                        mcp_tool
                    } else {
                        tool_name
                    };

                    let actual_command = if let Some(p) = params {
                        let args_obj = p.get("Arguments").or_else(|| p.get("arguments")).unwrap_or(p);
                        args_obj.get("CommandLine")
                            .or_else(|| args_obj.get("command_line"))
                            .or_else(|| args_obj.get("command"))
                            .or_else(|| args_obj.get("cmd"))
                            .or_else(|| args_obj.get("script"))
                            .and_then(|c| c.as_str())
                    } else {
                        None
                    };

                    let entry_tool_name = if (display_tool_name == "run_command"
                        || display_tool_name == "run_shell_command"
                        || display_tool_name == "bash"
                        || display_tool_name == "exec"
                        || display_tool_name == "run")
                        && actual_command.map_or(false, |c| !c.trim().is_empty())
                    {
                        actual_command.unwrap().to_string()
                    } else {
                        display_tool_name.to_string()
                    };

                    let raw_cmd = if let Some(args) = params.and_then(|p| p.get("Arguments").or_else(|| p.get("arguments"))) {
                        args.to_string()
                    } else {
                        params.map(|p| p.to_string()).unwrap_or_default()
                    };

                    let mut file_paths = Vec::new();
                    if let Some(p) = params {
                        let args_obj = p.get("Arguments").or_else(|| p.get("arguments")).unwrap_or(p);
                        if let Some(path) = args_obj.get("filePath")
                            .or_else(|| args_obj.get("path"))
                            .or_else(|| args_obj.get("file_path"))
                            .or_else(|| args_obj.get("file"))
                            .or_else(|| args_obj.get("target_file"))
                            .or_else(|| args_obj.get("TargetFile"))
                            .and_then(|p| p.as_str())
                        {
                            file_paths.push(path.to_string());
                        }
                    }

                    let lower = display_tool_name.to_lowercase();
                    let category = if lower.contains("query_peer") || lower.contains("peer_symbol") || lower.contains("peer_file") {
                        "peer"
                    } else if lower == "task" || lower.contains("subagent") || lower.contains("delegate") {
                        "task"
                    } else if lower.contains("read") || lower.contains("view") {
                        "read"
                    } else if lower.contains("write") || lower.contains("edit") || lower.contains("replace") || lower.contains("lease") {
                        "write"
                    } else if lower.contains("test") {
                        "test"
                    } else if lower.contains("build") || lower.contains("compile") || lower.contains("check") {
                        "build"
                    } else if lower.contains("git") {
                        "git"
                    } else if lower.contains("search") || lower.contains("grep") || lower.contains("find") {
                        "search"
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
                        let is_new = {
                            let mut open = OPEN_TOOLS.lock().unwrap_or_else(|e| e.into_inner());
                            if !open.contains(&call_id) {
                                open.push(call_id.clone());
                                true
                            } else {
                                false
                            }
                        };

                        if is_new {
                            events.push(AgentEvent::NewEntry {
                                vendor_id: call_id,
                                tool: entry_tool_name.clone(),
                                category: category.to_string(),
                                raw_cmd,
                                file_paths,
                            });
                        }
                    } else if state == "DONE" || state == "done" || state == "completed" || state == "success" || output_str.is_some() {
                        let was_open = {
                            let mut open = OPEN_TOOLS.lock().unwrap_or_else(|e| e.into_inner());
                            if let Some(pos) = open.iter().position(|x| x == &call_id) {
                                open.swap_remove(pos);
                                true
                            } else {
                                false
                            }
                        };

                        if !was_open {
                            events.push(AgentEvent::NewEntry {
                                vendor_id: call_id.clone(),
                                tool: entry_tool_name,
                                category: category.to_string(),
                                raw_cmd,
                                file_paths,
                            });
                        }

                        let exit_code = step.get("exit_code")
                            .or_else(|| tool_info.and_then(|ti| ti.get("exit_code")))
                            .and_then(|c| c.as_i64())
                            .unwrap_or(0) as i32;

                        let mut lines = Vec::new();
                        if let Some(out) = output_str {
                            lines = out.lines().map(|l| strip_ansi(l)).collect();
                        }

                        events.push(AgentEvent::CloseEntry {
                            vendor_id: call_id,
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
    fn test_parse_antigravity_tool_categories() {
        let test_json = r#"{"event":"step_update","step_update":{"step_type":"tool","state":"DONE","tool_name":"run_cargo_test","call_id":"c4","tool_info":{"output":"test ok"}}}"#;
        let evs = parse_antigravity_json_line(test_json);
        match &evs[0] {
            AgentEvent::NewEntry { tool, category, .. } => {
                assert_eq!(tool, "run_cargo_test");
                assert_eq!(category, "test");
            }
            _ => panic!("expected NewEntry for run_cargo_test"),
        }

        let git_json = r#"{"event":"step_update","step_update":{"step_type":"tool","state":"DONE","tool_name":"git_status","call_id":"c5","tool_info":{"output":"clean"}}}"#;
        let evs2 = parse_antigravity_json_line(git_json);
        match &evs2[0] {
            AgentEvent::NewEntry { tool, category, .. } => {
                assert_eq!(tool, "git_status");
                assert_eq!(category, "git");
            }
            _ => panic!("expected NewEntry for git_status"),
        }

        let check_json = r#"{"event":"step_update","step_update":{"step_type":"tool","state":"DONE","tool_name":"basalt_check","call_id":"c6","tool_info":{"output":"clean"}}}"#;
        let evs3 = parse_antigravity_json_line(check_json);
        match &evs3[0] {
            AgentEvent::NewEntry { tool, category, .. } => {
                assert_eq!(tool, "basalt_check");
                assert_eq!(category, "build");
            }
            _ => panic!("expected NewEntry for basalt_check"),
        }

        let peer_json = r#"{"event":"step_update","step_update":{"step_type":"tool","state":"DONE","tool_name":"query_peer_symbol","call_id":"c7","tool_info":{"output":"ok"}}}"#;
        let evs4 = parse_antigravity_json_line(peer_json);
        match &evs4[0] {
            AgentEvent::NewEntry { tool, category, .. } => {
                assert_eq!(tool, "query_peer_symbol");
                assert_eq!(category, "peer");
            }
            _ => panic!("expected NewEntry for query_peer_symbol"),
        }
    }

    #[test]
    fn test_parse_antigravity_two_phase_tool_call() {
        if let Ok(mut set) = OPEN_TOOLS.lock() {
            set.clear();
        }
        let active_json = r#"{"event":"step_update","step_update":{"step_index":2,"step_type":"tool","state":"ACTIVE","tool_name":"call_mcp_tool","tool_info":{"parameters":{"ServerName":"basalt","ToolName":"write_file","Arguments":{"path":"README.md","content":"test"}}}}}"#;
        let evs1 = parse_antigravity_json_line(active_json);
        assert_eq!(evs1.len(), 1);
        match &evs1[0] {
            AgentEvent::NewEntry { vendor_id, tool, category, file_paths, .. } => {
                assert_eq!(vendor_id, "step-2");
                assert_eq!(tool, "write_file");
                assert_eq!(category, "write");
                assert_eq!(file_paths, &vec!["README.md".to_string()]);
            }
            _ => panic!("expected NewEntry"),
        }

        let done_json = r#"{"event":"step_update","step_update":{"step_index":2,"step_type":"tool","state":"DONE","tool_name":"call_mcp_tool","tool_info":{"parameters":{"ServerName":"basalt","ToolName":"write_file","Arguments":{"path":"README.md","content":"test"}},"output":"Success"}}}"#;
        let evs2 = parse_antigravity_json_line(done_json);
        assert_eq!(evs2.len(), 1);
        match &evs2[0] {
            AgentEvent::CloseEntry { vendor_id, exit_code, output_lines } => {
                assert_eq!(vendor_id, "step-2");
                assert_eq!(*exit_code, 0);
                assert_eq!(output_lines, &vec!["Success".to_string()]);
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
            workspace_path: None,
        };

        let prep = prepare_antigravity_launch(&req);
        assert!(prep.extra_args.is_empty());
        assert_eq!(prep.workspace_files.len(), 7);
        assert_eq!(prep.workspace_files[0].relative_path, ".gemini/settings.json");
        assert_eq!(prep.workspace_files[1].relative_path, ".gemini/config/mcp_config.json");
        assert_eq!(prep.workspace_files[2].relative_path, ".gemini/antigravity-cli/settings.json");
        assert_eq!(prep.workspace_files[3].relative_path, "mcp_config.json");
        assert_eq!(prep.workspace_files[4].relative_path, ".agents/rules/basalt.md");
        assert_eq!(prep.workspace_files[5].relative_path, "GEMINI.md");
        assert_eq!(prep.workspace_files[6].relative_path, "AGENTS.md");

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
            workspace_path: None,
        };

        let prep2 = prepare_antigravity_launch(&req2);
        assert_eq!(prep2.workspace_files.len(), 9);
        let json2: serde_json::Value = serde_json::from_str(&prep2.workspace_files[0].content).unwrap();
        let disabled = json2["disabledBuiltinTools"].as_array().expect("disabledBuiltinTools should be array");
        assert!(disabled.iter().any(|v| v == "read_file"), "read_file should be disabled");
        assert!(disabled.iter().any(|v| v == "view_file"), "view_file should be disabled");
        assert!(disabled.iter().any(|v| v == "write_file"), "write_file should be disabled");
        assert!(disabled.iter().any(|v| v == "write_to_file"), "write_to_file should be disabled");
        assert!(disabled.iter().any(|v| v == "replace_file_content"), "replace_file_content should be disabled");
        assert!(disabled.iter().any(|v| v == "multi_replace_file_content"), "multi_replace_file_content should be disabled");
        assert_eq!(disabled.len(), 6);

        let deny = json2["permissions"]["deny"].as_array().expect("permissions.deny should be array");
        assert!(deny.iter().any(|v| v == ":read_file:*"));
        assert!(deny.iter().any(|v| v == ":write_file:*"));

        // Check project json
        let proj: serde_json::Value = serde_json::from_str(&prep2.workspace_files[4].content).unwrap();
        assert_eq!(proj["id"], "default-cli-project");
        let proj_deny = proj["permissionGrants"]["deny"].as_array().unwrap();
        assert!(proj_deny.iter().any(|v| v == ":read_file:*"));

        // 3. With workspace_path provided → generates --gemini_dir absolute path
        let req3 = AgentLaunchRequest {
            mcp_url: None,
            disabled_tools: vec![],
            model: None,
            variant: None,
            workspace_path: Some("C:\\repos\\myproject".into()),
        };

        let prep3 = prepare_antigravity_launch(&req3);
        assert_eq!(prep3.extra_args.len(), 2);
        assert_eq!(prep3.extra_args[0], "--gemini_dir");
        #[cfg(target_os = "windows")]
        assert_eq!(prep3.extra_args[1], "C:\\repos\\myproject\\.gemini");
    }

    #[test]
    fn test_parse_antigravity_run_command_actual_cmd() {
        let json = r#"{"event":"step_update","step_update":{"step_type":"tool","state":"DONE","tool_name":"call_mcp_tool","call_id":"c6","tool_info":{"parameters":{"ServerName":"basalt","ToolName":"run_command","Arguments":{"CommandLine":"cargo check --workspace","Cwd":"."}},"output":"Finished"}}}"#;
        let evs = parse_antigravity_json_line(json);
        match &evs[0] {
            AgentEvent::NewEntry { tool, category, .. } => {
                assert_eq!(tool, "cargo check --workspace");
                assert_eq!(category, "run");
            }
            _ => panic!("expected NewEntry for run_command with actual command string"),
        }
    }
}
