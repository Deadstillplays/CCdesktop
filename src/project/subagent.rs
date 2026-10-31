use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub struct CodebaseSubagent;

impl CodebaseSubagent {
    pub fn run_subagent(
        base_dir: &Path,
        role: &str,
        task: &str,
        detail_level: Option<&str>,
    ) -> String {
        let role_name = if role.trim().is_empty() {
            "Codebase Inspector"
        } else {
            role.trim()
        };

        let mut report = String::new();
        report.push_str(&format!(
            "# 🤖 Subagent Report: {}\n\n",
            role_name
        ));
        report.push_str(&format!("**Target Directory**: `{}`\n", base_dir.display()));
        report.push_str(&format!("**Objective**: {}\n\n", task));

        if !base_dir.exists() {
            report.push_str(&format!("❌ Error: Directory `{}` does not exist.\n", base_dir.display()));
            return report;
        }

        // 1. Project Manifest & Environment Detection
        report.push_str("## 1. Project Environment & Manifest Analysis\n");
        let manifest_info = Self::detect_project_type(base_dir);
        report.push_str(&manifest_info);
        report.push_str("\n\n");

        // 2. Directory Structure & File Hierarchy
        report.push_str("## 2. Codebase Structure & File Map\n");
        let (tree_str, code_files, lang_counts, total_lines) = Self::scan_hierarchy(base_dir);
        report.push_str(&tree_str);
        report.push_str("\n");

        // Language breakdown
        report.push_str("**Language & File Breakdown**:\n");
        let mut sorted_langs: Vec<(&String, &usize)> = lang_counts.iter().collect();
        sorted_langs.sort_by(|a, b| b.1.cmp(a.1));
        for (lang, count) in sorted_langs {
            report.push_str(&format!("- **{}**: {} file(s)\n", lang, count));
        }
        report.push_str(&format!("- **Total Analyzed Lines of Code**: ~{} lines across {} source files\n\n", total_lines, code_files.len()));

        // 3. High-Level Component & Symbol Blueprint
        report.push_str("## 3. Component & Symbol Blueprint\n");
        let deep = detail_level.map(|d| d.eq_ignore_ascii_case("deep_dive") || d.eq_ignore_ascii_case("full_structure")).unwrap_or(true);
        let symbols_str = Self::extract_component_symbols(base_dir, &code_files, deep);
        report.push_str(&symbols_str);
        report.push_str("\n\n");

        // 4. Task-Specific Findings & Relevant Code Elements
        report.push_str("## 4. Task-Specific Findings & Code Elements\n");
        let findings = Self::search_task_elements(base_dir, &code_files, task);
        report.push_str(&findings);
        report.push_str("\n\n");

        // 5. Synthesis & Guidance for Main Claude Agent
        report.push_str("## 5. Summary & Recommended Focus for Main Claude\n");
        report.push_str("- The full codebase structure has been mapped above. Main Claude does NOT need to re-read all files.\n");
        report.push_str("- If any specific function implementation or section requires deeper modification, use `read_file` on the target file directly.\n");
        report.push_str("- Use `edit_file` with exact target strings to apply modifications cleanly.\n");

        report
    }

    fn detect_project_type(base: &Path) -> String {
        let mut detected = Vec::new();

        let cargo_toml = base.join("Cargo.toml");
        if cargo_toml.exists() {
            if let Ok(content) = fs::read_to_string(&cargo_toml) {
                let mut name = "Rust Crate".to_string();
                let mut deps = Vec::new();
                let mut in_deps = false;

                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("name =") {
                        name = trimmed.replace("name =", "").replace('\"', "").trim().to_string();
                    } else if trimmed.starts_with("[dependencies]") {
                        in_deps = true;
                    } else if trimmed.starts_with('[') {
                        in_deps = false;
                    } else if in_deps && trimmed.contains('=') {
                        let dep_name = trimmed.split('=').next().unwrap_or("").trim();
                        if !dep_name.is_empty() {
                            deps.push(dep_name.to_string());
                        }
                    }
                }
                detected.push(format!(
                    "- **Rust Project**: `{}`\n  - Manifest: `Cargo.toml`\n  - Core Dependencies: {}",
                    name,
                    if deps.is_empty() { "None listed".to_string() } else { deps.join(", ") }
                ));
            }
        }

        let pkg_json = base.join("package.json");
        if pkg_json.exists() {
            if let Ok(content) = fs::read_to_string(&pkg_json) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                    let name = val.get("name").and_then(|n| n.as_str()).unwrap_or("Node Package");
                    let mut dep_keys = Vec::new();
                    if let Some(deps) = val.get("dependencies").and_then(|d| d.as_object()) {
                        dep_keys.extend(deps.keys().cloned());
                    }
                    detected.push(format!(
                        "- **Node.js / TypeScript Project**: `{}`\n  - Manifest: `package.json`\n  - Dependencies: {}",
                        name,
                        if dep_keys.is_empty() { "None listed".to_string() } else { dep_keys.join(", ") }
                    ));
                }
            }
        }

        let pyproject = base.join("pyproject.toml");
        let req_txt = base.join("requirements.txt");
        if pyproject.exists() || req_txt.exists() {
            detected.push("- **Python Project**: (Detected pyproject.toml or requirements.txt)".to_string());
        }

        let roblox_proj = base.join("default.project.json");
        if roblox_proj.exists() {
            detected.push("- **Roblox Studio Rojo Project**: `default.project.json` detected".to_string());
        }

        if detected.is_empty() {
            "- Generic or multi-language repository".to_string()
        } else {
            detected.join("\n")
        }
    }

    fn scan_hierarchy(
        base: &Path,
    ) -> (String, Vec<PathBuf>, HashMap<String, usize>, usize) {
        let mut code_files = Vec::new();
        let mut lang_counts: HashMap<String, usize> = HashMap::new();
        let mut total_lines = 0;
        let mut tree_lines = Vec::new();

        tree_lines.push(format!("```\n{}", base.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "root".to_string())));

        for entry in walkdir::WalkDir::new(base)
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
            let depth = entry.depth();
            if depth == 0 {
                continue;
            }

            let path = entry.path();
            let name = entry.file_name().to_string_lossy();
            let indent = "  ".repeat(depth);

            if entry.file_type().is_dir() {
                tree_lines.push(format!("{}📁 {}/", indent, name));
            } else {
                let ext = path.extension().unwrap_or_default().to_string_lossy().to_lowercase();
                let lang = match ext.as_str() {
                    "rs" => "Rust",
                    "ts" | "tsx" => "TypeScript",
                    "js" | "jsx" => "JavaScript",
                    "py" => "Python",
                    "lua" | "luau" => "Lua / Luau",
                    "toml" => "TOML",
                    "json" => "JSON",
                    "md" => "Markdown",
                    "html" | "css" => "HTML/CSS",
                    "go" => "Go",
                    "c" | "cpp" | "h" | "hpp" => "C/C++",
                    _ => "Other",
                };

                *lang_counts.entry(lang.to_string()).or_insert(0) += 1;

                let line_cnt = if let Ok(c) = fs::read_to_string(path) {
                    let cnt = c.lines().count();
                    total_lines += cnt;
                    cnt
                } else {
                    0
                };

                tree_lines.push(format!("{}📄 {} ({} lines)", indent, name, line_cnt));

                if lang != "Other" && lang != "Markdown" {
                    code_files.push(path.to_path_buf());
                }
            }

            if tree_lines.len() >= 80 {
                tree_lines.push("  ... [Remaining tree collapsed for brevity]".to_string());
                break;
            }
        }

        tree_lines.push("```".to_string());
        (tree_lines.join("\n"), code_files, lang_counts, total_lines)
    }

    fn extract_component_symbols(
        base: &Path,
        files: &[PathBuf],
        deep: bool,
    ) -> String {
        let mut out = String::new();
        let max_files = if deep { 30 } else { 15 };

        for file in files.iter().take(max_files) {
            let rel = file.strip_prefix(base).unwrap_or(file);
            let ext = file.extension().unwrap_or_default().to_string_lossy().to_lowercase();

            if let Ok(content) = fs::read_to_string(file) {
                let lines: Vec<&str> = content.lines().collect();
                let line_count = lines.len();

                let mut signatures = Vec::new();
                for l in &lines {
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
                                || trimmed.starts_with("pub trait ")
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
                        signatures.push(clean_sig);
                        if signatures.len() >= 8 {
                            break;
                        }
                    }
                }

                out.push_str(&format!("### `{}` ({} lines)\n", rel.display(), line_count));
                if signatures.is_empty() {
                    out.push_str("- *Utility or configuration file*\n\n");
                } else {
                    for sig in signatures {
                        out.push_str(&format!("- `{}`\n", sig));
                    }
                    out.push('\n');
                }
            }
        }

        if files.len() > max_files {
            out.push_str(&format!("*(... and {} additional files mapped)*\n", files.len() - max_files));
        }

        out
    }

    fn search_task_elements(
        base: &Path,
        files: &[PathBuf],
        task: &str,
    ) -> String {
        let stop_words = ["the", "and", "for", "with", "that", "this", "from", "how", "what", "where", "inspect", "check", "find", "codebase", "structure", "stuff"];
        let raw_keywords: Vec<&str> = task
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|w| w.len() >= 3 && !stop_words.contains(&w.to_lowercase().as_str()))
            .collect();

        if raw_keywords.is_empty() {
            return "No specific technical keywords detected in task; full codebase map provided in sections 2 & 3 above.".to_string();
        }

        let mut matches_by_file: HashMap<String, Vec<(usize, String)>> = HashMap::new();

        for file in files {
            if let Ok(content) = fs::read_to_string(file) {
                let rel = file.strip_prefix(base).unwrap_or(file).display().to_string();
                for (idx, line) in content.lines().enumerate() {
                    let l_lower = line.to_lowercase();
                    for kw in &raw_keywords {
                        if l_lower.contains(&kw.to_lowercase()) {
                            let entry = matches_by_file.entry(rel.clone()).or_default();
                            if entry.len() < 4 {
                                entry.push((idx + 1, line.trim().to_string()));
                            }
                            break;
                        }
                    }
                }
            }
        }

        if matches_by_file.is_empty() {
            format!("No direct keyword matches found for {:?}. Refer to the component blueprint above.", raw_keywords)
        } else {
            let mut out = format!("Found relevant code elements matching keywords {:?}:\n\n", raw_keywords);
            for (f, occurrences) in matches_by_file.iter().take(12) {
                out.push_str(&format!("- **`{}`**:\n", f));
                for (line_num, text) in occurrences {
                    let truncated = if text.len() > 120 {
                        format!("{}...", &text[..117])
                    } else {
                        text.clone()
                    };
                    out.push_str(&format!("  - Line {}: `{}`\n", line_num, truncated));
                }
                out.push('\n');
            }
            out
        }
    }
}
