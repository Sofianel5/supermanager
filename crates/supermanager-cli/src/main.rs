use std::path::PathBuf;

use anyhow::{Context, Result};
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
    /// Create a new room from the current git repo, connect it, and copy its dashboard URL.
    Room {
        name: Option<String>,
        #[arg(long, env = "SUPERMANAGER_SERVER_URL", default_value = supermanager::DEFAULT_SERVER_URL)]
        server_url: String,
        #[arg(long, env = "SUPERMANAGER_APP_URL", default_value = supermanager::DEFAULT_APP_URL)]
        app_url: String,
        #[arg(long, default_value = ".")]
        cwd: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let home_dir = supermanager::resolve_home_dir()?;

    match cli.command {
        Commands::Create { command } => match command {
            CreateCommands::Room {
                name,
                server_url,
                app_url,
                cwd,
            } => {
                let outcome = supermanager::create_room(supermanager::CreateRoomConfig {
                    server_url: server_url.clone(),
                    name,
                    cwd,
                })?;
                let join_outcome = supermanager::join_repo(supermanager::JoinConfig {
                    server_url: server_url,
                    app_url: app_url,
                    room_id: outcome.room_id.clone(),
                    repo_dir: outcome.repo_dir.clone(),
                    home_dir,
                });

                println!();
                println!("  \x1b[32m✓\x1b[0m \x1b[1mRoom created\x1b[0m");
                println!();
                println!("    \x1b[2mRoom\x1b[0m       {}", outcome.room_id);
                println!("    \x1b[2mName\x1b[0m       {}", outcome.room_name);
                println!("    \x1b[2mDashboard\x1b[0m  {}", outcome.dashboard_url);
                println!(
                    "    \x1b[2mRepo\x1b[0m       {}",
                    outcome.repo_dir.display()
                );
                println!("    \x1b[2mShare\x1b[0m      {}", outcome.join_command);

                match join_outcome {
                    Ok(join_outcome) => {
                        println!(
                            "    \x1b[2mEmployee\x1b[0m   {}",
                            join_outcome.employee_name
                        );
                        println!();
                        print_clipboard_status(&outcome.dashboard_url);
                    }
                    Err(error) => {
                        println!();
                        print_clipboard_status(&outcome.dashboard_url);
                        return Err(error).with_context(|| {
                            format!(
                                "room {} was created, but joining repo {} failed; run `{}` after fixing the repo setup",
                                outcome.room_id,
                                outcome.repo_dir.display(),
                                outcome.join_command
                            )
                        });
                    }
                }
            }
        },
        Commands::Join {
            room,
            server,
            app_url,
            cwd,
        } => {
            let room = supermanager::get_room(&server, &room)?;
            let outcome = supermanager::join_repo(supermanager::JoinConfig {
                server_url: server,
                app_url,
                room_id: room.room_id,
                repo_dir: cwd,
                home_dir,
            })?;

            println!();
            println!("  \x1b[32m✓\x1b[0m \x1b[1mJoined room\x1b[0m");
            println!();
            println!("    \x1b[2mRoom\x1b[0m       {}", outcome.room_id);
            println!("    \x1b[2mEmployee\x1b[0m   {}", outcome.employee_name);
            println!("    \x1b[2mDashboard\x1b[0m  {}", outcome.dashboard_url);
            println!(
                "    \x1b[2mRepo\x1b[0m       {}",
                outcome.repo_dir.display()
            );
            println!();
            print_clipboard_status(&outcome.dashboard_url);
        }
        Commands::Leave { cwd } => {
            let outcome = supermanager::leave_repo(&cwd, &home_dir)?;

            println!();
            println!("  \x1b[32m✓\x1b[0m \x1b[1mLeft room\x1b[0m");
            println!();
            println!(
                "    \x1b[2mRepo\x1b[0m       {}",
                outcome.repo_dir.display()
            );
            println!(
                "    \x1b[2mRemoved\x1b[0m    {}",
                outcome.removed_paths.join(", ")
            );
        }
        Commands::HookReport { client } => {
            let _ = supermanager::report_hook_turn(&client, &home_dir);
        }
    }
    Ok(())
}

fn print_clipboard_status(text: &str) {
    match supermanager::copy_to_clipboard(text) {
        Ok(()) => println!("  \x1b[32m✓\x1b[0m Dashboard URL copied to clipboard"),
        Err(error) => eprintln!("  \x1b[33m!\x1b[0m Clipboard: {error}"),
    }
}
