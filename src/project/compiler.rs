#![allow(dead_code)]
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProjectType {
    RustCargo,
    DotNet,
    TypeScriptNode,
    Python,
    CMake,
    Go,
    RobloxLuau,
    Unknown,
}

impl ProjectType {
    pub fn display_name(&self) -> &'static str {
        match self {
            ProjectType::RustCargo => "Rust (Cargo)",
            ProjectType::DotNet => ".NET (C# / F# / VB)",
            ProjectType::TypeScriptNode => "Node.js / TypeScript",
            ProjectType::Python => "Python",
            ProjectType::CMake => "CMake / C++",
            ProjectType::Go => "Go",
            ProjectType::RobloxLuau => "Roblox (Luau / Rojo)",
            ProjectType::Unknown => "Generic / Unknown",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BuildMode {
    Check,
    Build,
    Release,
    Test,
    Clean,
    Format,
    Lint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileDiagnostic {
    pub file: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub severity: String, // "error", "warning", "info"
    pub code: Option<String>,
    pub message: String,
    pub snippet: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileResult {
    pub success: bool,
    pub project_type: String,
    pub command: String,
    pub duration_ms: u64,
    pub exit_code: Option<i32>,
    pub error_count: usize,
    pub warning_count: usize,
    pub diagnostics: Vec<CompileDiagnostic>,
    pub raw_stdout: String,
    pub raw_stderr: String,
    pub summary: String,
}

pub struct ProjectCompiler;

impl ProjectCompiler {
    /// Automatically detects the project type by scanning manifests and configuration files
    pub fn detect_project_type(project_dir: &Path) -> ProjectType {
        if project_dir.join("Cargo.toml").exists() {
            return ProjectType::RustCargo;
        }

        // Check for .NET (.csproj, .sln, .fsproj)
        if let Ok(entries) = std::fs::read_dir(project_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_lowercase();
                if name.ends_with(".csproj") || name.ends_with(".sln") || name.ends_with(".fsproj") {
                    return ProjectType::DotNet;
                }
            }
        }

        // Check for Node / TypeScript
        if project_dir.join("tsconfig.json").exists() || project_dir.join("package.json").exists() {
            return ProjectType::TypeScriptNode;
        }

        // Check for Roblox Rojo / Luau
        if project_dir.join("default.project.json").exists() || project_dir.join("wally.toml").exists() {
            return ProjectType::RobloxLuau;
        }

        // Check for Python
        if project_dir.join("pyproject.toml").exists()
            || project_dir.join("requirements.txt").exists()
            || project_dir.join("setup.py").exists()
        {
            return ProjectType::Python;
        }

        // Check for Go
        if project_dir.join("go.mod").exists() {
            return ProjectType::Go;
        }

        // Check for CMake
        if project_dir.join("CMakeLists.txt").exists() {
            return ProjectType::CMake;
        }

        ProjectType::Unknown
    }

    /// Compiles, checks, tests, or cleans the project using the native compiler toolchain
    pub fn compile(project_dir: &Path, mode: BuildMode) -> CompileResult {
        let p_type = Self::detect_project_type(project_dir);
        let start = Instant::now();

        let (cmd_name, cmd_args) = match (&p_type, &mode) {
            // Rust Cargo
            (ProjectType::RustCargo, BuildMode::Check) => ("cargo", vec!["check", "--message-format=short"]),
            (ProjectType::RustCargo, BuildMode::Build) => ("cargo", vec!["build"]),
            (ProjectType::RustCargo, BuildMode::Release) => ("cargo", vec!["build", "--release"]),
            (ProjectType::RustCargo, BuildMode::Test) => ("cargo", vec!["test"]),
            (ProjectType::RustCargo, BuildMode::Clean) => ("cargo", vec!["clean"]),
            (ProjectType::RustCargo, BuildMode::Format) => ("cargo", vec!["fmt", "--check"]),
            (ProjectType::RustCargo, BuildMode::Lint) => ("cargo", vec!["clippy"]),

            // .NET C# / F# / VB
            (ProjectType::DotNet, BuildMode::Check) => ("dotnet", vec!["build", "--no-restore", "-v", "q"]),
            (ProjectType::DotNet, BuildMode::Build) => ("dotnet", vec!["build"]),
            (ProjectType::DotNet, BuildMode::Release) => ("dotnet", vec!["build", "-c", "Release"]),
            (ProjectType::DotNet, BuildMode::Test) => ("dotnet", vec!["test"]),
            (ProjectType::DotNet, BuildMode::Clean) => ("dotnet", vec!["clean"]),
            (ProjectType::DotNet, BuildMode::Format) => ("dotnet", vec!["format", "--verify-no-changes"]),
            (ProjectType::DotNet, BuildMode::Lint) => ("dotnet", vec!["build", "-warnaserror"]),

            // Node / TypeScript
            (ProjectType::TypeScriptNode, BuildMode::Check) => {
                if project_dir.join("tsconfig.json").exists() {
                    ("npx", vec!["tsc", "--noEmit"])
                } else {
                    ("npm", vec!["run", "check"])
                }
            }
            (ProjectType::TypeScriptNode, BuildMode::Build) => ("npm", vec!["run", "build"]),
            (ProjectType::TypeScriptNode, BuildMode::Release) => ("npm", vec!["run", "build"]),
            (ProjectType::TypeScriptNode, BuildMode::Test) => ("npm", vec!["test"]),
            (ProjectType::TypeScriptNode, BuildMode::Clean) => ("npm", vec!["run", "clean"]),
            (ProjectType::TypeScriptNode, BuildMode::Format) => ("npx", vec!["prettier", "--check", "."]),
            (ProjectType::TypeScriptNode, BuildMode::Lint) => ("npx", vec!["eslint", "."]),

            // Python
            (ProjectType::Python, BuildMode::Check) => ("python", vec!["-m", "py_compile"]),
            (ProjectType::Python, BuildMode::Build) => ("python", vec!["-m", "build"]),
            (ProjectType::Python, BuildMode::Release) => ("python", vec!["-m", "build"]),
            (ProjectType::Python, BuildMode::Test) => ("pytest", vec![]),
            (ProjectType::Python, BuildMode::Clean) => ("python", vec!["-c", "import pathlib, shutil; [shutil.rmtree(p) for p in pathlib.Path('.').rglob('__pycache__')]"]),
            (ProjectType::Python, BuildMode::Format) => ("black", vec!["--check", "."]),
            (ProjectType::Python, BuildMode::Lint) => ("ruff", vec!["check", "."]),

            // Go
            (ProjectType::Go, BuildMode::Check) => ("go", vec!["vet", "./..."]),
            (ProjectType::Go, BuildMode::Build) => ("go", vec!["build", "./..."]),
            (ProjectType::Go, BuildMode::Release) => ("go", vec!["build", "-ldflags=-s -w", "./..."]),
            (ProjectType::Go, BuildMode::Test) => ("go", vec!["test", "./..."]),
            (ProjectType::Go, BuildMode::Clean) => ("go", vec!["clean"]),
            (ProjectType::Go, BuildMode::Format) => ("gofmt", vec!["-l", "."]),
            (ProjectType::Go, BuildMode::Lint) => ("golangci-lint", vec!["run"]),

            // CMake / C++
            (ProjectType::CMake, BuildMode::Check | BuildMode::Build) => ("cmake", vec!["--build", "."]),
            (ProjectType::CMake, BuildMode::Release) => ("cmake", vec!["--build", ".", "--config", "Release"]),
            (ProjectType::CMake, BuildMode::Test) => ("ctest", vec![]),
            (ProjectType::CMake, BuildMode::Clean) => ("cmake", vec!["--build", ".", "--target", "clean"]),
            (ProjectType::CMake, BuildMode::Format) => ("clang-format", vec!["-style=file", "-n", "*"]),
            (ProjectType::CMake, BuildMode::Lint) => ("clang-tidy", vec![]),

            // Roblox Luau / Rojo
            (ProjectType::RobloxLuau, BuildMode::Build | BuildMode::Release) => ("rojo", vec!["build", "-o", "place.rbxl"]),
            (ProjectType::RobloxLuau, BuildMode::Check | BuildMode::Lint) => ("selene", vec!["."]),
            (ProjectType::RobloxLuau, BuildMode::Test) => ("run-in-roblox", vec!["--place", "place.rbxl", "--script", "tests.lua"]),
            (ProjectType::RobloxLuau, _) => ("rojo", vec!["build", "-o", "place.rbxl"]),

            // Fallback Generic
            (ProjectType::Unknown, _) => {
                return CompileResult {
                    success: false,
                    project_type: "Unknown".to_string(),
                    command: "none".to_string(),
                    duration_ms: 0,
                    exit_code: Some(1),
                    error_count: 1,
                    warning_count: 0,
                    diagnostics: vec![CompileDiagnostic {
                        file: None,
                        line: None,
                        column: None,
                        severity: "error".to_string(),
                        code: None,
                        message: "No recognized build system or manifest found in project directory (expected Cargo.toml, .csproj, package.json, CMakeLists.txt, or default.project.json).".to_string(),
                        snippet: None,
                    }],
                    raw_stdout: String::new(),
                    raw_stderr: "No supported manifest found.".to_string(),
                    summary: "Build failed: Unrecognized project type.".to_string(),
                };
            }
        };

        let full_command = format!("{} {}", cmd_name, cmd_args.join(" "));

        let mut cmd = Command::new(cmd_name);
        cmd.args(&cmd_args);
        cmd.current_dir(project_dir);

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        let output_res = cmd.output();
        let duration_ms = start.elapsed().as_millis() as u64;

        match output_res {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let exit_code = output.status.code();
                let success = output.status.success();

                let combined_log = format!("{}\n{}", stdout, stderr);
                let diagnostics = Self::parse_diagnostics(&p_type, &combined_log);

                let error_count = diagnostics.iter().filter(|d| d.severity == "error").count();
                let warning_count = diagnostics.iter().filter(|d| d.severity == "warning").count();

                let summary = if success {
                    if warning_count > 0 {
                        format!("✓ Compilation Succeeded with {} warning(s) in {}ms.", warning_count, duration_ms)
                    } else {
                        format!("✓ Compilation Succeeded cleanly in {}ms.", duration_ms)
                    }
                } else {
                    format!("✗ Compilation Failed with {} error(s), {} warning(s) in {}ms.", error_count.max(1), warning_count, duration_ms)
                };

                CompileResult {
                    success,
                    project_type: p_type.display_name().to_string(),
                    command: full_command,
                    duration_ms,
                    exit_code,
                    error_count,
                    warning_count,
                    diagnostics,
                    raw_stdout: stdout,
                    raw_stderr: stderr,
                    summary,
                }
            }
            Err(e) => CompileResult {
                success: false,
                project_type: p_type.display_name().to_string(),
                command: full_command,
                duration_ms,
                exit_code: Some(1),
                error_count: 1,
                warning_count: 0,
                diagnostics: vec![CompileDiagnostic {
                    file: None,
                    line: None,
                    column: None,
                    severity: "error".to_string(),
                    code: None,
                    message: format!("Failed to invoke compiler `{}`: {}. Is it installed in system PATH?", cmd_name, e),
                    snippet: None,
                }],
                raw_stdout: String::new(),
                raw_stderr: format!("Compiler execution error: {}", e),
                summary: format!("Build failed: Could not execute `{}`.", cmd_name),
            },
        }
    }

    /// Parses compiler diagnostics from compiler output logs across Rust, .NET, TypeScript, GCC/Clang, Python, etc.
    pub fn parse_diagnostics(_p_type: &ProjectType, log: &str) -> Vec<CompileDiagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = log.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Ignore cargo summary banners
            if trimmed.starts_with("error: aborting due to")
                || trimmed.starts_with("warning: `")
                || trimmed.starts_with("error: could not compile")
            {
                continue;
            }

            // 1. Rust error format: error[E0425]: cannot find value `...` in this scope
            //    --> src\ui\chat_view.rs:213:48
            if trimmed.starts_with("error[") || (trimmed.starts_with("error:") && !trimmed.contains("could not compile")) {
                let code = if let Some(bracket_end) = trimmed.find(']') {
                    Some(trimmed[6..bracket_end].to_string())
                } else {
                    None
                };
                let message = trimmed.to_string();

                let mut file = None;
                let mut line_num = None;
                let mut col_num = None;
                let mut snippet = None;

                // Look ahead for "--> path:line:col"
                if i + 1 < lines.len() && lines[i + 1].trim().starts_with("-->") {
                    let ptr_line = lines[i + 1].trim();
                    let after_arrow = ptr_line.trim_start_matches("-->").trim();
                    let parts: Vec<&str> = after_arrow.split(':').collect();
                    if parts.len() >= 3 {
                        file = Some(parts[0].to_string());
                        line_num = parts[1].parse::<u32>().ok();
                        col_num = parts[2].parse::<u32>().ok();
                    } else if parts.len() >= 2 {
                        file = Some(parts[0].to_string());
                        line_num = parts[1].parse::<u32>().ok();
                    }
                    if i + 2 < lines.len() {
                        snippet = Some(lines[i + 2].to_string());
                    }
                }

                diagnostics.push(CompileDiagnostic {
                    file,
                    line: line_num,
                    column: col_num,
                    severity: "error".to_string(),
                    code,
                    message,
                    snippet,
                });
            } else if trimmed.starts_with("warning:") || trimmed.starts_with("warning[") {
                let code = if let Some(bracket_end) = trimmed.find(']') {
                    Some(trimmed[8..bracket_end].to_string())
                } else {
                    None
                };
                let message = trimmed.to_string();

                let mut file = None;
                let mut line_num = None;
                let mut col_num = None;
                let mut snippet = None;

                if i + 1 < lines.len() && lines[i + 1].trim().starts_with("-->") {
                    let ptr_line = lines[i + 1].trim();
                    let after_arrow = ptr_line.trim_start_matches("-->").trim();
                    let parts: Vec<&str> = after_arrow.split(':').collect();
                    if parts.len() >= 3 {
                        file = Some(parts[0].to_string());
                        line_num = parts[1].parse::<u32>().ok();
                        col_num = parts[2].parse::<u32>().ok();
                    } else if parts.len() >= 2 {
                        file = Some(parts[0].to_string());
                        line_num = parts[1].parse::<u32>().ok();
                    }
                    if i + 2 < lines.len() {
                        snippet = Some(lines[i + 2].to_string());
                    }
                }

                diagnostics.push(CompileDiagnostic {
                    file,
                    line: line_num,
                    column: col_num,
                    severity: "warning".to_string(),
                    code,
                    message,
                    snippet,
                });
            }
            // 2. .NET / C# format: src\Program.cs(12,5): error CS0103: The name 'foo' does not exist in the current context
            else if trimmed.contains("): error ") || trimmed.contains("): warning ") {
                let is_err = trimmed.contains("): error ");
                let parts: Vec<&str> = trimmed.split("): ").collect();
                if parts.len() == 2 {
                    let loc_part = parts[0];
                    let msg_part = parts[1];

                    let mut file = None;
                    let mut line_num = None;
                    let mut col_num = None;

                    if let Some(open_paren) = loc_part.find('(') {
                        file = Some(loc_part[..open_paren].to_string());
                        let coords = &loc_part[open_paren + 1..];
                        let coord_parts: Vec<&str> = coords.split(',').collect();
                        if coord_parts.len() == 2 {
                            line_num = coord_parts[0].trim().parse::<u32>().ok();
                            col_num = coord_parts[1].trim().parse::<u32>().ok();
                        }
                    }

                    let code = if let Some(rest) = msg_part.strip_prefix("error ").or_else(|| msg_part.strip_prefix("warning ")) {
                        rest.split(':').next().map(|c| c.trim().to_string())
                    } else {
                        None
                    };

                    diagnostics.push(CompileDiagnostic {
                        file,
                        line: line_num,
                        column: col_num,
                        severity: if is_err { "error".to_string() } else { "warning".to_string() },
                        code,
                        message: msg_part.to_string(),
                        snippet: None,
                    });
                }
            }
            // 3. TypeScript / GCC / Clang format: src/index.ts:12:5 - error TS2304: Cannot find name 'x'.
            else if trimmed.contains(" - error ") || trimmed.contains(" - warning ") || trimmed.contains(": error ") || trimmed.contains(": warning ") {
                let is_err = trimmed.contains("error");
                let dash_split: Vec<&str> = if trimmed.contains(" - ") {
                    trimmed.split(" - ").collect()
                } else if is_err {
                    trimmed.split(": error: ").collect()
                } else {
                    trimmed.split(": warning: ").collect()
                };

                if dash_split.len() >= 2 {
                    let loc_part = dash_split[0];
                    let msg_part = dash_split[1];

                    let loc_tokens: Vec<&str> = loc_part.split(':').collect();
                    let file = if !loc_tokens.is_empty() { Some(loc_tokens[0].to_string()) } else { None };
                    let line_num = if loc_tokens.len() >= 2 { loc_tokens[1].trim().parse::<u32>().ok() } else { None };
                    let col_num = if loc_tokens.len() >= 3 { loc_tokens[2].trim().parse::<u32>().ok() } else { None };

                    let code = if let Some(pos) = msg_part.find("TS") {
                        msg_part[pos..].split(':').next().map(|s| s.trim().to_string())
                    } else {
                        None
                    };

                    diagnostics.push(CompileDiagnostic {
                        file,
                        line: line_num,
                        column: col_num,
                        severity: if is_err { "error".to_string() } else { "warning".to_string() },
                        code,
                        message: msg_part.to_string(),
                        snippet: None,
                    });
                }
            }
        }

        diagnostics
    }
}
