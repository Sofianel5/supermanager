use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Create, join, list, or leave supermanager rooms from the CLI"
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
    /// Authenticate this machine in the browser.
    Login {
        #[arg(long, env = "SUPERMANAGER_SERVER_URL", default_value = supermanager::DEFAULT_SERVER_URL)]
        server: String,
    },
    /// Show the currently signed-in user.
    Whoami {
        #[arg(long, env = "SUPERMANAGER_SERVER_URL", default_value = supermanager::DEFAULT_SERVER_URL)]
        server: String,
    },
    /// Remove the stored browser login for this machine.
    Logout,
    /// Create room invites.
    Invite {
        #[command(subcommand)]
        command: InviteCommands,
    },
    /// Remove supermanager configuration from the current repo.
    Leave {
        #[arg(long, default_value = ".")]
        cwd: PathBuf,
    },
    /// List the rooms currently joined on this machine.
    List,
    /// Check for and install the latest published CLI release.
    Update {
        #[arg(long)]
        check: bool,
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

#[derive(Subcommand, Debug)]
enum InviteCommands {
    /// Create an invite restricted to one email address.
    Email {
        email: String,
        #[arg(long)]
        room: Option<String>,
        #[arg(long, env = "SUPERMANAGER_SERVER_URL", default_value = supermanager::DEFAULT_SERVER_URL)]
        server: String,
        #[arg(long, default_value = ".")]
        cwd: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let home_dir = supermanager::resolve_home_dir()?;

    if should_auto_update(&cli.command)
        && let Ok(Some(outcome)) = supermanager::maybe_auto_update(&home_dir)
    {
        print_self_update_status(&outcome);
    }

    match cli.command {
        Commands::Create { command } => match command {
            CreateCommands::Room {
                name,
                server_url,
                app_url,
                cwd,
            } => {
                let _ = supermanager::ensure_login(&server_url, &home_dir)?;
                let outcome = supermanager::create_room(supermanager::CreateRoomConfig {
                    server_url: server_url.clone(),
                    name,
                    cwd,
                    home_dir: home_dir.clone(),
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
            let _ = supermanager::ensure_login(&server, &home_dir)?;
            let room = supermanager::get_room(&server, &room, &home_dir)?;
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
        Commands::Login { server } => {
            let outcome = supermanager::login(supermanager::LoginConfig {
                server_url: server,
                home_dir,
            })?;

            println!();
            println!("  \x1b[32m✓\x1b[0m \x1b[1mSigned in\x1b[0m");
            println!();
            println!("    \x1b[2mName\x1b[0m       {}", outcome.user.display_name);
            println!(
                "    \x1b[2mEmail\x1b[0m      {}",
                outcome.user.primary_email
            );
        }
        Commands::Whoami { server } => {
            let user = supermanager::whoami(&server, &home_dir)?;

            println!();
            println!("  \x1b[32m✓\x1b[0m \x1b[1mCurrent user\x1b[0m");
            println!();
            println!("    \x1b[2mName\x1b[0m       {}", user.display_name);
            println!("    \x1b[2mEmail\x1b[0m      {}", user.primary_email);
        }
        Commands::Logout => {
            let removed = supermanager::logout(&home_dir)?;

            println!();
            if removed {
                println!("  \x1b[32m✓\x1b[0m \x1b[1mSigned out\x1b[0m");
            } else {
                println!("  \x1b[33m!\x1b[0m \x1b[1mNo stored login\x1b[0m");
            }
        }
        Commands::Invite { command } => match command {
            InviteCommands::Email {
                email,
                room,
                server,
                cwd,
            } => {
                let _ = supermanager::ensure_login(&server, &home_dir)?;
                let room_id = resolve_invite_room(room, &server, &cwd, &home_dir)?;
                let invite =
                    supermanager::create_email_invite(&server, &room_id, &email, &home_dir)?;

                println!();
                println!("  \x1b[32m✓\x1b[0m \x1b[1mEmail invite created\x1b[0m");
                println!();
                println!("    \x1b[2mRoom\x1b[0m       {}", invite.room_id);
                println!(
                    "    \x1b[2mEmail\x1b[0m      {}",
                    invite.target_email.unwrap_or(email)
                );
                println!("    \x1b[2mExpires\x1b[0m    {}", invite.expires_at);
                if let Some(invite_url) = invite.invite_url.as_deref() {
                    println!("    \x1b[2mInvite\x1b[0m     {invite_url}");
                    println!();
                    print_clipboard_status(invite_url);
                }
            }
        },
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
        Commands::List => {
            let outcome = supermanager::list_rooms(&home_dir)?;

            if outcome.rooms.is_empty() {
                println!();
                println!("  \x1b[33m!\x1b[0m \x1b[1mNo joined rooms\x1b[0m");
                println!();
                println!("    Join one with `supermanager join <room-code>` inside a git repo");
                return Ok(());
            }

            let repo_count = outcome
                .rooms
                .iter()
                .map(|room| room.repo_dirs.len())
                .sum::<usize>();

            println!();
            println!("  \x1b[32m✓\x1b[0m \x1b[1mJoined rooms\x1b[0m");
            println!();
            println!("    \x1b[2mRooms\x1b[0m      {}", outcome.rooms.len());
            println!("    \x1b[2mRepos\x1b[0m      {}", repo_count);

            for room in outcome.rooms {
                println!();
                println!("    \x1b[2mRoom\x1b[0m       {}", room.room_id);
                println!("    \x1b[2mServer\x1b[0m     {}", room.server_url);
                if let Some((first_repo, other_repos)) = room.repo_dirs.split_first() {
                    println!("    \x1b[2mRepos\x1b[0m      {}", first_repo.display());
                    for repo_dir in other_repos {
                        println!("               {}", repo_dir.display());
                    }
                }
            }
        }
        Commands::Update { check } => {
            let outcome = supermanager::run_self_update(check)?;
            print_self_update_status(&outcome);
        }
        Commands::HookReport { client } => {
            let _ = supermanager::report_hook_turn(&client, &home_dir);
        }
    }
    Ok(())
}

fn should_auto_update(command: &Commands) -> bool {
    matches!(
        command,
        Commands::Create { .. }
            | Commands::Join { .. }
            | Commands::Leave { .. }
            | Commands::List
            | Commands::Invite { .. }
    )
}

fn print_clipboard_status(text: &str) {
    match supermanager::copy_to_clipboard(text) {
        Ok(()) => println!("  \x1b[32m✓\x1b[0m Copied to clipboard"),
        Err(error) => eprintln!("  \x1b[33m!\x1b[0m Clipboard: {error}"),
    }
}

fn print_self_update_status(outcome: &supermanager::SelfUpdateOutcome) {
    match outcome {
        supermanager::SelfUpdateOutcome::Updated {
            previous_version,
            current_version,
        } => {
            println!();
            println!("  \x1b[32m✓\x1b[0m \x1b[1mCLI updated\x1b[0m");
            println!();
            println!("    \x1b[2mFrom\x1b[0m       {previous_version}");
            println!("    \x1b[2mTo\x1b[0m         {current_version}");
            println!();
        }
        supermanager::SelfUpdateOutcome::UpdateAvailable {
            current_version,
            latest_version,
        } => {
            println!();
            println!("  \x1b[33m!\x1b[0m \x1b[1mUpdate available\x1b[0m");
            println!();
            println!("    \x1b[2mCurrent\x1b[0m    {current_version}");
            println!("    \x1b[2mLatest\x1b[0m     {latest_version}");
            println!();
        }
        supermanager::SelfUpdateOutcome::AlreadyCurrent { version } => {
            println!();
            println!("  \x1b[32m✓\x1b[0m \x1b[1mAlready up to date\x1b[0m");
            println!();
            println!("    \x1b[2mVersion\x1b[0m    {version}");
            println!();
        }
        supermanager::SelfUpdateOutcome::Unsupported { reason } => {
            println!();
            println!("  \x1b[33m!\x1b[0m \x1b[1mSelf-update unavailable\x1b[0m");
            println!();
            println!("    \x1b[2mReason\x1b[0m     {reason}");
            println!();
        }
    }
}

fn resolve_invite_room(
    explicit_room: Option<String>,
    server: &str,
    cwd: &PathBuf,
    home_dir: &PathBuf,
) -> Result<String> {
    if let Some(room) = explicit_room {
        return Ok(room);
    }

    supermanager::joined_room_for_path(home_dir, cwd, server)?.ok_or_else(|| {
        anyhow::anyhow!(
            "no joined room found for {}; pass --room explicitly",
            cwd.display()
        )
    })
}
