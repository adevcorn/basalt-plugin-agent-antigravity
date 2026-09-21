# basalt-plugin-antigravity

Google Antigravity CLI (`agy`) launcher plugin for [Basalt](https://github.com/adevcorn/basalt).

Provides `CAP_AGENT_LAUNCHER` capability to manage, launch, and stream Antigravity agent sessions within Basalt workspaces.

---

## 🚀 Capabilities & Metadata

- **Plugin Name**: `antigravity`
- **Hook Flags**: `CAP_AGENT_LAUNCHER`
- **Provides**: `agent-launcher@antigravity/v1`
- **Execution Tier**: `AgentExecutionTier::MountedWorkspace`
- **Protocol**: `AgentProtocol::Cli`
- **Workspace Capabilities**: `mcp`, `shadow`

---

## ✨ Features

- **CLI Session Orchestration**: Launches `agy` with `--output-format stream-json` and `--dangerously-skip-permissions` for headless automated workflows.
- **Dynamic Model Discovery**: Queries available Gemini and auxiliary models dynamically via `agy models`.
- **Reasoning Effort Variants**: Supports effort configuration flags (`default`, `low`, `medium`, `high`) mapped to `--effort {variant}`.
- **Automated MCP Server Injection**: Automatically configures and writes project-level `.gemini/settings.json` (and legacy `mcp_config.json`) with Basalt's session MCP server endpoint.
- **Selective Tool Restrictions**: Allows native file read and directory inspection while redirecting write and execute operations through Basalt's coordinated MCP tools.
- **Stateful Event Parsing**:
  - Session startup & conversation identification (`init`, `session_start`).
  - Active & completed tool execution lifecycles (`step_update`).
  - Real-time streaming response deltas (`agent_response`, `message`).
  - Agent reasoning & thought streaming (`thought`, `reasoning`).
  - Result status & exit code handling with ANSI terminal escape stripping.

---

## 🛠️ Configuration & Settings Schema

The plugin exports `basalt_agent_settings_schema`:

```json
{
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
}
```

### Generated `.gemini/settings.json`

During session preparation (`basalt_agent_prepare_launch`), the plugin generates the workspace configuration:

```json
{
  "mcpServers": {
    "basalt": {
      "serverUrl": "http://127.0.0.1:<PORT>",
      "disabled": false
    }
  },
  "disabledBuiltinTools": [
    "write_file",
    "run_shell_command"
  ]
}
```

---

## 🏗️ Building & Testing

### Prerequisites

- Rust 1.75+ (2021 edition)
- Local `basalt-plugin-sdk` and `basalt` crates in neighboring directories

### Build

```bash
# Build native rlib / cdylib
cargo build --release

# Check compilation
cargo check
```

### Run Tests

The test suite covers JSON streaming parser event sequences, tool result handling, and launch preparation:

```bash
cargo test
```

---

## 📄 License

Licensed under the same terms as Basalt.
