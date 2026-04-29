use serde::Serialize;
use tokio::process::Command;

const MAX_OUTPUT: usize = 8 * 1024;

#[derive(Serialize)]
pub struct ToolResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

pub async fn run(command: &str) -> ToolResult {
    let out = Command::new("bash")
        .arg("-c")
        .arg(command)
        .output()
        .await;

    match out {
        Ok(o) => ToolResult {
            stdout: truncate(String::from_utf8_lossy(&o.stdout).into_owned()),
            stderr: truncate(String::from_utf8_lossy(&o.stderr).into_owned()),
            exit_code: o.status.code().unwrap_or(-1),
        },
        Err(e) => ToolResult {
            stdout: String::new(),
            stderr: format!("failed to spawn bash: {e}"),
            exit_code: -1,
        },
    }
}

fn truncate(mut s: String) -> String {
    if s.len() > MAX_OUTPUT {
        s.truncate(MAX_OUTPUT);
        s.push_str("\n...[truncated]");
    }
    s
}
