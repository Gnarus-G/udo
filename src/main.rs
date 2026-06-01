mod agent;
mod bash;
mod frontend;
mod notify;
mod ollama;
mod paths;
mod progress;
mod watch;
mod worker;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "udo", version, about = "Async bash agent. Runs detached, notifies on completion.")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Natural-language task description (shorthand for `udo run <prompt>`)
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
    /// Run a task (default when no subcommand given)
    Run {
        /// Natural-language task description
        prompt: String,

        /// Ollama model to use
        #[arg(long, env = "UDO_MODEL", default_value = "glm-5.1:cloud")]
        model: String,
    },
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
        Some(Commands::Run { prompt, model }) => frontend::run(prompt, model),
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
        None => {
            if let Some(prompt) = cli.prompt {
                frontend::run(prompt, cli.model)
            } else {
                eprintln!("udo: provide a prompt or use a subcommand (run, watch, status, list)");
                std::process::exit(1);
            }
        }
    }
}