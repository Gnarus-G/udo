# udo

Async bash agent. Describe a task in natural language, `udo` spawns a
detached worker that loops with an Ollama model calling `bash` until the task
is done, then drops a desktop notification.

```
$ udo "find all TODOs in this repo and group them by file"
udo: task 04a3 started (pid 12847, log /home/you/.local/state/udo/log/04a3.log)
```

watch with: `udo watch 04a3`

## Install

```
cargo build --release
# binary at target/release/udo
```

Requires an Ollama server (local or remote) with a tool-calling model pulled.

## Usage

Run a task:

```
udo "list the 5 largest files under /var/log"
udo --model qwen2.5:7b "restart the docker compose stack"
```

If no prompt is given, `udo` reads stdin, or asks interactively.

Watch progress in a TUI:

```
udo watch            # all recent tasks
udo watch 04a3       # focus on one
```

Keys: `j`/`k` or arrows to move, `PgUp`/`PgDn` to scroll events, `q` to quit.

Machine-readable status for bars (Eww, waybar, etc.):

```
udo status    # JSON: running/recent counts + latest task
udo list      # JSON: array of all known task ids
```

## How it works

1. The frontend assigns a 4-hex-char task id, opens a log file, and re-execs
   itself as a detached worker via `setsid`.
2. The worker opens a JSONL progress file under the state dir, emits a
   `Started` event, then drives the agent loop.
3. The agent sends the conversation (system + user + tool results) to Ollama's
   `/api/chat` with a single tool: `bash(command)`. Up to 25 iterations.
4. Each tool call runs `bash -c <command>`; stdout/stderr are truncated to
   8 KiB and returned to the model.
5. On completion or failure, the worker emits a terminal event and fires a
   `notify-rust` desktop notification pointing at the log file.

## State

Everything lives under `$XDG_STATE_HOME/udo` (falls back to
`~/.local/state/udo`):

```
udo/
  log/<task_id>.log       # raw stdout/stderr of the bash tool calls
  tasks/<task_id>.jsonl   # one JSON event per line (see below)
```

Task events (`progress::Event`):

| event                     | fields                                |
| ------------------------- | ------------------------------------- |
| `started`                 | `task_id`, `model`, `prompt`          |
| `iteration_started`       | `iter`                                |
| `model_request_started`   | `iter`                                |
| `model_response_received` | `iter`                                |
| `tool_started`            | `iter`, `command`                     |
| `tool_finished`           | `iter`, `command`, `exit_code`, sizes |
| `completed`               | `summary`                             |
| `failed`                  | `error`                               |

The TUI and `udo status` derive everything from these files, so you can also
tail them directly.

## Configuration

| env var          | default                  | meaning                         |
| ---------------- | ------------------------ | ------------------------------- |
| `UDO_MODEL`      | `glm-5.1:cloud`          | default Ollama model            |
| `OLLAMA_HOST`    | `http://localhost:11434` | Ollama API base URL             |
| `OLLAMA_API_KEY` | _(none)_                 | sent as `Authorization: Bearer` |

`--model` overrides `UDO_MODEL` per-invocation.

## Exit codes (worker)

| code | meaning                           |
| ---- | --------------------------------- |
| 0    | task completed                    |
| 1    | agent error or client init failed |
| 2    | notification failed               |

## License

[GPL-2.0](LICENSE)
