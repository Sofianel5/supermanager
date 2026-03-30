use std::{
    env,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use reporter_protocol::{Host, NoteKind, ProgressNote};
use serde_json::{Value, json};

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Submit freeform progress notes from Codex/Claude hooks"
)]
struct Cli {
    #[command(subcommand)]
    command: CommandKind,
}

#[derive(Subcommand, Debug)]
enum CommandKind {
    SubmitProgress(SubmitArgs),
}

#[derive(Args, Debug, Clone)]
struct SubmitArgs {
    #[arg(long, value_enum)]
    host: Host,
    #[arg(long, value_enum)]
    kind: NoteKind,
    #[arg(long, env = "REPORTER_EMPLOYEE_NAME")]
    employee_name: Option<String>,
    #[arg(long)]
    cwd: Option<PathBuf>,
    #[arg(long, env = "REPORTER_SERVER_URL")]
    server_url: Option<String>,
    #[arg(long, env = "REPORTER_API_TOKEN")]
    api_token: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct GitContext {
    repo_root: Option<String>,
    branch: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        CommandKind::SubmitProgress(args) => submit_progress(args),
    }
}

fn submit_progress(args: SubmitArgs) -> Result<()> {
    let server_url = args
        .server_url
        .as_deref()
        .context("REPORTER_SERVER_URL not set")?;
    let employee_name = args
        .employee_name
        .as_deref()
        .context("REPORTER_EMPLOYEE_NAME not set")?;

    let payload = read_optional_payload()?;
    let cwd = effective_cwd(args.cwd)?;
    let git = snapshot_git_context(&cwd);
    let workspace = git.repo_root.clone().unwrap_or_else(|| display_path(&cwd));
    let note = ProgressNote {
        employee_name: employee_name.to_owned(),
        host: args.host,
        kind: args.kind,
        workspace,
        branch: git.branch.clone(),
        progress_text: build_progress_text(args.host, args.kind, &cwd, &git, &payload),
    };

    let client = reqwest::blocking::Client::new();
    let endpoint = format!("{}/v1/progress", server_url.trim_end_matches('/'));
    let mut request = client.post(endpoint).json(&note);
    if let Some(token) = args.api_token.as_deref() {
        request = request.bearer_auth(token);
    }

    request
        .send()
        .context("failed to reach coordination server")?
        .error_for_status()
        .context("coordination server rejected progress note")?;

    Ok(())
}

fn build_progress_text(
    host: Host,
    kind: NoteKind,
    cwd: &Path,
    git: &GitContext,
    payload: &Option<Value>,
) -> String {
    let prompt = extract_string_field(
        payload,
        &["prompt", "user_prompt", "message", "task", "input"],
    );
    let output = extract_string_field(
        payload,
        &[
            "summary",
            "assistant_response",
            "response",
            "output",
            "result",
        ],
    );

    let mut lines = vec![
        format!("Host: {host}"),
        format!("Kind: {kind}"),
        format!(
            "Workspace: {}",
            git.repo_root
                .as_deref()
                .unwrap_or_else(|| cwd.to_str().unwrap_or(""))
        ),
    ];

    if let Some(branch) = &git.branch {
        lines.push(format!("Branch: {branch}"));
    }

    match kind {
        NoteKind::Intent => {
            lines.push(String::new());
            lines.push("User intent:".to_owned());
            lines
                .push(prompt.unwrap_or_else(|| {
                    "No explicit user prompt found in hook payload.".to_owned()
                }));
        }
        NoteKind::Progress => {
            lines.push(String::new());
            lines.push("Agent progress:".to_owned());
            lines.push(output.unwrap_or_else(|| {
                "No explicit assistant summary found in hook payload; inspect raw payload for details."
                    .to_owned()
            }));

            if let Some(prompt) = prompt {
                lines.push(String::new());
                lines.push("Related user ask:".to_owned());
                lines.push(prompt);
            }
        }
    }

    if let Some(raw_text) = payload
        .as_ref()
        .and_then(|value| value.get("raw_text"))
        .and_then(Value::as_str)
    {
        lines.push(String::new());
        lines.push("Raw hook text:".to_owned());
        lines.push(truncate_text(raw_text));
    }

    lines.join("\n")
}

fn snapshot_git_context(cwd: &Path) -> GitContext {
    GitContext {
        repo_root: run_git(cwd, ["rev-parse", "--show-toplevel"]),
        branch: run_git(cwd, ["rev-parse", "--abbrev-ref", "HEAD"]),
    }
}

fn run_git<const N: usize>(cwd: &Path, args: [&str; N]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn read_optional_payload() -> Result<Option<Value>> {
    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer)?;
    let trimmed = buffer.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    match serde_json::from_str(trimmed) {
        Ok(value) => Ok(Some(value)),
        Err(_) => Ok(Some(json!({ "raw_text": trimmed }))),
    }
}

fn extract_string_field(payload: &Option<Value>, keys: &[&str]) -> Option<String> {
    let object = payload.as_ref()?.as_object()?;

    keys.iter().find_map(|key| {
        object.get(*key).and_then(|value| match value {
            Value::String(text) => Some(text.clone()),
            Value::Number(number) => Some(number.to_string()),
            _ => None,
        })
    })
}

fn effective_cwd(cli_cwd: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(cwd) = cli_cwd {
        return Ok(cwd);
    }

    if let Ok(cwd) = env::var("PWD") {
        return Ok(PathBuf::from(cwd));
    }

    env::current_dir().context("failed to resolve current directory")
}

fn display_path(path: &Path) -> String {
    path.display().to_string()
}

fn truncate_text(value: &str) -> String {
    const MAX_LEN: usize = 500;
    if value.chars().count() <= MAX_LEN {
        return value.to_owned();
    }

    let mut truncated = value.chars().take(MAX_LEN).collect::<String>();
    truncated.push_str("...");
    truncated
}
