use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Create, join, or leave supermanager rooms from the CLI"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Create new resources in supermanager.
    Create {
        #[command(subcommand)]
        command: CreateCommands,
    },
    /// Configure the current repo to report into a room.
    Join {
        room: String,
        #[arg(long, env = "SUPERMANAGER_SERVER_URL", default_value = supermanager::DEFAULT_SERVER_URL)]
        server: String,
        #[arg(long, env = "SUPERMANAGER_APP_URL", default_value = supermanager::DEFAULT_APP_URL)]
        app_url: String,
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

#[derive(Subcommand, Debug)]
enum CreateCommands {
    /// Create a new room and copy its dashboard URL.
    Room {
        name: Option<String>,
        #[arg(long, env = "SUPERMANAGER_SERVER_URL", default_value = supermanager::DEFAULT_SERVER_URL)]
        server: String,
        #[arg(long, default_value = ".")]
        cwd: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let home_dir = supermanager::resolve_home_dir()?;

    match cli.command {
        Commands::Create { command } => match command {
            CreateCommands::Room { name, server, cwd } => {
                let cwd = cwd.canonicalize().unwrap_or(cwd);
                let outcome = supermanager::create_room(supermanager::CreateRoomConfig {
                    server_url: server,
                    name,
                    cwd,
                })?;

                println!();
                println!("supermanager room created");
                println!("room: {}", outcome.room_id);
                println!("name: {}", outcome.room_name);
                println!("dashboard: {}", outcome.dashboard_url);
                println!("join: {}", outcome.join_command);
                print_clipboard_status(&outcome.dashboard_url);
            }
        },
        Commands::Join {
            room,
            server,
            app_url,
            cwd,
        } => {
            let repo_dir = cwd.canonicalize().unwrap_or(cwd);
            let room = supermanager::get_room(&server, &room)?;
            let outcome = supermanager::join_repo(supermanager::JoinConfig {
                server_url: server,
                app_url,
                room_id: room.room_id,
                repo_dir,
                home_dir,
            })?;

            println!();
            println!("supermanager join complete");
            println!("room: {}", outcome.room_id);
            println!("employee: {}", outcome.employee_name);
            println!("dashboard: {}", outcome.dashboard_url);
            println!("repo: {}", outcome.repo_dir.display());
            print_clipboard_status(&outcome.dashboard_url);
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

fn print_clipboard_status(text: &str) {
    match supermanager::copy_to_clipboard(text) {
        Ok(()) => println!("clipboard: dashboard url copied"),
        Err(error) => eprintln!("clipboard: {error}"),
    }
}
