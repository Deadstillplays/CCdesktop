use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomTool {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema", default)]
    pub input_schema: Value,
    #[serde(default = "default_interpreter")]
    pub interpreter: String, // "powershell", "cmd", "python", "node"
    pub script_code: String,
    #[serde(default)]
    pub created_at: String,
}

fn default_interpreter() -> String {
    "powershell".to_string()
}

#[derive(Clone, Debug)]
pub struct DynamicToolManager {
    dir: PathBuf,
}

impl DynamicToolManager {
    pub fn new(base_dir: &str) -> Self {
        let dir = Path::new(base_dir).join("custom_tools");
        let _ = fs::create_dir_all(&dir);
        Self { dir }
    }

    pub fn list_tools(&self) -> Vec<CustomTool> {
        let mut tools = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("json") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(tool) = serde_json::from_str::<CustomTool>(&content) {
                            tools.push(tool);
                        }
                    }
                }
            }
        }
        tools.sort_by(|a, b| a.name.cmp(&b.name));
        tools
    }

    pub fn get_tool(&self, name: &str) -> Option<CustomTool> {
        let file_path = self.dir.join(format!("{}.json", name.to_lowercase()));
        if file_path.exists() {
            if let Ok(content) = fs::read_to_string(&file_path) {
                return serde_json::from_str::<CustomTool>(&content).ok();
            }
        }
        self.list_tools().into_iter().find(|t| t.name.eq_ignore_ascii_case(name))
    }

    pub fn save_tool(&self, mut tool: CustomTool) -> Result<String, String> {
        let clean_name = tool.name.trim().to_lowercase().replace(' ', "_");
        if clean_name.is_empty() {
            return Err("Tool name cannot be empty".to_string());
        }
        tool.name = clean_name.clone();
        if tool.created_at.is_empty() {
            tool.created_at = Utc::now().to_rfc3339();
        }

        let file_path = self.dir.join(format!("{}.json", clean_name));
        let json_str = serde_json::to_string_pretty(&tool)
            .map_err(|e| format!("Serialization error: {}", e))?;

        fs::write(&file_path, json_str)
            .map_err(|e| format!("Failed to write tool file: {}", e))?;

        Ok(format!(
            "Successfully registered custom tool '{}' ({}) with dynamic execution capability.",
            clean_name, tool.interpreter
        ))
    }

    pub fn delete_tool(&self, name: &str) -> Result<String, String> {
        let clean_name = name.trim().to_lowercase().replace(' ', "_");
        let file_path = self.dir.join(format!("{}.json", clean_name));
        if file_path.exists() {
            fs::remove_file(&file_path)
                .map_err(|e| format!("Failed to delete tool file: {}", e))?;
            Ok(format!("Custom tool '{}' was removed.", clean_name))
        } else {
            Err(format!("Custom tool '{}' not found.", name))
        }
    }

    pub fn execute_tool(&self, name: &str, args: &Value) -> Result<String, String> {
        let tool = self
            .get_tool(name)
            .ok_or_else(|| format!("Custom tool '{}' not found.", name))?;

        let mut script = tool.script_code.clone();

        // Inject arguments as template replacements {{key}}
        if let Some(obj) = args.as_object() {
            for (k, v) in obj {
                let val_str = match v {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    other => other.to_string(),
                };
                script = script.replace(&format!("{{{{{}}}}}", k), &val_str);
            }
        }

        let interp = tool.interpreter.to_lowercase().trim().to_string();
        let mut cmd = match interp.as_str() {
            "python" | "py" => {
                let mut c = Command::new("python");
                c.arg("-c").arg(&script);
                c
            }
            "node" | "javascript" | "js" => {
                let mut c = Command::new("node");
                c.arg("-e").arg(&script);
                c
            }
            "cmd" | "bat" | "batch" => {
                let mut c = Command::new("cmd");
                c.args(["/c", &script]);
                c
            }
            _ => {
                // Default to PowerShell
                let mut c = Command::new("powershell");
                c.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &script]);
                c
            }
        };

        // Pass arguments as environment variables TOOL_ARG_<KEY>=<VAL>
        if let Some(obj) = args.as_object() {
            for (k, v) in obj {
                let var_name = format!("TOOL_ARG_{}", k.to_uppercase().replace('-', "_"));
                let val_str = match v {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    other => other.to_string(),
                };
                cmd.env(&var_name, val_str);
            }
        }

        cmd.env("TOOL_NAME", &tool.name);
        cmd.env("TOOL_ARGS_JSON", serde_json::to_string(args).unwrap_or_default());

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to execute {} script for '{}': {}", interp, name, e))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let mut res = String::new();
        if !stdout.is_empty() {
            res.push_str(&stdout);
        }
        if !stderr.is_empty() {
            if !res.is_empty() {
                res.push_str("\n--- Error Stream ---\n");
            }
            res.push_str(&stderr);
        }
        if res.is_empty() {
            res = format!("Tool '{}' completed with exit status: {:?}", name, output.status.code());
        }

        if output.status.success() {
            Ok(res)
        } else {
            Err(format!("Tool '{}' failed with exit code {:?}:\n{}", name, output.status.code(), res))
        }
    }
}
