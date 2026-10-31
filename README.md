# Claude Desktop (Rust Edition) ✦

A fast, modern native Windows desktop application inspired by **Claude Desktop** and **Claude Code (CC)** with **Antigravity** storage, written in pure **Rust**.

![Version](https://img.shields.io/badge/version-1.1.0-orange.svg)
![Rust](https://img.shields.io/badge/rust-2021-blue.svg)
![Platform](https://img.shields.io/badge/platform-Windows%20x64-lightgrey.svg)
![MCP](https://img.shields.io/badge/MCP-Model%20Context%20Protocol-green.svg)
![Claude Account](https://img.shields.io/badge/Claude%20Account-No%20API%20Key%20Needed-purple.svg)

---

## 🌟 What's New in v1.1.0

### 1. ✦ Claude Account Mode (No API Keys / No Token Billing)
- Connect using your **Claude web account** (Free / Pro / Team) via your browser session (`sessionKey`).
- **No Developer Console API Keys required**: avoids per-token credit card billing on `console.anthropic.com`.
- **1-Click Verification**: In **⚙ Settings**, paste your `sessionKey` (starts with `sk-ant-sid01-`) and click **"Verify Account"** to immediately link your account and organization.
- **How to find your `sessionKey` in 10 seconds**:
  1. Go to [claude.ai](https://claude.ai) in Chrome/Edge (while logged in).
  2. Press `F12` $\rightarrow$ **Application** tab $\rightarrow$ **Cookies** $\rightarrow$ `https://claude.ai`.
  3. Double-click the value of `sessionKey`, copy it, and paste it into the app!

---

### 2. 📁 Folder Projects (Claude Code in Desktop App)
- Open or create projects mapped to **any folder on your computer** (Roblox game repo, Rust codebase, web project, etc.).
- **Project File Explorer**:
  - Recursively index and browse all project files and directories.
  - Search/filter files in real time.
  - Built-in syntax-styled code editor with direct **"💾 Save"** to disk.
  - **"💬 Add to Chat"** button: instantly injects any project file into your chat prompt with one click.
- **`CLAUDE.md` Project Instructions**:
  - View and edit your project's `CLAUDE.md` directly inside the app.
  - Automatically injected into Claude's prompt context so the AI always understands your architecture, guidelines, and rules.
- **Built-in Claude Code Tools**:
  - `read_file(path)`
  - `write_file(path, content)`
  - `list_files(directory)`
  - `run_command(command)`: integrated terminal runner inside the project workspace!

---

### 3. 🔌 Roblox Studio MCP Integration
- Automatically reads your standard Claude desktop configuration:
  ```json
  {
    "mcpServers": {
      "Roblox_Studio": {
        "command": "cmd.exe",
        "args": [
          "/c",
          "cd /d %LOCALAPPDATA%\\Roblox && .\\mcp.bat"
        ]
      }
    }
  }
  ```
- **34 Roblox Studio tools ready**: `get_studio_state`, `execute_luau`, `inspect_instance`, `script_search`, etc.
- **Direct Testbench**: Test any MCP tool with custom parameters without needing an LLM call.

---

### 4. 🧠 Antigravity Storage
- **Artifacts**: Persistent code scripts, Lua files, diagrams, and markdown documents.
- **Brain Memory**: Long-term notes (`memory_notes.md`) automatically injected into conversations.
- **Scratchpad**: Quick temporary notes and scripts.
- **Split-View Drawer**: Toggle side-by-side artifact and storage viewer while chatting.

---

## 🚀 Quick Start

Launch `ClaudeDesktop.exe`:
```powershell
.\ClaudeDesktop.exe
```
Or build from source:
```powershell
cargo run --release
```
