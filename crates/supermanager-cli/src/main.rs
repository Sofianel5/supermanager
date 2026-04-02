use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Join or leave supermanager rooms from a local repo"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Configure the current repo to report into a room.
    Join {
        #[arg(long)]
        server: String,
        #[arg(long)]
        room: String,
        #[arg(long)]
        secret: String,
        #[arg(long, default_value = ".")]
        cwd: PathBuf,
    },
    /// Remove supermanager configuration from the current repo.
    Leave {
        #[arg(long, default_value = ".")]
        cwd: PathBuf,
    },
    #[command(hide = true)]
    HookReport {
        #[arg(long, value_parser = ["claude", "codex"])]
        client: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let home_dir = supermanager::resolve_home_dir()?;

    match cli.command {
        Commands::Join {
            server,
            room,
            secret,
            cwd,
        } => {
            let repo_dir = cwd.canonicalize().unwrap_or(cwd);
            let outcome = supermanager::join_repo(supermanager::JoinConfig {
                server_url: server,
                room_id: room,
                secret,
                repo_dir,
                home_dir,
            })?;

            println!();
            println!("supermanager join complete");
            println!("room: {}", outcome.room_id);
            println!("employee: {}", outcome.employee_name);
            println!("dashboard: {}", outcome.dashboard_url);
            println!("repo: {}", outcome.repo_dir.display());
        }
        Commands::Leave { cwd } => {
            let repo_dir = cwd.canonicalize().unwrap_or(cwd);
            let outcome = supermanager::leave_repo(&repo_dir, &home_dir)?;

            println!();
            println!("supermanager leave complete");
            println!("repo: {}", outcome.repo_dir.display());
            println!("removed: {}", outcome.removed_paths.join(", "));
        }
        Commands::HookReport { client } => {
            let _ = supermanager::report_hook_turn(&client, &home_dir);
        }
    }
    Ok(())
}
