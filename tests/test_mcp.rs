use claude_desktop_rust::mcp::{McpClientHandle, McpServerConfig};

#[tokio::test]
async fn test_roblox_mcp_connection() {
    let env = std::collections::HashMap::new();
    let cfg = McpServerConfig {
        command: "cmd.exe".to_string(),
        args: vec!["/c".to_string(), "cd /d %LOCALAPPDATA%\\Roblox && .\\mcp.bat".to_string()],
        env,
        url: None,
    };

    println!("Testing Roblox MCP spawn...");
    let res = McpClientHandle::spawn("Roblox_Studio".to_string(), cfg).await;
    match res {
        Ok((client, tools)) => {
            println!("Successfully connected to Roblox Studio MCP!");
            println!("Found {} tools:", tools.len());
            for t in tools.iter().take(5) {
                println!(" - {}", t.name);
            }
            client.shutdown().await;
        }
        Err(e) => {
            println!("Roblox MCP spawn result: {}", e);
        }
    }
}
