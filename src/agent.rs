use anyhow::{Context, Result};

use crate::bash;
use crate::ollama::{self, Client, Message};

const MAX_ITER: usize = 25;
const SYSTEM_PROMPT: &str = "You are a bash agent on Linux. Use the `bash` tool to accomplish the user's task in as few commands as possible. Inspect output as needed. When the task is done, reply with a single short sentence summarizing what you did, and stop calling tools. If the task is impossible or unsafe, explain briefly and stop.";

pub async fn run(client: &Client, model: &str, prompt: &str) -> Result<String> {
    let mut messages = vec![
        Message {
            role: "system".into(),
            content: SYSTEM_PROMPT.into(),
            tool_calls: None,
        },
        Message {
            role: "user".into(),
            content: prompt.into(),
            tool_calls: None,
        },
    ];
    let tools = vec![ollama::bash_tool()];

    for iter in 1..=MAX_ITER {
        println!("--- iteration {iter} ---");
        let msg = client.chat(model, &messages, &tools).await?;

        let calls = msg.tool_calls.clone().unwrap_or_default();
        let content = msg.content.clone();
        messages.push(msg);

        if calls.is_empty() {
            println!("agent done: {content}");
            return Ok(content);
        }

        for call in calls {
            let cmd = call
                .function
                .arguments
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            println!("$ {cmd}");
            let result = bash::run(cmd).await;
            println!(
                "  exit={} stdout={}B stderr={}B",
                result.exit_code,
                result.stdout.len(),
                result.stderr.len()
            );
            let serialized = serde_json::to_string(&result).context("serializing tool result")?;
            messages.push(Message {
                role: "tool".into(),
                content: serialized,
                tool_calls: None,
            });
        }
    }

    anyhow::bail!("hit max iterations ({MAX_ITER}) without completion")
}
