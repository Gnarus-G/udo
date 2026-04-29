use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Message {
    pub role: String,
    #[serde(default)]
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ToolCall {
    pub function: ToolCallFunction,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ToolCallFunction {
    pub name: String,
    /// Ollama returns this as a JSON object, not a string.
    pub arguments: Value,
}

#[derive(Serialize)]
pub struct Tool {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub function: ToolFunction,
}

#[derive(Serialize)]
pub struct ToolFunction {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: Value,
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [Message],
    tools: &'a [Tool],
    stream: bool,
}

#[derive(Deserialize)]
struct ChatResponse {
    message: Message,
}

pub struct Client {
    http: reqwest::Client,
    host: String,
    api_key: Option<String>,
}

impl Client {
    pub fn new() -> Result<Self> {
        let host = std::env::var("OLLAMA_HOST")
            .unwrap_or_else(|_| "http://localhost:11434".to_string());
        let api_key = std::env::var("OLLAMA_API_KEY").ok();
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()?;
        Ok(Self { http, host, api_key })
    }

    pub async fn chat(
        &self,
        model: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<Message> {
        let url = format!("{}/api/chat", self.host.trim_end_matches('/'));
        let body = ChatRequest {
            model,
            messages,
            tools,
            stream: false,
        };
        let mut req = self.http.post(&url).json(&body);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }
        let resp = req.send().await.with_context(|| format!("POST {url}"))?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("ollama {status}: {text}");
        }
        let parsed: ChatResponse = resp.json().await.context("parsing ollama response")?;
        Ok(parsed.message)
    }
}

pub fn bash_tool() -> Tool {
    Tool {
        kind: "function",
        function: ToolFunction {
            name: "bash",
            description: "Run a bash command on the user's Linux system. Returns stdout, stderr, and exit_code.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The bash command to execute. Runs as `bash -c <command>`."
                    }
                },
                "required": ["command"]
            }),
        },
    }
}
