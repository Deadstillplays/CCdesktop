use super::types::*;
use crate::storage::{AttachmentKind, ChatAttachment};
use chrono::Utc;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;
use walkdir::WalkDir;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeSymbol {
    pub name: String,
    pub kind: String,
    pub line: usize,
    pub signature: String,
}

#[derive(Clone)]
pub struct ProjectManager {
    storage_file: PathBuf,
}

impl ProjectManager {
    pub fn extract_symbols(ext: &str, content: &str) -> Vec<CodeSymbol> {
        let mut symbols = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty()
                || trimmed.starts_with("//")
                || trimmed.starts_with('#')
                || trimmed.starts_with("/*")
                || trimmed.starts_with('*')
            {
                continue;
            }

            let line_num = idx + 1;
            match ext {
                "rs" => {
                    let clean = trimmed
                        .trim_start_matches("pub ")
                        .trim_start_matches("pub(crate) ")
                        .trim_start_matches("async ")
                        .trim();
                    if let Some(rest) = clean.strip_prefix("fn ") {
                        let name = rest.split(|c: char| c == '(' || c == '<' || c == ' ').next().unwrap_or("").trim().to_string();
                        if !name.is_empty() {
                            let sig = trimmed.trim_end_matches('{').trim().to_string();
                            symbols.push(CodeSymbol { name, kind: "fn".to_string(), line: line_num, signature: sig });
                        }
                    } else if let Some(rest) = clean.strip_prefix("struct ") {
                        let name = rest.split(|c: char| c == '<' || c == '{' || c == ';' || c == ' ').next().unwrap_or("").trim().to_string();
                        if !name.is_empty() {
                            symbols.push(CodeSymbol { name, kind: "struct".to_string(), line: line_num, signature: trimmed.trim_end_matches('{').trim().to_string() });
                        }
                    } else if let Some(rest) = clean.strip_prefix("enum ") {
                        let name = rest.split(|c: char| c == '<' || c == '{' || c == ' ').next().unwrap_or("").trim().to_string();
                        if !name.is_empty() {
                            symbols.push(CodeSymbol { name, kind: "enum".to_string(), line: line_num, signature: trimmed.trim_end_matches('{').trim().to_string() });
                        }
                    } else if let Some(rest) = clean.strip_prefix("trait ") {
                        let name = rest.split(|c: char| c == '<' || c == '{' || c == ' ').next().unwrap_or("").trim().to_string();
                        if !name.is_empty() {
                            symbols.push(CodeSymbol { name, kind: "trait".to_string(), line: line_num, signature: trimmed.trim_end_matches('{').trim().to_string() });
                        }
                    } else if clean.starts_with("impl ") {
                        let sig = trimmed.trim_end_matches('{').trim().to_string();
                        let name = sig.clone();
                        symbols.push(CodeSymbol { name, kind: "impl".to_string(), line: line_num, signature: sig });
                    }
                }
                "py" => {
                    let clean = trimmed.trim_start_matches("async ").trim();
                    if let Some(rest) = clean.strip_prefix("def ") {
                        let name = rest.split('(').next().unwrap_or("").trim().to_string();
                        if !name.is_empty() {
                            symbols.push(CodeSymbol { name, kind: "def".to_string(), line: line_num, signature: trimmed.trim_end_matches(':').trim().to_string() });
                        }
                    } else if let Some(rest) = clean.strip_prefix("class ") {
                        let name = rest.split(|c: char| c == '(' || c == ':').next().unwrap_or("").trim().to_string();
                        if !name.is_empty() {
                            symbols.push(CodeSymbol { name, kind: "class".to_string(), line: line_num, signature: trimmed.trim_end_matches(':').trim().to_string() });
                        }
                    }
                }
                "ts" | "tsx" | "js" | "jsx" => {
                    let clean = trimmed.trim_start_matches("export ").trim_start_matches("default ").trim_start_matches("async ").trim();
                    if let Some(rest) = clean.strip_prefix("function ") {
                        let name = rest.split(|c: char| c == '(' || c == '<' || c == ' ').next().unwrap_or("").trim().to_string();
                        if !name.is_empty() {
                            symbols.push(CodeSymbol { name, kind: "function".to_string(), line: line_num, signature: trimmed.trim_end_matches('{').trim().to_string() });
                        }
                    } else if let Some(rest) = clean.strip_prefix("class ") {
                        let name = rest.split(|c: char| c == '<' || c == '{' || c == ' ').next().unwrap_or("").trim().to_string();
                        if !name.is_empty() {
                            symbols.push(CodeSymbol { name, kind: "class".to_string(), line: line_num, signature: trimmed.trim_end_matches('{').trim().to_string() });
                        }
                    } else if let Some(rest) = clean.strip_prefix("interface ") {
                        let name = rest.split(|c: char| c == '<' || c == '{' || c == ' ').next().unwrap_or("").trim().to_string();
                        if !name.is_empty() {
                            symbols.push(CodeSymbol { name, kind: "interface".to_string(), line: line_num, signature: trimmed.trim_end_matches('{').trim().to_string() });
                        }
                    } else if let Some(rest) = clean.strip_prefix("type ") {
                        let name = rest.split(|c: char| c == '<' || c == '=' || c == ' ').next().unwrap_or("").trim().to_string();
                        if !name.is_empty() {
                            symbols.push(CodeSymbol { name, kind: "type".to_string(), line: line_num, signature: trimmed.to_string() });
                        }
                    } else if clean.starts_with("const ") && clean.contains("=>") {
                        let rest = clean.trim_start_matches("const ").trim();
                        let name = rest.split(|c: char| c == ':' || c == '=' || c == ' ').next().unwrap_or("").trim().to_string();
                        if !name.is_empty() {
                            symbols.push(CodeSymbol { name, kind: "const_fn".to_string(), line: line_num, signature: trimmed.trim_end_matches('{').trim().to_string() });
                        }
                    }
                }
                "cs" => {
                    let clean = trimmed
                        .trim_start_matches("public ")
                        .trim_start_matches("private ")
                        .trim_start_matches("protected ")
                        .trim_start_matches("internal ")
                        .trim_start_matches("static ")
                        .trim_start_matches("async ")
                        .trim();
                    if let Some(rest) = clean.strip_prefix("class ") {
                        let name = rest.split(|c: char| c == '<' || c == ':' || c == '{' || c == ' ').next().unwrap_or("").trim().to_string();
                        if !name.is_empty() {
                            symbols.push(CodeSymbol { name, kind: "class".to_string(), line: line_num, signature: trimmed.trim_end_matches('{').trim().to_string() });
                        }
                    } else if let Some(rest) = clean.strip_prefix("interface ") {
                        let name = rest.split(|c: char| c == '<' || c == ':' || c == '{' || c == ' ').next().unwrap_or("").trim().to_string();
                        if !name.is_empty() {
                            symbols.push(CodeSymbol { name, kind: "interface".to_string(), line: line_num, signature: trimmed.trim_end_matches('{').trim().to_string() });
                        }
                    } else if clean.contains('(') && clean.contains(')') && !clean.starts_with("if") && !clean.starts_with("while") && !clean.starts_with("for") {
                        let before_paren = clean.split('(').next().unwrap_or("").trim();
                        let name = before_paren.split_whitespace().last().unwrap_or("").to_string();
                        if !name.is_empty() && name != "new" {
                            symbols.push(CodeSymbol { name, kind: "method".to_string(), line: line_num, signature: trimmed.trim_end_matches('{').trim().to_string() });
                        }
                    }
                }
                "lua" | "luau" => {
                    if let Some(rest) = trimmed.strip_prefix("local function ") {
                        let name = rest.split('(').next().unwrap_or("").trim().to_string();
                        if !name.is_empty() {
                            symbols.push(CodeSymbol { name, kind: "fn".to_string(), line: line_num, signature: trimmed.to_string() });
                        }
                    } else if let Some(rest) = trimmed.strip_prefix("function ") {
                        let name = rest.split('(').next().unwrap_or("").trim().to_string();
                        if !name.is_empty() {
                            symbols.push(CodeSymbol { name, kind: "fn".to_string(), line: line_num, signature: trimmed.to_string() });
                        }
                    }
                }
                _ => {}
            }
        }

        symbols
    }
    pub fn new(storage_dir: &Path) -> Self {
        let storage_file = storage_dir.join("projects.json");
        Self { storage_file }
    }

    pub fn list_projects(&self) -> Vec<Project> {
        if self.storage_file.exists() {
            if let Ok(content) = fs::read_to_string(&self.storage_file) {
                if let Ok(projects) = serde_json::from_str::<Vec<Project>>(&content) {
                    return projects;
                }
            }
        }
        Vec::new()
    }

    pub fn save_projects(&self, projects: &[Project]) -> Result<(), String> {
        if let Some(parent) = self.storage_file.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let json = serde_json::to_string_pretty(projects).map_err(|e| e.to_string())?;
        fs::write(&self.storage_file, json).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn create_project(&self, name: &str, folder_path: &str) -> Result<Project, String> {
        let p_path = Path::new(folder_path);
        if !p_path.exists() {
            return Err(format!("Directory does not exist: {}", folder_path));
        }

        let mut projects = self.list_projects();

        // Check if CLAUDE.md exists in the folder
        let claude_md = p_path.join("CLAUDE.md");
        let initial_instructions = if claude_md.exists() {
            fs::read_to_string(&claude_md).unwrap_or_default()
        } else {
            format!("# Project: {}\n\n- Main project guidelines and rules for Claude\n", name)
        };

        let new_proj = Project {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            folder_path: folder_path.to_string(),
            instructions: initial_instructions,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        projects.push(new_proj.clone());
        self.save_projects(&projects)?;
        Ok(new_proj)
    }

    pub fn delete_project(&self, id: &str) -> Result<(), String> {
        let mut projects = self.list_projects();
        projects.retain(|p| p.id != id);
        self.save_projects(&projects)
    }

    #[allow(dead_code)]
    pub fn update_project(&self, project: &Project) -> Result<(), String> {
        let mut projects = self.list_projects();
        if let Some(existing) = projects.iter_mut().find(|p| p.id == project.id) {
            *existing = project.clone();
            self.save_projects(&projects)?;
        }
        Ok(())
    }

    pub fn scan_files(&self, folder_path: &str, max_files: usize) -> Vec<ProjectFileEntry> {
        let root = Path::new(folder_path);
        if !root.exists() {
            return Vec::new();
        }

        let mut items = Vec::new();
        let walker = WalkDir::new(root)
            .max_depth(8)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                !name.starts_with('.')
                    && name != "target"
                    && name != "node_modules"
                    && name != "dist"
                    && name != "build"
                    && name != ".git"
            });

        for entry in walker.flatten() {
            if entry.path() == root {
                continue;
            }
            if let Ok(rel) = entry.path().strip_prefix(root) {
                let rel_str = rel.to_string_lossy().replace('\\', "/");
                let is_dir = entry.file_type().is_dir();
                let meta = entry.metadata().ok();
                let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                let updated_at = meta
                    .and_then(|m| m.modified().ok())
                    .map(|t| chrono::DateTime::<Utc>::from(t))
                    .unwrap_or_else(Utc::now);

                items.push(ProjectFileEntry {
                    relative_path: rel_str,
                    is_dir,
                    size,
                    updated_at,
                });

                if items.len() >= max_files {
                    break;
                }
            }
        }
        items
    }

    pub fn resolve_path(&self, folder_path: Option<&str>, path_str: &str) -> PathBuf {
        let expanded = crate::mcp::client::expand_vars(path_str);
        let path = Path::new(&expanded);
        if path.is_absolute() {
            path.to_path_buf()
        } else if let Some(base) = folder_path {
            Path::new(base).join(path)
        } else {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(path)
        }
    }

    pub fn read_file_any(
        &self,
        folder_path: Option<&str>,
        path_str: &str,
        start_line: Option<usize>,
        line_count: Option<usize>,
    ) -> Result<String, String> {
        let target = self.resolve_path(folder_path, path_str);
        if !target.exists() {
            return Err(format!("File does not exist: {}", target.display()));
        }
        let content = fs::read_to_string(&target)
            .map_err(|e| format!("Failed to read {}: {}", target.display(), e))?;

        let lines: Vec<&str> = content.lines().collect();
        let total = lines.len();

        if total == 0 {
            return Ok(String::new());
        }

        if start_line.is_none() && line_count.is_none() {
            if total <= 160 {
                let mut out = String::new();
                for (idx, line) in lines.iter().enumerate() {
                    out.push_str(&format!("{:4}: {}\n", idx + 1, line));
                }
                return Ok(out);
            } else {
                let ext = target.extension().unwrap_or_default().to_string_lossy().to_lowercase();
                let symbols = Self::extract_symbols(&ext, &content);

                let mut out = format!("📄 Large file preview (Lines 1..70 of {} total lines in '{}'):\n", total, target.display());
                for (idx, line) in lines[..70].iter().enumerate() {
                    out.push_str(&format!("{:4}: {}\n", idx + 1, line));
                }

                out.push_str(&format!("\n... [Remaining {} lines omitted for token efficiency] ...\n\n", total - 70));
                out.push_str("💡 Symbols available in this file (Use `read_symbol(path, symbol)` to view full function/class bodies with zero hallucination):\n");
                for sym in symbols {
                    out.push_str(&format!("  - [{}] L{}: {}\n", sym.kind, sym.line, sym.signature));
                }
                out.push_str("\n💡 Tip: To read specific lines, call `read_file(path, start_line, line_count)`.");
                return Ok(out);
            }
        }

        let start = start_line.unwrap_or(1).max(1);
        let count = line_count.unwrap_or(total);

        if start > total {
            return Ok(format!(
                "File '{}' has {} total lines. Requested start_line {} is beyond end of file.",
                target.display(),
                total,
                start
            ));
        }

        let end = (start + count - 1).min(total);
        let mut out = String::new();
        for (offset, line) in lines[start - 1..end].iter().enumerate() {
            let actual_line = start + offset;
            out.push_str(&format!("{:4}: {}\n", actual_line, line));
        }
        Ok(out)
    }

    pub fn read_symbol_any(
        &self,
        folder_path: Option<&str>,
        path_str: &str,
        symbol_name: &str,
    ) -> Result<String, String> {
        let target = self.resolve_path(folder_path, path_str);
        if !target.exists() {
            return Err(format!("File does not exist: {}", target.display()));
        }
        let content = fs::read_to_string(&target)
            .map_err(|e| format!("Failed to read {}: {}", target.display(), e))?;

        let ext = target.extension().unwrap_or_default().to_string_lossy().to_lowercase();
        let symbols = Self::extract_symbols(&ext, &content);

        let clean_query = symbol_name.trim();
        let matched = symbols.iter().find(|s| s.name == clean_query)
            .or_else(|| symbols.iter().find(|s| s.name.eq_ignore_ascii_case(clean_query)))
            .or_else(|| symbols.iter().find(|s| s.name.contains(clean_query)));

        let lines: Vec<&str> = content.lines().collect();
        let total = lines.len();

        if let Some(sym) = matched {
            let start_idx = sym.line.saturating_sub(1);
            let mut end_idx = start_idx;

            if ["rs", "ts", "tsx", "js", "jsx", "cs", "c", "cpp", "java", "go"].contains(&ext.as_str()) {
                let mut brace_count = 0;
                let mut started = false;
                for i in start_idx..total {
                    let line = lines[i];
                    for ch in line.chars() {
                        if ch == '{' {
                            brace_count += 1;
                            started = true;
                        } else if ch == '}' {
                            brace_count -= 1;
                        }
                    }
                    end_idx = i;
                    if started && brace_count <= 0 {
                        break;
                    }
                }
            } else if ext == "py" {
                let first_line = lines[start_idx];
                let base_indent = first_line.len() - first_line.trim_start().len();
                end_idx = start_idx;
                for i in (start_idx + 1)..total {
                    let line = lines[i];
                    let trimmed = line.trim();
                    if trimmed.is_empty() || trimmed.starts_with('#') {
                        continue;
                    }
                    let line_indent = line.len() - line.trim_start().len();
                    if line_indent <= base_indent {
                        break;
                    }
                    end_idx = i;
                }
            } else {
                end_idx = (start_idx + 50).min(total.saturating_sub(1));
            }

            let mut out = format!(
                "=== Symbol '{}' ({}) in '{}' (Lines {}..{}) ===\n",
                sym.name,
                sym.kind,
                target.display(),
                start_idx + 1,
                end_idx + 1
            );
            for i in start_idx..=end_idx {
                out.push_str(&format!("{:4}: {}\n", i + 1, lines[i]));
            }
            Ok(out)
        } else {
            let available: Vec<String> = symbols.iter().map(|s| format!("{} (L{})", s.name, s.line)).collect();
            Err(format!(
                "Symbol '{}' not found in '{}'. Available symbols: {}",
                symbol_name,
                target.display(),
                if available.is_empty() { "none detected".to_string() } else { available.join(", ") }
            ))
        }
    }

    pub fn get_codebase_map(&self, folder_path: Option<&str>) -> Result<String, String> {
        let base_dir = if let Some(base) = folder_path {
            PathBuf::from(base)
        } else {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        };

        if !base_dir.exists() {
            return Err(format!("Directory does not exist: {}", base_dir.display()));
        }

        let mut out = format!("🗺 CODEBASE ARCHITECTURAL MAP: {}\n\n", base_dir.display());
        let mut file_count = 0;
        let mut total_lines = 0;

        for entry in walkdir::WalkDir::new(&base_dir)
            .max_depth(5)
            .into_iter()
            .filter_entry(|e| {
                let n = e.file_name().to_string_lossy();
                !n.starts_with('.')
                    && n != "node_modules"
                    && n != "target"
                    && n != "dist"
                    && n != "build"
                    && n != ".git"
                    && n != "bin"
                    && n != "obj"
            })
            .flatten()
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let ext = path.extension().unwrap_or_default().to_string_lossy().to_lowercase();
            let code_exts = ["rs", "py", "ts", "tsx", "js", "jsx", "cs", "go", "lua", "luau", "java"];
            if !code_exts.contains(&ext.as_str()) {
                continue;
            }

            if let Ok(content) = fs::read_to_string(path) {
                file_count += 1;
                let rel = path
                    .strip_prefix(&base_dir)
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| path.display().to_string());
                let lines: Vec<&str> = content.lines().collect();
                let line_len = lines.len();
                total_lines += line_len;

                let symbols = Self::extract_symbols(&ext, &content);
                let sym_summary: Vec<String> = symbols
                    .iter()
                    .take(6)
                    .map(|s| format!("{}:L{}", s.name, s.line))
                    .collect();

                out.push_str(&format!("📄 {} ({} lines)", rel, line_len));
                if !sym_summary.is_empty() {
                    out.push_str(&format!(" -> [{}]", sym_summary.join(", ")));
                    if symbols.len() > 6 {
                        out.push_str(&format!(" (+{} more)", symbols.len() - 6));
                    }
                }
                out.push('\n');

                if file_count >= 50 {
                    out.push_str("\n... [Showing first 50 source files. Use `find_files` or `read_project_skeleton` for more] ...\n");
                    break;
                }
            }
        }

        out.push_str(&format!("\nTotal: {} files indexed, {} total lines of code.\n", file_count, total_lines));
        Ok(out)
    }

    pub fn read_project_skeleton(
        &self,
        folder_path: Option<&str>,
        subpath: Option<&str>,
    ) -> Result<String, String> {
        let base_dir = if let Some(sp) = subpath {
            self.resolve_path(folder_path, sp)
        } else if let Some(base) = folder_path {
            PathBuf::from(base)
        } else {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        };

        if !base_dir.exists() {
            return Err(format!("Directory does not exist: {}", base_dir.display()));
        }

        let mut out = format!("=== Project Skeleton: {} ===\n", base_dir.display());
        let mut file_count = 0;
        let mut total_lines_all = 0;

        for entry in walkdir::WalkDir::new(&base_dir)
            .max_depth(4)
            .into_iter()
            .filter_entry(|e| {
                let n = e.file_name().to_string_lossy();
                !n.starts_with('.')
                    && n != "node_modules"
                    && n != "target"
                    && n != "dist"
                    && n != "build"
                    && n != ".git"
                    && n != "bin"
                    && n != "obj"
            })
            .flatten()
        {
            if !entry.file_type().is_file() {
                continue;
            }

            let path = entry.path();
            let ext = path.extension().unwrap_or_default().to_string_lossy().to_lowercase();
            let code_exts = [
                "rs", "py", "ts", "tsx", "js", "jsx", "lua", "luau", "go", "java", "c", "cpp", "h",
                "hpp", "cs", "toml", "json", "yaml", "yml", "md", "html", "css",
            ];
            if !code_exts.contains(&ext.as_str()) {
                continue;
            }

            if let Ok(content) = fs::read_to_string(path) {
                file_count += 1;
                let rel_path = path
                    .strip_prefix(&base_dir)
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| path.display().to_string());
                let lines: Vec<&str> = content.lines().collect();
                let line_count = lines.len();
                total_lines_all += line_count;

                let mut signatures: Vec<String> = Vec::new();
                for (idx, l) in lines.iter().enumerate() {
                    let trimmed = l.trim();
                    if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('#') {
                        continue;
                    }
                    let is_sig = match ext.as_str() {
                        "rs" => {
                            trimmed.starts_with("pub struct ")
                                || trimmed.starts_with("struct ")
                                || trimmed.starts_with("pub enum ")
                                || trimmed.starts_with("enum ")
                                || trimmed.starts_with("pub fn ")
                                || trimmed.starts_with("fn ")
                                || trimmed.starts_with("pub trait ")
                                || trimmed.starts_with("trait ")
                                || trimmed.starts_with("impl ")
                                || trimmed.starts_with("mod ")
                        }
                        "py" => {
                            trimmed.starts_with("class ")
                                || trimmed.starts_with("def ")
                                || trimmed.starts_with("async def ")
                        }
                        "ts" | "tsx" | "js" | "jsx" => {
                            trimmed.starts_with("export ")
                                || trimmed.starts_with("function ")
                                || trimmed.starts_with("class ")
                                || trimmed.starts_with("interface ")
                                || trimmed.starts_with("type ")
                        }
                        "lua" | "luau" => {
                            trimmed.starts_with("function ")
                                || trimmed.starts_with("local function ")
                                || (trimmed.starts_with("local ") && trimmed.contains('='))
                        }
                        "toml" => trimmed.starts_with('['),
                        _ => false,
                    };

                    if is_sig {
                        let clean_sig = trimmed.trim_end_matches('{').trim().to_string();
                        signatures.push(format!("L{}: {}", idx + 1, clean_sig));
                        if signatures.len() >= 12 {
                            break;
                        }
                    }
                }

                out.push_str(&format!("\n[{}] ({} lines)\n", rel_path, line_count));
                if signatures.is_empty() {
                    out.push_str("  (configuration / data file)\n");
                } else {
                    for sig in signatures {
                        out.push_str(&format!("  - {}\n", sig));
                    }
                }

                if file_count >= 50 {
                    out.push_str("\n... [Showing first 50 code files. Call read_project_skeleton with 'subpath' for subdirectories] ...\n");
                    break;
                }
            }
        }

        out.push_str(&format!(
            "\nSummary: {} code files analyzed, {} total lines across project.\n",
            file_count, total_lines_all
        ));
        Ok(out)
    }

    pub fn run_subagent_task(
        &self,
        folder_path: Option<&str>,
        role: &str,
        task: &str,
        detail_level: Option<&str>,
    ) -> String {
        let base_dir = if let Some(base) = folder_path {
            self.resolve_path(None, base)
        } else {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        };
        crate::project::subagent::CodebaseSubagent::run_subagent(&base_dir, role, task, detail_level)
    }

    pub fn write_file_any(&self, folder_path: Option<&str>, path_str: &str, content: &str) -> Result<String, String> {
        let target = self.resolve_path(folder_path, path_str);
        if let Some(parent) = target.parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(&target, content).map_err(|e| format!("Failed to write {}: {}", target.display(), e))?;
        Ok(format!("Successfully wrote {} ({} bytes)", target.display(), content.len()))
    }

    pub fn edit_file_any(&self, folder_path: Option<&str>, path_str: &str, target_str: &str, replacement: &str) -> Result<String, String> {
        let target = self.resolve_path(folder_path, path_str);
        if !target.exists() {
            return Err(format!("File does not exist: {}", target.display()));
        }
        let content = fs::read_to_string(&target).map_err(|e| format!("Failed to read {}: {}", target.display(), e))?;

        if content.contains(target_str) {
            let new_content = content.replace(target_str, replacement);
            fs::write(&target, new_content).map_err(|e| format!("Failed to save edit to {}: {}", target.display(), e))?;
            return Ok(format!("Successfully edited {}", target.display()));
        }

        let norm_content = content.replace("\r\n", "\n");
        let norm_target = target_str.replace("\r\n", "\n");
        let norm_replacement = replacement.replace("\r\n", "\n");

        if norm_content.contains(&norm_target) {
            let new_content = norm_content.replace(&norm_target, &norm_replacement);
            fs::write(&target, new_content).map_err(|e| format!("Failed to save edit to {}: {}", target.display(), e))?;
            return Ok(format!("Successfully edited {}", target.display()));
        }

        Err(format!("Target string not found in {}", target.display()))
    }

    pub fn list_directory_any(&self, folder_path: Option<&str>, path_str: Option<&str>) -> Result<String, String> {
        let target = if let Some(p) = path_str {
            self.resolve_path(folder_path, p)
        } else if let Some(base) = folder_path {
            PathBuf::from(base)
        } else {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        };

        if !target.exists() {
            return Err(format!("Directory does not exist: {}", target.display()));
        }

        let mut output = format!("Directory listing for {}:\n", target.display());
        let entries = fs::read_dir(&target).map_err(|e| format!("Cannot read directory: {}", e))?;
        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            if is_dir {
                output.push_str(&format!("[DIR]  {}\n", file_name));
            } else {
                output.push_str(&format!("[FILE] {} ({} bytes)\n", file_name, size));
            }
        }
        Ok(output)
    }

    pub fn inspect_path_any(&self, folder_path: Option<&str>, path_str: &str) -> Result<String, String> {
        let target = self.resolve_path(folder_path, path_str);
        if !target.exists() {
            return Err(format!("Path does not exist: {}", target.display()));
        }

        let metadata = fs::metadata(&target).map_err(|e| e.to_string())?;
        if metadata.is_dir() {
            let mut out = format!("📁 DIRECTORY INSPECTION: {}\n", target.display());
            let mut file_count = 0;
            let mut dir_count = 0;
            let mut total_size = 0u64;
            let mut tree_preview = String::new();

            for entry in WalkDir::new(&target)
                .max_depth(3)
                .into_iter()
                .filter_entry(|e| {
                    let name = e.file_name().to_string_lossy();
                    !name.starts_with('.') && name != "node_modules" && name != "target"
                })
                .flatten()
            {
                let depth = entry.depth();
                let indent = "  ".repeat(depth);
                let name = entry.file_name().to_string_lossy();
                if entry.file_type().is_dir() {
                    if depth > 0 {
                        dir_count += 1;
                    }
                    tree_preview.push_str(&format!("{}[DIR]  {}/\n", indent, name));
                } else {
                    file_count += 1;
                    let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    total_size += size;
                    if file_count <= 40 {
                        tree_preview.push_str(&format!("{}[FILE] {} ({} KB)\n", indent, name, size / 1024));
                    }
                }
            }

            out.push_str(&format!(
                "Summary: {} files, {} subdirectories, total size: {:.2} MB\n\nStructure Preview:\n{}",
                file_count,
                dir_count,
                total_size as f64 / (1024.0 * 1024.0),
                tree_preview
            ));
            Ok(out)
        } else {
            let size = metadata.len();
            let mut out = format!("📄 FILE INSPECTION: {}\nSize: {} bytes ({:.2} KB)\n", target.display(), size, size as f64 / 1024.0);

            match fs::read(&target) {
                Ok(bytes) => {
                    if let Ok(text) = std::str::from_utf8(&bytes) {
                        let lines: Vec<&str> = text.lines().collect();
                        out.push_str(&format!("Format: UTF-8 Text ({} lines)\n\n--- CONTENT ---\n", lines.len()));
                        if lines.len() <= 600 {
                            out.push_str(text);
                        } else {
                            out.push_str(&lines[..400].join("\n"));
                            out.push_str(&format!("\n\n... [Truncated {} remaining lines] ...\n", lines.len() - 400));
                        }
                    } else {
                        out.push_str("Format: Binary Data\nHex Preview (first 128 bytes):\n");
                        for chunk in bytes.chunks(16).take(8) {
                            let hex_str: Vec<String> = chunk.iter().map(|b| format!("{:02X}", b)).collect();
                            let ascii_str: String = chunk
                                .iter()
                                .map(|&b| if (32..=126).contains(&b) { b as char } else { '.' })
                                .collect();
                            out.push_str(&format!("{: <48} | {}\n", hex_str.join(" "), ascii_str));
                        }
                    }
                }
                Err(e) => {
                    out.push_str(&format!("Error reading content: {}\n", e));
                }
            }
            Ok(out)
        }
    }

    pub fn run_command_any(&self, folder_path: Option<&str>, command_str: &str, working_dir: Option<&str>) -> Result<String, String> {
        let dir = if let Some(wd) = working_dir {
            self.resolve_path(folder_path, wd)
        } else if let Some(base) = folder_path {
            PathBuf::from(base)
        } else {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        };

        let mut cmd = Command::new("cmd.exe");
        cmd.args(["/c", command_str]);
        if dir.exists() {
            cmd.current_dir(&dir);
        }

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        let output = cmd.output().map_err(|e| format!("Failed to execute command: {}", e))?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let mut res = stdout;
        if !stderr.is_empty() {
            if !res.is_empty() {
                res.push_str("\n[STDERR]\n");
            }
            res.push_str(&stderr);
        }
        if res.trim().is_empty() {
            res = "Command executed successfully (no output).".to_string();
        }
        Ok(res)
    }

    pub fn run_ssh_command(
        &self,
        host: &str,
        command: &str,
        user: Option<&str>,
        port: Option<u16>,
        key_path: Option<&str>,
        working_dir: Option<&str>,
    ) -> Result<String, String> {
        let ssh_bin = if Path::new(r"C:\Windows\System32\OpenSSH\ssh.exe").exists() {
            r"C:\Windows\System32\OpenSSH\ssh.exe"
        } else {
            "ssh"
        };

        let mut cmd = Command::new(ssh_bin);
        // Non-interactive safe flags to prevent hanging on prompts
        cmd.args([
            "-o", "BatchMode=yes",
            "-o", "StrictHostKeyChecking=accept-new",
            "-o", "ConnectTimeout=15",
        ]);

        if let Some(p) = port {
            cmd.args(["-p", &p.to_string()]);
        }
        if let Some(k) = key_path {
            cmd.args(["-i", k]);
        }

        let destination = if let Some(u) = user {
            format!("{}@{}", u, host)
        } else {
            host.to_string()
        };
        cmd.arg(&destination);

        let final_cmd = if let Some(wd) = working_dir {
            format!("cd '{}' && {}", wd, command)
        } else {
            command.to_string()
        };
        cmd.arg(&final_cmd);

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        let output = cmd.output().map_err(|e| format!("Failed to execute SSH command: {}", e))?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let mut res = stdout;
        if !stderr.is_empty() {
            if !res.is_empty() {
                res.push_str("\n[STDERR]\n");
            }
            res.push_str(&stderr);
        }
        if res.trim().is_empty() {
            res = "SSH command executed successfully (no output).".to_string();
        }
        Ok(res)
    }

    pub fn read_file(&self, folder_path: &str, relative_path: &str) -> Result<String, String> {
        let root = Path::new(folder_path);
        let target = root.join(relative_path);
        if !target.exists() {
            return Err(format!("File does not exist: {}", relative_path));
        }
        fs::read_to_string(&target).map_err(|e| format!("Failed to read {}: {}", relative_path, e))
    }

    pub fn write_file(&self, folder_path: &str, relative_path: &str, content: &str) -> Result<(), String> {
        let root = Path::new(folder_path);
        let target = root.join(relative_path);
        if let Some(parent) = target.parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(&target, content).map_err(|e| format!("Failed to write {}: {}", relative_path, e))
    }

    #[allow(dead_code)]
    pub fn search_files(&self, folder_path: &str, query: &str) -> Vec<(String, usize, String)> {
        let files = self.scan_files(folder_path, 200);
        let mut results = Vec::new();
        let q_lower = query.to_lowercase();

        for file in files {
            if file.is_dir || file.size > 500_000 {
                continue;
            }
            if let Ok(content) = self.read_file(folder_path, &file.relative_path) {
                for (line_num, line) in content.lines().enumerate() {
                    if line.to_lowercase().contains(&q_lower) {
                        results.push((file.relative_path.clone(), line_num + 1, line.trim().to_string()));
                        if results.len() >= 50 {
                            return results;
                        }
                    }
                }
            }
        }
        results
    }

    pub fn search_code_any(&self, folder_path: Option<&str>, query: &str) -> Result<String, String> {
        let folder = folder_path.unwrap_or(".");
        let matches = self.search_files(folder, query);
        if matches.is_empty() {
            Ok(format!("No matches found for '{}' in '{}'", query, folder))
        } else {
            let mut out = format!("Found {} match(es) for '{}' in '{}':\n\n", matches.len(), query, folder);
            for (file, line, text) in matches {
                out.push_str(&format!("{}:{}: {}\n", file, line, text));
            }
            Ok(out)
        }
    }

    pub fn run_command(&self, folder_path: &str, command_str: &str) -> Result<String, String> {
        let mut cmd = Command::new("cmd.exe");
        cmd.args(["/c", command_str]);
        cmd.current_dir(folder_path);

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        let output = cmd.output().map_err(|e| format!("Failed to execute command: {}", e))?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let mut res = stdout;
        if !stderr.is_empty() {
            if !res.is_empty() {
                res.push_str("\n[STDERR]\n");
            }
            res.push_str(&stderr);
        }
        Ok(res)
    }

    pub fn load_claude_md(&self, folder_path: &str) -> Option<String> {
        let path = Path::new(folder_path).join("CLAUDE.md");
        if path.exists() {
            fs::read_to_string(path).ok()
        } else {
            None
        }
    }

    pub fn compile_project_any(&self, folder_path: Option<&str>, mode: crate::project::compiler::BuildMode) -> crate::project::compiler::CompileResult {
        let base_dir = if let Some(base) = folder_path {
            self.resolve_path(None, base)
        } else {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        };
        crate::project::compiler::ProjectCompiler::compile(&base_dir, mode)
    }

    pub fn batch_read_files_any(&self, folder_path: Option<&str>, paths: &[String]) -> Result<String, String> {
        let mut out = format!("Batch Read {} file(s):\n\n", paths.len());
        for p in paths {
            match self.read_file_any(folder_path, p, None, None) {
                Ok(content) => {
                    out.push_str(&format!("--- FILE: {} ---\n```\n{}\n```\n\n", p, content));
                }
                Err(err) => {
                    out.push_str(&format!("--- FILE: {} (ERROR: {}) ---\n\n", p, err));
                }
            }
        }
        Ok(out)
    }

    pub fn delete_file_any(&self, folder_path: Option<&str>, path_str: &str) -> Result<String, String> {
        let target = self.resolve_path(folder_path, path_str);
        if !target.exists() {
            return Err(format!("File does not exist: {}", target.display()));
        }
        fs::remove_file(&target).map_err(|e| format!("Failed to delete {}: {}", target.display(), e))?;
        Ok(format!("Successfully deleted file: {}", target.display()))
    }

    pub fn delete_directory_any(&self, folder_path: Option<&str>, path_str: &str) -> Result<String, String> {
        let target = self.resolve_path(folder_path, path_str);
        if !target.exists() {
            return Err(format!("Directory does not exist: {}", target.display()));
        }
        fs::remove_dir_all(&target).map_err(|e| format!("Failed to delete directory {}: {}", target.display(), e))?;
        Ok(format!("Successfully deleted directory: {}", target.display()))
    }

    pub fn move_or_rename_any(&self, folder_path: Option<&str>, from_str: &str, to_str: &str) -> Result<String, String> {
        let from = self.resolve_path(folder_path, from_str);
        let to = self.resolve_path(folder_path, to_str);
        if !from.exists() {
            return Err(format!("Source path does not exist: {}", from.display()));
        }
        if let Some(parent) = to.parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::rename(&from, &to).map_err(|e| format!("Failed to move {} to {}: {}", from.display(), to.display(), e))?;
        Ok(format!("Successfully moved {} -> {}", from.display(), to.display()))
    }

    pub fn copy_file_or_dir_any(&self, folder_path: Option<&str>, from_str: &str, to_str: &str) -> Result<String, String> {
        let from = self.resolve_path(folder_path, from_str);
        let to = self.resolve_path(folder_path, to_str);
        if !from.exists() {
            return Err(format!("Source path does not exist: {}", from.display()));
        }
        if let Some(parent) = to.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if from.is_dir() {
            let mut count = 0;
            for entry in walkdir::WalkDir::new(&from) {
                let entry = entry.map_err(|e| e.to_string())?;
                let rel = entry.path().strip_prefix(&from).map_err(|e| e.to_string())?;
                let dest = to.join(rel);
                if entry.file_type().is_dir() {
                    let _ = fs::create_dir_all(&dest);
                } else {
                    let _ = fs::copy(entry.path(), &dest);
                    count += 1;
                }
            }
            Ok(format!("Successfully copied directory {} -> {} ({} files)", from.display(), to.display(), count))
        } else {
            fs::copy(&from, &to).map_err(|e| format!("Failed to copy {} to {}: {}", from.display(), to.display(), e))?;
            Ok(format!("Successfully copied {} -> {}", from.display(), to.display()))
        }
    }

    pub fn find_files_any(&self, folder_path: Option<&str>, pattern: &str) -> Result<String, String> {
        let base_dir = if let Some(base) = folder_path {
            self.resolve_path(None, base)
        } else {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        };
        let pat_lower = pattern.to_lowercase();
        let mut matches = Vec::new();

        for entry in walkdir::WalkDir::new(&base_dir)
            .max_depth(8)
            .into_iter()
            .filter_entry(|e| {
                let n = e.file_name().to_string_lossy();
                !n.starts_with('.') && n != "node_modules" && n != "target" && n != "bin" && n != "obj"
            })
            .flatten()
        {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            let rel = entry.path().strip_prefix(&base_dir).map(|p| p.display().to_string()).unwrap_or_default();
            if name.contains(&pat_lower) || rel.to_lowercase().contains(&pat_lower) {
                let kind = if entry.file_type().is_dir() { "DIR" } else { "FILE" };
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                matches.push(format!("- [{}] {} ({} bytes)", kind, rel, size));
                if matches.len() >= 100 {
                    break;
                }
            }
        }

        if matches.is_empty() {
            Ok(format!("No files matching '{}' found in {}", pattern, base_dir.display()))
        } else {
            Ok(format!("Found {} matching file(s) for '{}':\n{}", matches.len(), pattern, matches.join("\n")))
        }
    }

    pub fn git_command_any(&self, folder_path: Option<&str>, args: &[&str]) -> Result<String, String> {
        let base_dir = if let Some(base) = folder_path {
            self.resolve_path(None, base)
        } else {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        };

        let mut cmd = Command::new("git");
        cmd.args(args);
        cmd.current_dir(&base_dir);

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }

        let output = cmd.output().map_err(|e| format!("Git execution failed: {}. Is git installed?", e))?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let mut res = stdout;
        if !stderr.is_empty() {
            if !res.is_empty() {
                res.push('\n');
            }
            res.push_str(&stderr);
        }
        Ok(res)
    }

    pub fn save_claude_md(&self, folder_path: &str, content: &str) -> Result<(), String> {
        let path = Path::new(folder_path).join("CLAUDE.md");
        fs::write(path, content).map_err(|e| e.to_string())
    }
}

pub fn inspect_any_path_standalone(path_str: &str) -> Option<ChatAttachment> {
    use base64::Engine;
    let expanded = crate::mcp::client::expand_vars(path_str.trim().trim_matches('"'));
    let target = Path::new(&expanded);
    if !target.exists() {
        return None;
    }

    let name = target.file_name().unwrap_or_default().to_string_lossy().to_string();
    let metadata = fs::metadata(target).ok()?;

    if metadata.is_dir() {
        let mut file_count = 0;
        let mut listing_summary = format!("📁 Directory: {}\n", target.display());
        for entry in WalkDir::new(target)
            .max_depth(8)
            .into_iter()
            .filter_entry(|e| {
                let n = e.file_name().to_string_lossy();
                !n.starts_with('.')
                    && n != "node_modules"
                    && n != "target"
                    && n != "dist"
                    && n != "build"
                    && n != ".git"
                    && n != ".cargo"
                    && n != "bin"
                    && n != "obj"
            })
            .flatten()
        {
            if entry.file_type().is_file() {
                file_count += 1;
                if file_count <= 80 {
                    let sz = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    let rel = entry
                        .path()
                        .strip_prefix(target)
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|_| entry.path().display().to_string());
                    listing_summary.push_str(&format!("  - {} ({} bytes)\n", rel, sz));
                }
            }
        }
        if file_count > 80 {
            listing_summary.push_str(&format!("  ... and {} more files\n", file_count - 80));
        }

        Some(ChatAttachment {
            id: Uuid::new_v4().to_string(),
            name: if name.is_empty() {
                target.display().to_string()
            } else {
                name
            },
            kind: AttachmentKind::Directory {
                path: target.display().to_string(),
                dir_name: target.display().to_string(),
                file_count,
                listing_summary,
            },
        })
    } else {
        let ext = target.extension().unwrap_or_default().to_string_lossy().to_lowercase();
        let size = metadata.len();

        if ["png", "jpg", "jpeg", "webp", "gif", "bmp"].contains(&ext.as_str()) {
            if let Ok(bytes) = fs::read(target) {
                if let Ok(dyn_img) = image::load_from_memory(&bytes) {
                    let (w, h) = (dyn_img.width(), dyn_img.height());
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                    let mime = match ext.as_str() {
                        "jpg" | "jpeg" => "image/jpeg",
                        "webp" => "image/webp",
                        "gif" => "image/gif",
                        "bmp" => "image/bmp",
                        _ => "image/png",
                    };
                    return Some(ChatAttachment {
                        id: Uuid::new_v4().to_string(),
                        name: format!("🖼 {}", name),
                        kind: AttachmentKind::Image {
                            mime_type: mime.to_string(),
                            base64_data: b64,
                            width: w,
                            height: h,
                        },
                    });
                }
            }
        }

        match fs::read(target) {
            Ok(bytes) => {
                if let Ok(text) = std::str::from_utf8(&bytes) {
                    let total_lines = text.lines().count();
                    let preview = if total_lines > 60 {
                        let lines: Vec<&str> = text.lines().take(50).collect();
                        format!(
                            "{}\n... [Token-efficient preview: lines 1-50 of {} shown. Use read_file to inspect full content without token bloat] ...",
                            lines.join("\n"),
                            total_lines
                        )
                    } else {
                        text.to_string()
                    };
                    Some(ChatAttachment {
                        id: Uuid::new_v4().to_string(),
                        name: format!("📄 {}", name),
                        kind: AttachmentKind::File {
                            path: target.display().to_string(),
                            filename: name,
                            size_bytes: size,
                            is_text: true,
                            content_preview: preview,
                        },
                    })
                } else {
                    let mut hex_preview = String::new();
                    for chunk in bytes.chunks(16).take(4) {
                        let hex_str: Vec<String> = chunk.iter().map(|b| format!("{:02X}", b)).collect();
                        hex_preview.push_str(&hex_str.join(" "));
                        hex_preview.push('\n');
                    }
                    Some(ChatAttachment {
                        id: Uuid::new_v4().to_string(),
                        name: format!("📦 {}", name),
                        kind: AttachmentKind::File {
                            path: target.display().to_string(),
                            filename: name,
                            size_bytes: size,
                            is_text: false,
                            content_preview: format!("Binary data ({} bytes)\nHex sample:\n{}", size, hex_preview),
                        },
                    })
                }
            }
            Err(_) => None,
        }
    }
}
