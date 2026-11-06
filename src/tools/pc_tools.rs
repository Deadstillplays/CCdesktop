use crate::ui::clipboard::ClipboardHelper;
use chrono::Timelike;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

pub fn execute_powershell(command: &str, working_dir: Option<&str>, _timeout_secs: u64) -> (String, bool) {
    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", command]);

    if let Some(wd) = working_dir {
        let expanded = crate::mcp::client::expand_vars(wd);
        let p = Path::new(&expanded);
        if p.exists() {
            cmd.current_dir(p);
        }
    }

    match cmd.output() {
        Ok(output) => {
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
                res = "Command executed with no output.".to_string();
            }
            let is_err = !output.status.success();
            (res, is_err)
        }
        Err(e) => (format!("Failed to spawn PowerShell: {}", e), true),
    }
}

pub fn execute_cmd(command: &str, working_dir: Option<&str>) -> (String, bool) {
    let mut cmd = Command::new("cmd");
    cmd.args(["/c", command]);

    if let Some(wd) = working_dir {
        let expanded = crate::mcp::client::expand_vars(wd);
        let p = Path::new(&expanded);
        if p.exists() {
            cmd.current_dir(p);
        }
    }

    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let mut res = String::new();
            if !stdout.is_empty() {
                res.push_str(&stdout);
            }
            if !stderr.is_empty() {
                if !res.is_empty() {
                    res.push_str("\n--- Error Output ---\n");
                }
                res.push_str(&stderr);
            }
            if res.is_empty() {
                res = "Command executed with no output.".to_string();
            }
            (res, !output.status.success())
        }
        Err(e) => (format!("Failed to spawn CMD: {}", e), true),
    }
}

pub fn get_clipboard_text() -> (String, bool) {
    match ClipboardHelper::get_text() {
        Some(t) if !t.is_empty() => (t, false),
        Some(_) => ("Clipboard is currently empty.".to_string(), false),
        None => ("Clipboard is empty or contains non-text data (e.g. image/file).".to_string(), false),
    }
}

pub fn set_clipboard_text(text: &str) -> (String, bool) {
    ClipboardHelper::set_text(text);
    (format!("Copied {} characters to Windows clipboard.", text.len()), false)
}

pub fn capture_screen(save_path: Option<&str>) -> (String, bool) {
    let target_file: PathBuf = if let Some(p) = save_path {
        let exp = crate::mcp::client::expand_vars(p);
        PathBuf::from(exp)
    } else {
        let temp = std::env::temp_dir();
        let ts = chrono::Utc::now().timestamp_millis();
        temp.join(format!("screenshot_{}.png", ts))
    };

    if let Some(parent) = target_file.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let ps_code = format!(
        "[Reflection.Assembly]::LoadWithPartialName('System.Drawing') | Out-Null; \
         [Reflection.Assembly]::LoadWithPartialName('System.Windows.Forms') | Out-Null; \
         $bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds; \
         $bmp = New-Object System.Drawing.Bitmap $bounds.Width, $bounds.Height; \
         $g = [System.Drawing.Graphics]::FromImage($bmp); \
         $g.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size); \
         $bmp.Save('{}', [System.Drawing.Imaging.ImageFormat]::Png); \
         $g.Dispose(); \
         $bmp.Dispose(); \
         Write-Output \"Resolution: $($bounds.Width)x$($bounds.Height)\"",
        target_file.display().to_string().replace('\\', "\\\\")
    );

    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &ps_code]);

    match cmd.output() {
        Ok(out) => {
            if target_file.exists() {
                let size_bytes = fs::metadata(&target_file).map(|m| m.len()).unwrap_or(0);
                let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
                (
                    format!(
                        "Screenshot successfully saved to: {}\nFile size: {:.1} KB\n{}",
                        target_file.display(),
                        size_bytes as f64 / 1024.0,
                        stdout
                    ),
                    false,
                )
            } else {
                let err = String::from_utf8_lossy(&out.stderr);
                (format!("Screenshot capture failed: {}", err), true)
            }
        }
        Err(e) => (format!("Failed to execute screenshot command: {}", e), true),
    }
}

pub fn list_running_processes(filter: Option<&str>) -> (String, bool) {
    let ps_code = if let Some(f) = filter {
        format!(
            "Get-Process | Where-Object {{ $_.ProcessName -like '*{}*' -or $_.MainWindowTitle -like '*{}*' }} | \
             Select-Object Id, ProcessName, @{{Name='MemoryMB';Expression={{[math]::Round($_.WorkingSet64 / 1MB, 1)}}}}, MainWindowTitle | \
             Format-Table -AutoSize | Out-String -Width 160",
            f, f
        )
    } else {
        "Get-Process | Sort-Object WorkingSet64 -Descending | Select-Object -First 40 Id, ProcessName, @{Name='MemoryMB';Expression={[math]::Round($_.WorkingSet64 / 1MB, 1)}}, MainWindowTitle | \
         Format-Table -AutoSize | Out-String -Width 160"
            .to_string()
    };

    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-Command", &ps_code]);

    match cmd.output() {
        Ok(out) => {
            let res = String::from_utf8_lossy(&out.stdout).to_string();
            if res.trim().is_empty() {
                ("No matching processes found.".to_string(), false)
            } else {
                (res, false)
            }
        }
        Err(e) => (format!("Failed to query processes: {}", e), true),
    }
}

pub fn kill_process(pid: Option<u32>, name: Option<&str>) -> (String, bool) {
    if pid.is_none() && name.is_none() {
        return ("Error: Provide either 'pid' or 'name' of the process to terminate.".to_string(), true);
    }

    let mut cmd = Command::new("taskkill");
    cmd.arg("/F");

    if let Some(p) = pid {
        cmd.args(["/PID", &p.to_string()]);
    } else if let Some(n) = name {
        let im_name = if n.ends_with(".exe") { n.to_string() } else { format!("{}.exe", n) };
        cmd.args(["/IM", &im_name]);
    }

    match cmd.output() {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            let combined = format!("{}{}", stdout, stderr);
            (combined, !out.status.success())
        }
        Err(e) => (format!("Failed to execute taskkill: {}", e), true),
    }
}

pub fn list_open_windows() -> (String, bool) {
    let ps_code = "Get-Process | Where-Object { $_.MainWindowTitle.Length -gt 0 } | \
                   Select-Object Id, ProcessName, MainWindowTitle | \
                   Format-Table -AutoSize | Out-String -Width 160";

    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-Command", ps_code]);

    match cmd.output() {
        Ok(out) => {
            let res = String::from_utf8_lossy(&out.stdout).to_string();
            (res, false)
        }
        Err(e) => (format!("Failed to list open windows: {}", e), true),
    }
}

pub fn get_system_resources() -> (String, bool) {
    let ps_code = "$cpu = (Get-CimInstance Win32_Processor | Measure-Object -Property NumberOfLogicalProcessors -Sum).Sum; \
                   $os = Get-CimInstance Win32_OperatingSystem; \
                   $totalRam = [math]::Round($os.TotalVisibleMemorySize / 1MB, 2); \
                   $freeRam = [math]::Round($os.FreePhysicalMemory / 1MB, 2); \
                   $usedRam = [math]::Round($totalRam - $freeRam, 2); \
                   $drives = Get-PSDrive -PSProvider FileSystem | Select-Object Name, @{N='UsedGB';E={[math]::Round($_.Used/1GB,1)}}, @{N='FreeGB';E={[math]::Round($_.Free/1GB,1)}}, @{N='TotalGB';E={[math]::Round(($_.Used+$_.Free)/1GB,1)}} | Out-String -Width 100; \
                   Write-Output \"OS: $($os.Caption) (Build $($os.BuildNumber))\"; \
                   Write-Output \"Logical CPU Cores: $cpu\"; \
                   Write-Output \"RAM: $usedRam GB used / $totalRam GB total ($freeRam GB free)\"; \
                   Write-Output \"Storage Drives:\"; \
                   Write-Output $drives";

    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-Command", ps_code]);

    match cmd.output() {
        Ok(out) => {
            let res = String::from_utf8_lossy(&out.stdout).to_string();
            (res, false)
        }
        Err(e) => (format!("Failed to get system resources: {}", e), true),
    }
}

pub fn read_registry_key(path: &str, value_name: Option<&str>) -> (String, bool) {
    let mut cmd = Command::new("reg");
    cmd.args(["query", path]);
    if let Some(v) = value_name {
        cmd.args(["/v", v]);
    }

    match cmd.output() {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            if out.status.success() {
                (stdout, false)
            } else {
                (format!("Registry query failed:\n{}", stderr), true)
            }
        }
        Err(e) => (format!("Failed to run reg query: {}", e), true),
    }
}

pub fn write_registry_key(path: &str, value_name: &str, value_type: &str, value_data: &str) -> (String, bool) {
    let mut cmd = Command::new("reg");
    cmd.args(["add", path, "/v", value_name, "/t", value_type, "/d", value_data, "/f"]);

    match cmd.output() {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            if out.status.success() {
                (format!("Registry value added successfully.\n{}", stdout), false)
            } else {
                (format!("Registry add failed:\n{}", stderr), true)
            }
        }
        Err(e) => (format!("Failed to run reg add: {}", e), true),
    }
}

pub fn check_port_status(port: u16) -> (String, bool) {
    let ps_code = format!(
        "$c = Get-NetTCPConnection -LocalPort {} -ErrorAction SilentlyContinue; \
         if ($c) {{ \
             $pids = $c.OwningProcess | Select-Object -Unique; \
             foreach ($p in $pids) {{ \
                 $proc = Get-Process -Id $p -ErrorAction SilentlyContinue; \
                 Write-Output \"Port {} is IN USE by PID $p ($($proc.ProcessName))\"; \
             }} \
         }} else {{ \
             Write-Output \"Port {} is FREE and open.\"; \
         }}",
        port, port, port
    );

    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-Command", &ps_code]);

    match cmd.output() {
        Ok(out) => {
            let res = String::from_utf8_lossy(&out.stdout).trim().to_string();
            (res, false)
        }
        Err(e) => (format!("Failed to check port: {}", e), true),
    }
}

pub fn kill_port_owner(port: u16) -> (String, bool) {
    let ps_code = format!(
        "$c = Get-NetTCPConnection -LocalPort {} -ErrorAction SilentlyContinue; \
         if ($c) {{ \
             $pids = $c.OwningProcess | Select-Object -Unique; \
             foreach ($p in $pids) {{ \
                 $proc = Get-Process -Id $p -ErrorAction SilentlyContinue; \
                 Stop-Process -Id $p -Force; \
                 Write-Output \"Successfully terminated PID $p ($($proc.ProcessName)) holding port {}.\"; \
             }} \
         }} else {{ \
             Write-Output \"No active process was listening on port {}.\"; \
         }}",
        port, port, port
    );

    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-Command", &ps_code]);

    match cmd.output() {
        Ok(out) => {
            let res = String::from_utf8_lossy(&out.stdout).trim().to_string();
            (res, false)
        }
        Err(e) => (format!("Failed to kill port owner: {}", e), true),
    }
}

pub async fn web_search_query(query: &str, max_results: usize) -> (String, bool) {
    let clean = query.trim();
    if clean.is_empty() {
        return ("Search query cannot be empty.".to_string(), true);
    }

    let url = format!("https://html.duckduckgo.com/html/?q={}", urlencoding(clean));
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()
        .unwrap_or_default();

    match client.get(&url).send().await {
        Ok(resp) => {
            let html = resp.text().await.unwrap_or_default();
            let mut results = Vec::new();

            // Parse result links and snippets
            let mut cursor = 0;
            while let Some(start) = html[cursor..].find("<a class=\"result__snippet\"") {
                let abs_start = cursor + start;
                // Find href before this or title
                let title_start = html[..abs_start].rfind("class=\"result__a\"");
                let title = if let Some(ts) = title_start {
                    if let Some(tag_end) = html[ts..abs_start].find('>') {
                        let text_start = ts + tag_end + 1;
                        if let Some(tag_close) = html[text_start..abs_start].find("</a>") {
                            strip_html(&html[text_start..text_start + tag_close])
                        } else {
                            "Result".to_string()
                        }
                    } else {
                        "Result".to_string()
                    }
                } else {
                    "Result".to_string()
                };

                let snippet_tag_end = html[abs_start..].find('>').unwrap_or(0);
                let content_start = abs_start + snippet_tag_end + 1;
                let snippet = if let Some(end) = html[content_start..].find("</a>") {
                    strip_html(&html[content_start..content_start + end])
                } else {
                    String::new()
                };

                if !snippet.is_empty() {
                    results.push(format!("{}. {}\n   {}", results.len() + 1, title.trim(), snippet.trim()));
                }

                if results.len() >= max_results {
                    break;
                }
                cursor = content_start + 10;
            }

            if results.is_empty() {
                // Fallback to text preview of search
                let clean_text = strip_html(&html);
                let lines: Vec<&str> = clean_text.lines().map(|l| l.trim()).filter(|l| l.len() > 20).take(6).collect();
                if !lines.is_empty() {
                    (format!("Web search results for '{}':\n\n{}", query, lines.join("\n\n")), false)
                } else {
                    (format!("No search results found for '{}'.", query), false)
                }
            } else {
                (format!("Web search results for '{}':\n\n{}", query, results.join("\n\n")), false)
            }
        }
        Err(e) => (format!("Web search network error: {}", e), true),
    }
}

fn urlencoding(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
            out.push(b as char);
        } else if b == b' ' {
            out.push('+');
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

fn strip_html(input: &str) -> String {
    let mut result = String::new();
    let mut inside = false;
    for c in input.chars() {
        if c == '<' {
            inside = true;
        } else if c == '>' {
            inside = false;
        } else if !inside {
            result.push(c);
        }
    }
    result.replace("&amp;", "&").replace("&quot;", "\"").replace("&#39;", "'").replace("&lt;", "<").replace("&gt;", ">")
}

pub fn file_search_glob(search_root: &str, pattern: &str, max_results: usize) -> (String, bool) {
    let root_exp = crate::mcp::client::expand_vars(search_root);
    let root = Path::new(&root_exp);
    if !root.exists() {
        return (format!("Directory does not exist: {}", root_exp), true);
    }

    let p_clean = pattern.trim().to_lowercase();
    let is_ext_match = p_clean.starts_with("*.") && p_clean.len() > 2;
    let target_ext = if is_ext_match { &p_clean[2..] } else { "" };

    let mut matches = Vec::new();
    for entry in walkdir::WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file() {
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
            let matched = if is_ext_match {
                path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()) == Some(target_ext.to_string())
            } else {
                file_name.contains(&p_clean) || path.to_string_lossy().to_lowercase().contains(&p_clean)
            };

            if matched {
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                matches.push(format!("- {} ({:.1} KB)", path.display(), size as f64 / 1024.0));
                if matches.len() >= max_results {
                    break;
                }
            }
        }
    }

    if matches.is_empty() {
        (format!("No files matching '{}' found under '{}'.", pattern, root_exp), false)
    } else {
        (format!("Found {} matching file(s) for '{}':\n\n{}", matches.len(), pattern, matches.join("\n")), false)
    }
}

pub fn get_env_var(name: &str) -> (String, bool) {
    match std::env::var(name) {
        Ok(v) => (format!("{}={}", name, v), false),
        Err(_) => (format!("Environment variable '{}' is not set.", name), false),
    }
}

pub fn set_env_var(name: &str, value: &str) -> (String, bool) {
    std::env::set_var(name, value);
    (format!("Set environment variable: {}={}", name, value), false)
}

pub fn auto_install_mcp_package(package: &str, server_name: &str, target_type: &str, mcp_config_path: &str) -> (String, bool) {
    let s_name = if server_name.trim().is_empty() {
        package.split('/').last().unwrap_or("custom_mcp").replace('@', "").replace('-', "_")
    } else {
        server_name.trim().to_string()
    };

    let (install_cmd, run_cmd, run_args) = match target_type.to_lowercase().as_str() {
        "python" | "pip" => (
            format!("pip install {}", package),
            "python".to_string(),
            vec!["-m".to_string(), package.to_string()],
        ),
        _ => (
            format!("npm install -g {}", package),
            "npx".to_string(),
            vec!["-y".to_string(), package.to_string()],
        ),
    };

    // 1. Run installer
    let mut sh = Command::new("cmd");
    sh.args(["/c", &install_cmd]);
    let out = match sh.output() {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let stderr = String::from_utf8_lossy(&o.stderr);
            format!("{}\n{}", stdout, stderr)
        }
        Err(e) => format!("Install error: {}", e),
    };

    // 2. Persist into claude_desktop_config.json
    let cfg_path = crate::mcp::client::expand_vars(mcp_config_path);
    let mut config: crate::mcp::McpConfigFile = if Path::new(&cfg_path).exists() {
        fs::read_to_string(&cfg_path)
            .ok()
            .and_then(|c| serde_json::from_str(&c).ok())
            .unwrap_or_default()
    } else {
        crate::mcp::McpConfigFile::default()
    };

    config.mcp_servers.insert(
        s_name.clone(),
        crate::mcp::McpServerConfig {
            command: run_cmd,
            args: run_args,
            env: std::collections::HashMap::new(),
            url: None,
        },
    );

    if let Ok(serialized) = serde_json::to_string_pretty(&config) {
        if let Some(parent) = Path::new(&cfg_path).parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&cfg_path, serialized);
    }

    (
        format!(
            "Installed MCP package '{}' as server '{}'.\nInstaller log:\n{}\n\nRegistered in: {}",
            package, s_name, out.trim(), cfg_path
        ),
        false,
    )
}

/// Run PowerShell command with admin elevation via Start-Process -Verb RunAs.
/// Captures output to a temporary file.
pub fn execute_powershell_admin(command: &str) -> (String, bool) {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let out_file = std::env::temp_dir().join(format!("admin_out_{}.txt", ts));
    let err_file = std::env::temp_dir().join(format!("admin_err_{}.txt", ts));

    let out_path = out_file.display().to_string().replace('\\', "\\\\");
    let err_path = err_file.display().to_string().replace('\\', "\\\\");

    // Script runs elevated, captures output and error streams
    let wrapped = format!(
        "try {{ $o = ({}) 2>&1 | Out-String; [IO.File]::WriteAllText('{}', $o) }} catch {{ [IO.File]::WriteAllText('{}', $_.ToString()) }}",
        command, out_path, err_path
    );
    let encoded = {
        let utf16: Vec<u16> = wrapped.encode_utf16().collect();
        let bytes: Vec<u8> = utf16.iter().flat_map(|c| c.to_le_bytes()).collect();
        base64_encode(&bytes)
    };

    let ps_launcher = format!(
        "Start-Process powershell -Verb RunAs -Wait -ArgumentList '-NoProfile -NonInteractive -EncodedCommand {}'",
        encoded
    );

    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &ps_launcher]);

    match cmd.output() {
        Ok(_) => {
            let stdout = fs::read_to_string(&out_file).unwrap_or_default();
            let stderr = fs::read_to_string(&err_file).unwrap_or_default();
            let _ = fs::remove_file(&out_file);
            let _ = fs::remove_file(&err_file);

            if stdout.trim().is_empty() && stderr.trim().is_empty() {
                ("Admin command executed (UAC prompt completed). No stdout captured — command may have had no output or UAC was dismissed.".to_string(), false)
            } else if !stderr.trim().is_empty() {
                (format!("Admin Error:\n{}", stderr.trim()), true)
            } else {
                (stdout, false)
            }
        }
        Err(e) => (format!("Failed to spawn elevated PowerShell: {}", e), true),
    }
}

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    let mut i = 0;
    while i < data.len() {
        let b0 = data[i] as u32;
        let b1 = if i + 1 < data.len() { data[i + 1] as u32 } else { 0 };
        let b2 = if i + 2 < data.len() { data[i + 2] as u32 } else { 0 };

        result.push(CHARS[((b0 >> 2) & 0x3F) as usize] as char);
        result.push(CHARS[(((b0 & 0x3) << 4) | (b1 >> 4)) as usize] as char);
        result.push(if i + 1 < data.len() { CHARS[(((b1 & 0xF) << 2) | (b2 >> 6)) as usize] as char } else { '=' });
        result.push(if i + 2 < data.len() { CHARS[(b2 & 0x3F) as usize] as char } else { '=' });
        i += 3;
    }
    result
}

pub fn get_admin_status() -> (String, bool) {
    let ps = "if (([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { Write-Output 'ELEVATED: Yes - Running as Administrator' } else { Write-Output 'ELEVATED: No - Standard User (not elevated)' }; $id = [Security.Principal.WindowsIdentity]::GetCurrent(); Write-Output \"User: $($id.Name)\"; Write-Output \"SID: $($id.User.Value)\"";
    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-Command", ps]);
    match cmd.output() {
        Ok(out) => {
            let res = String::from_utf8_lossy(&out.stdout).to_string();
            (res, false)
        }
        Err(e) => (format!("Failed to check admin status: {}", e), true),
    }
}

pub fn get_network_info() -> (String, bool) {
    let ps = "Get-NetAdapter | Where-Object { $_.Status -eq 'Up' } | ForEach-Object { $a = $_; $ip = Get-NetIPAddress -InterfaceIndex $a.ifIndex -ErrorAction SilentlyContinue | Where-Object { $_.AddressFamily -eq 'IPv4' }; Write-Output \"Adapter: $($a.Name) | Status: $($a.Status) | Speed: $([math]::Round($a.LinkSpeed/1MB,0)) Mbps\"; foreach ($i in $ip) { Write-Output \" IP: $($i.IPAddress)/$($i.PrefixLength)\" } }; Write-Output ''; $dns = Get-DnsClientServerAddress -AddressFamily IPv4 | Where-Object { $_.ServerAddresses } | Select-Object -First 1; Write-Output \"DNS Servers: $($dns.ServerAddresses -join ', ')\"";
    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-Command", ps]);
    match cmd.output() {
        Ok(out) => (String::from_utf8_lossy(&out.stdout).to_string(), false),
        Err(e) => (format!("Failed to get network info: {}", e), true),
    }
}

pub fn get_disk_usage(path: Option<&str>) -> (String, bool) {
    let ps = if let Some(p) = path {
        format!(
            "$drives = Get-PSDrive -PSProvider FileSystem | Select-Object Name, @{{N='UsedGB';E={{[math]::Round($_.Used/1GB,2)}}}}, @{{N='FreeGB';E={{[math]::Round($_.Free/1GB,2)}}}}, @{{N='TotalGB';E={{[math]::Round(($_.Used+$_.Free)/1GB,2)}}}} | Format-Table | Out-String; Write-Output $drives; $f = Get-Item '{}' -ErrorAction SilentlyContinue; if ($f -and $f.PSIsContainer) {{ $size = (Get-ChildItem '{}' -Recurse -File -ErrorAction SilentlyContinue | Measure-Object -Property Length -Sum).Sum; Write-Output \"Folder '{}' size: $([math]::Round($size/1MB,1)) MB\" }}",
            p, p, p
        )
    } else {
        "Get-PSDrive -PSProvider FileSystem | Select-Object Name, Root, @{N='UsedGB';E={[math]::Round($_.Used/1GB,2)}}, @{N='FreeGB';E={[math]::Round($_.Free/1GB,2)}}, @{N='TotalGB';E={[math]::Round(($_.Used+$_.Free)/1GB,2)}} | Format-Table | Out-String".to_string()
    };
    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-Command", &ps]);
    match cmd.output() {
        Ok(out) => (String::from_utf8_lossy(&out.stdout).to_string(), false),
        Err(e) => (format!("Failed to get disk usage: {}", e), true),
    }
}

pub fn manage_windows_service(action: &str, service_name: &str) -> (String, bool) {
    let ps = match action.to_lowercase().as_str() {
        "start" => format!("Start-Service -Name '{}' -ErrorAction Stop; Write-Output \"Service '{}' started successfully.\"", service_name, service_name),
        "stop"  => format!("Stop-Service -Name '{}' -Force -ErrorAction Stop; Write-Output \"Service '{}' stopped successfully.\"", service_name, service_name),
        "restart" => format!("Restart-Service -Name '{}' -Force -ErrorAction Stop; Write-Output \"Service '{}' restarted successfully.\"", service_name, service_name),
        "status" => format!("$s = Get-Service -Name '{}' -ErrorAction SilentlyContinue; if ($s) {{ \"Service: $($s.Name)`nDisplayName: $($s.DisplayName)`nStatus: $($s.Status)`nStartType: $($s.StartType)\" }} else {{ Write-Error \"Service '{}' not found\" }}", service_name, service_name),
        "list"   => "Get-Service | Select-Object Name, Status, StartType | Sort-Object Status, Name | Format-Table | Out-String -Width 120".to_string(),
        _ => return (format!("Unknown action '{}'. Use: start, stop, restart, status, list", action), true),
    };
    let (out, is_err) = execute_powershell(&ps, None, 30);
    (out, is_err)
}

pub fn get_windows_event_log(log_name: &str, max_events: usize, level: Option<&str>) -> (String, bool) {
    let level_filter = match level.unwrap_or("all").to_lowercase().as_str() {
        "error" => " | Where-Object { $_.Level -le 2 }",
        "warning" => " | Where-Object { $_.Level -le 3 }",
        _ => "",
    };
    let ps = format!(
        "Get-WinEvent -LogName '{}' -MaxEvents {} -ErrorAction SilentlyContinue{} | Select-Object TimeCreated, LevelDisplayName, Id, Message | ForEach-Object {{ Write-Output \"[$($_.TimeCreated)] [$($_.LevelDisplayName)] ID=$($_.Id): $($_.Message -replace '`n',' ' | Select-Object -First 1)\" }} | Out-String -Width 200",
        log_name, max_events, level_filter
    );
    let (out, is_err) = execute_powershell(&ps, None, 30);
    if out.trim().is_empty() {
        (format!("No events found in log '{}'.", log_name), false)
    } else {
        (out, is_err)
    }
}

pub fn schedule_task(task_name: &str, command: &str, trigger: &str) -> (String, bool) {
    let trigger_arg = if trigger.starts_with("daily") {
        let time = trigger.trim_start_matches("daily").trim();
        format!("/SC DAILY /ST {}", if time.is_empty() { "09:00" } else { time })
    } else if trigger == "onstart" {
        "/SC ONSTART".to_string()
    } else if trigger == "onlogon" {
        "/SC ONLOGON".to_string()
    } else {
        let now = chrono::Local::now();
        format!("/SC ONCE /ST {}:{:02}", now.format("%H"), (now.minute() + 1) % 60)
    };

    let full_cmd = format!(
        "schtasks /Create /TN \"{}\" /TR \"{}\" {} /F",
        task_name, command, trigger_arg
    );
    execute_cmd(&full_cmd, None)
}
