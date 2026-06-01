mod agent;
mod bash;
mod frontend;
mod notify;
mod ollama;
mod paths;
mod progress;
mod text;
mod watch;
mod worker;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "udo", version, about = "Async bash agent. Runs detached, notifies on completion.")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Natural-language task description (runs when no subcommand is given)
    prompt: Option<String>,

    /// Ollama model to use
    #[arg(long, env = "UDO_MODEL", default_value = "glm-5.1:cloud")]
    model: String,

    /// Internal: run as detached worker
    #[arg(long = "__worker", hide = true)]
    worker: bool,

    /// Internal: task id assigned by the frontend
    #[arg(long, hide = true, default_value = "")]
    task_id: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Watch task progress in a TUI
    Watch {
        /// Specific task id to watch (watches all recent if omitted)
        task_id: Option<String>,
    },
    /// Show task status as JSON (for Eww/bar integration)
    Status,
    /// List all known task ids as JSON
    List,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if cli.worker {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        return rt.block_on(worker::run(cli.task_id, cli.prompt.unwrap_or_default(), cli.model));
    }

    match cli.command {
        Some(Commands::Watch { task_id }) => watch::run(task_id),
        Some(Commands::Status) => {
            let status = progress::eww_status();
            println!("{}", serde_json::to_string_pretty(&status)?);
            Ok(())
        }
        Some(Commands::List) => {
            let ids = progress::list_task_ids()?;
            println!("{}", serde_json::to_string_pretty(&ids)?);
            Ok(())
        }
        None => match cli.prompt {
            Some(prompt) => frontend::run(prompt, cli.model),
            None => {
                let prompt = read_prompt_interactively()?;
                frontend::run(prompt, cli.model)
            }
        },
    }
}

fn read_prompt_interactively() -> anyhow::Result<String> {
    use std::io::IsTerminal;
    let stdin = std::io::stdin();
    if !stdin.is_terminal() {
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut stdin.lock(), &mut buf)?;
        let s = String::from_utf8(buf)?;
        let trimmed = s.trim();
        anyhow::ensure!(!trimmed.is_empty(), "no prompt provided on stdin");
        return Ok(trimmed.to_string());
    }

    let q = inquire::Text::new("What should udo do?").with_help_message("⏎ to submit · Esc to cancel");
    match q.prompt() {
        Ok(s) if s.is_empty() => anyhow::bail!("empty prompt"),
        Ok(s) => Ok(s),
        Err(inquire::InquireError::OperationCanceled) => anyhow::bail!("cancelled"),
        Err(e) => Err(e.into()),
    }
}