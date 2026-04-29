mod agent;
mod bash;
mod frontend;
mod notify;
mod ollama;
mod paths;
mod worker;

use clap::Parser;

#[derive(Parser)]
#[command(name = "udo", version, about = "Async bash agent. Runs detached, notifies on completion.")]
struct Cli {
    /// Natural-language task description, e.g. "kill program on port 3999"
    prompt: String,

    /// Ollama model to use
    #[arg(long, env = "UDO_MODEL", default_value = "glm-5.1:cloud")]
    model: String,

    /// Internal: run as detached worker. Don't invoke directly.
    #[arg(long = "__worker", hide = true)]
    worker: bool,

    /// Internal: task id assigned by the frontend.
    #[arg(long, hide = true, default_value = "")]
    task_id: String,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if cli.worker {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        rt.block_on(worker::run(cli.task_id, cli.prompt, cli.model))
    } else {
        frontend::run(cli.prompt, cli.model)
    }
}
