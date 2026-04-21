use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Create, join, list, or leave supermanager projects from the CLI"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Authenticate this machine with the supermanager server.
    Login {
        #[arg(long, env = "SUPERMANAGER_SERVER_URL", default_value = supermanager::DEFAULT_SERVER_URL)]
        server: String,
    },
    /// Remove the stored supermanager login for this machine.
    Logout,
    /// Manage organization membership and the active organization.
    Orgs {
        #[command(subcommand)]
        command: OrgCommands,
    },
    /// Create new resources in supermanager.
    Create {
        #[command(subcommand)]
        command: CreateCommands,
    },
    /// Configure the current repo to report into a project.
    Join {
        project: String,
        #[arg(long, env = "SUPERMANAGER_SERVER_URL", default_value = supermanager::DEFAULT_SERVER_URL)]
        server: String,
        #[arg(long)]
        org: Option<String>,
        #[arg(long, default_value = ".")]
        cwd: PathBuf,
    },
    /// Remove supermanager configuration from the current repo.
    Leave {
        #[arg(long, default_value = ".")]
        cwd: PathBuf,
    },
    /// Refresh the local Claude/Codex context files for the current repo.
    Context {
        #[command(subcommand)]
        command: ContextCommands,
    },
    /// List the projects currently joined on this machine.
    List,
    /// Install the authenticated Supermanager MCP into global Claude and Codex config.
    Mcp {
        #[command(subcommand)]
        command: McpCommands,
    },
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
    #[command(hide = true)]
    HookSyncContext,
}

#[derive(Subcommand, Debug)]
enum CreateCommands {
    /// Create a new project from the current git repo, connect it, and copy its dashboard URL.
    Project {
        name: Option<String>,
        #[arg(long, env = "SUPERMANAGER_SERVER_URL", default_value = supermanager::DEFAULT_SERVER_URL)]
        server_url: String,
        #[arg(long)]
        org: Option<String>,
        #[arg(long, default_value = ".")]
        cwd: PathBuf,
    },
}

#[derive(Subcommand, Debug)]
enum OrgCommands {
    /// List every organization available to the current account.
    List {
        #[arg(long, env = "SUPERMANAGER_SERVER_URL", default_value = supermanager::DEFAULT_SERVER_URL)]
        server: String,
    },
    /// Create a new organization by entering its name interactively.
    Create {
        #[arg(long, env = "SUPERMANAGER_SERVER_URL", default_value = supermanager::DEFAULT_SERVER_URL)]
        server: String,
    },
    /// Select the active organization with an interactive picker.
    Configure {
        #[arg(long, env = "SUPERMANAGER_SERVER_URL", default_value = supermanager::DEFAULT_SERVER_URL)]
        server: String,
    },
}

#[derive(Subcommand, Debug)]
enum McpCommands {
    /// Install the Supermanager MCP into global Claude and Codex config.
    Install {
        #[arg(long, env = "SUPERMANAGER_SERVER_URL")]
        server: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum ContextCommands {
    /// Refresh exported memories and skills for the current repo's organization.
    Sync {
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
        Commands::Login { server } => {
            let outcome = supermanager::login(supermanager::LoginConfig {
                home_dir,
                server_url: server,
            })?;

            println!();
            println!("  \x1b[32m✓\x1b[0m \x1b[1mLogged in\x1b[0m");
            println!();
            println!("    \x1b[2mServer\x1b[0m     {}", outcome.server_url);
            println!("    \x1b[2mOrg\x1b[0m        {}", outcome.active_org_slug);
            println!();
        }
        Commands::Logout => {
            let removed = supermanager::logout(&home_dir)?;

            println!();
            if removed {
                println!("  \x1b[32m✓\x1b[0m \x1b[1mLogged out\x1b[0m");
            } else {
                println!("  \x1b[33m!\x1b[0m \x1b[1mNo stored login\x1b[0m");
            }
            println!();
        }
        Commands::Orgs { command } => match command {
            OrgCommands::List { server } => {
                let outcome =
                    supermanager::list_organizations(supermanager::ListOrganizationsConfig {
                        home_dir,
                        server_url: server,
                    })?;

                println!();
                println!("  \x1b[32m✓\x1b[0m \x1b[1mOrganizations\x1b[0m");
                println!();
                println!(
                    "    \x1b[2mCount\x1b[0m      {}",
                    outcome.organizations.len()
                );
                println!(
                    "    \x1b[2mActive\x1b[0m     {}",
                    outcome.active_org_slug.as_deref().unwrap_or("not set")
                );

                for organization in outcome.organizations {
                    println!();
                    println!(
                        "    \x1b[2mSlug\x1b[0m       {}",
                        organization.organization_slug
                    );
                    println!(
                        "    \x1b[2mName\x1b[0m       {}",
                        organization.organization_name
                    );
                }
            }
            OrgCommands::Create { server } => {
                let outcome = supermanager::create_organization_interactive(
                    supermanager::CreateOrganizationConfig {
                        home_dir,
                        server_url: server,
                    },
                )?;

                println!();
                println!("  \x1b[32m✓\x1b[0m \x1b[1mOrganization created\x1b[0m");
                println!();
                println!("    \x1b[2mName\x1b[0m       {}", outcome.organization_name);
                println!("    \x1b[2mSlug\x1b[0m       {}", outcome.organization_slug);
                println!("    \x1b[2mActive\x1b[0m     {}", outcome.organization_slug);
                println!();
            }
            OrgCommands::Configure { server } => {
                let outcome = supermanager::configure_organizations_interactive(
                    supermanager::ConfigureOrganizationsConfig {
                        home_dir,
                        server_url: server,
                    },
                )?;

                match outcome {
                    supermanager::ConfigureOrganizationsOutcome::Selected {
                        created_new,
                        organization_name,
                        organization_slug,
                    } => {
                        println!();
                        if created_new {
                            println!(
                                "  \x1b[32m✓\x1b[0m \x1b[1mOrganization created and selected\x1b[0m"
                            );
                        } else {
                            println!(
                                "  \x1b[32m✓\x1b[0m \x1b[1mActive organization updated\x1b[0m"
                            );
                        }
                        println!();
                        println!("    \x1b[2mName\x1b[0m       {}", organization_name);
                        println!("    \x1b[2mSlug\x1b[0m       {}", organization_slug);
                        println!("    \x1b[2mActive\x1b[0m     {}", organization_slug);
                        println!();
                    }
                    supermanager::ConfigureOrganizationsOutcome::InviteRequested => {
                        print_invite_request_guidance("supermanager orgs configure");
                    }
                }
            }
        },
        Commands::Create { command } => match command {
            CreateCommands::Project {
                name,
                server_url,
                org,
                cwd,
            } => {
                let outcome = supermanager::create_project(supermanager::CreateProjectConfig {
                    home_dir: home_dir.clone(),
                    organization_slug: org.clone(),
                    server_url: server_url.clone(),
                    name,
                    cwd,
                })?;
                let join_outcome = supermanager::join_repo(supermanager::JoinConfig {
                    server_url: server_url,
                    organization_slug: org,
                    project_id: outcome.project_id.clone(),
                    repo_dir: outcome.repo_dir.clone(),
                    home_dir,
                });

                match join_outcome {
                    Ok(join_outcome) => {
                        println!();
                        println!("  \x1b[32m✓\x1b[0m \x1b[1mProject created\x1b[0m");
                        println!();
                        println!("    \x1b[2mProject\x1b[0m    {}", outcome.project_id);
                        println!("    \x1b[2mName\x1b[0m       {}", outcome.project_name);
                        println!("    \x1b[2mDashboard\x1b[0m  {}", outcome.dashboard_url);
                        println!(
                            "    \x1b[2mRepo\x1b[0m       {}",
                            outcome.repo_dir.display()
                        );
                        println!("    \x1b[2mShare\x1b[0m      {}", outcome.join_command);
                        println!("    \x1b[2mMember\x1b[0m   {}", join_outcome.member_name);
                        println!();
                        print_clipboard_status(&outcome.dashboard_url);
                    }
                    Err(error) => {
                        println!();
                        println!(
                            "  \x1b[33m!\x1b[0m \x1b[1mProject created, repo setup incomplete\x1b[0m"
                        );
                        println!();
                        println!("    \x1b[2mProject\x1b[0m    {}", outcome.project_id);
                        println!("    \x1b[2mName\x1b[0m       {}", outcome.project_name);
                        println!("    \x1b[2mDashboard\x1b[0m  {}", outcome.dashboard_url);
                        println!(
                            "    \x1b[2mRepo\x1b[0m       {}",
                            outcome.repo_dir.display()
                        );
                        println!("    \x1b[2mRepair\x1b[0m     {}", outcome.join_command);
                        println!();
                        print_clipboard_status(&outcome.dashboard_url);
                        return Err(error).with_context(|| {
                            format!(
                                "project {} was created, but joining repo {} failed; run `{}` after fixing the repo setup",
                                outcome.project_id,
                                outcome.repo_dir.display(),
                                outcome.join_command
                            )
                        });
                    }
                }
            }
        },
        Commands::Join {
            project,
            server,
            org,
            cwd,
        } => {
            let outcome = supermanager::join_repo(supermanager::JoinConfig {
                server_url: server,
                organization_slug: org,
                project_id: project,
                repo_dir: cwd,
                home_dir,
            })?;

            println!();
            println!("  \x1b[32m✓\x1b[0m \x1b[1mJoined project\x1b[0m");
            println!();
            println!("    \x1b[2mProject\x1b[0m    {}", outcome.project_id);
            println!("    \x1b[2mMember\x1b[0m   {}", outcome.member_name);
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
            if outcome.removed_paths.is_empty() {
                println!("  \x1b[33m!\x1b[0m \x1b[1mNo project config found in this repo\x1b[0m");
                println!();
                println!(
                    "    \x1b[2mRepo\x1b[0m       {}",
                    outcome.repo_dir.display()
                );
            } else {
                println!("  \x1b[32m✓\x1b[0m \x1b[1mLeft project\x1b[0m");
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
        }
        Commands::Context { command } => match command {
            ContextCommands::Sync { cwd } => {
                let outcome = supermanager::sync_repo_context(supermanager::SyncContextConfig {
                    home_dir,
                    cwd,
                })?;

                println!();
                if outcome
                    .file_updates
                    .iter()
                    .all(|update| update.status == supermanager::ConfigFileUpdateStatus::Unchanged)
                {
                    println!("  \x1b[32m✓\x1b[0m \x1b[1mContext already up to date\x1b[0m");
                } else {
                    println!("  \x1b[32m✓\x1b[0m \x1b[1mContext synced\x1b[0m");
                }
                println!();
                println!("    \x1b[2mOrg\x1b[0m        {}", outcome.organization_slug);
                println!(
                    "    \x1b[2mRepo\x1b[0m       {}",
                    outcome.repo_dir.display()
                );
                for update in outcome.file_updates {
                    println!(
                        "    \x1b[2mFile\x1b[0m       {} ({})",
                        update.path,
                        format_config_file_update_status(update.status)
                    );
                }
                println!();
            }
        },
        Commands::List => {
            let outcome = supermanager::list_projects(&home_dir)?;

            if outcome.projects.is_empty() {
                println!();
                println!("  \x1b[33m!\x1b[0m \x1b[1mNo joined projects\x1b[0m");
                println!();
                println!("    Join one with `supermanager join <project-id>` inside a git repo");
                return Ok(());
            }

            let repo_count = outcome
                .projects
                .iter()
                .map(|project| project.repo_dirs.len())
                .sum::<usize>();

            println!();
            println!("  \x1b[32m✓\x1b[0m \x1b[1mJoined projects\x1b[0m");
            println!();
            println!("    \x1b[2mProjects\x1b[0m   {}", outcome.projects.len());
            println!("    \x1b[2mRepos\x1b[0m      {}", repo_count);
            println!();
            print_joined_projects(&outcome.projects);
        }
        Commands::Mcp { command } => match command {
            McpCommands::Install { server } => {
                let outcome = supermanager::install_mcp(supermanager::InstallMcpConfig {
                    home_dir,
                    server_url: server,
                })?;

                println!();
                if outcome
                    .file_updates
                    .iter()
                    .all(|update| update.status == supermanager::ConfigFileUpdateStatus::Unchanged)
                {
                    println!("  \x1b[32m✓\x1b[0m \x1b[1mSupermanager MCP already installed\x1b[0m");
                } else {
                    println!("  \x1b[32m✓\x1b[0m \x1b[1mInstalled Supermanager MCP\x1b[0m");
                }
                println!();
                println!("    \x1b[2mServer\x1b[0m     {}", outcome.server_url);
                println!("    \x1b[2mEndpoint\x1b[0m   {}", outcome.mcp_url);
                for update in outcome.file_updates {
                    println!(
                        "    \x1b[2mFile\x1b[0m       {} ({})",
                        update.path,
                        format_config_file_update_status(update.status)
                    );
                }
                println!();
            }
        },
        Commands::Update { check } => {
            let outcome = supermanager::run_self_update(check)?;
            print_self_update_status(&outcome);
        }
        Commands::HookReport { client } => {
            let _ = supermanager::report_hook_turn(&client, &home_dir);
        }
        Commands::HookSyncContext => {
            let _ = supermanager::sync_repo_context_from_hook(&home_dir);
        }
    }
    Ok(())
}

fn should_auto_update(command: &Commands) -> bool {
    matches!(
        command,
        Commands::Create { .. }
            | Commands::Context { .. }
            | Commands::Join { .. }
            | Commands::Leave { .. }
            | Commands::List
            | Commands::Login { .. }
            | Commands::Mcp { .. }
            | Commands::Orgs { .. }
    )
}

fn print_clipboard_status(text: &str) {
    match supermanager::copy_to_clipboard(text) {
        Ok(()) => println!("  \x1b[32m✓\x1b[0m Dashboard URL copied to clipboard"),
        Err(error) => eprintln!("  \x1b[33m!\x1b[0m Clipboard: {error}"),
    }
}

fn print_invite_request_guidance(rerun_command: &str) {
    println!();
    println!("  \x1b[33m!\x1b[0m \x1b[1mAsk your manager for an invite\x1b[0m");
    println!();
    println!(
        "    Ask your manager for an email-bound invite link, then use that email address to accept it."
    );
    println!("    Run       {rerun_command}");
    println!();
}

fn print_joined_projects(projects: &[supermanager::ListProjectEntry]) {
    let project_width = projects
        .iter()
        .map(|project| project.project_id.len())
        .max()
        .unwrap_or(4)
        .max(4);
    let org_width = projects
        .iter()
        .map(|project| project.organization_slug.len())
        .max()
        .unwrap_or(3)
        .max(3);
    let repo_width = projects
        .iter()
        .map(|project| project.repo_dirs.len().to_string().len())
        .max()
        .unwrap_or(5)
        .max(5);

    println!(
        "    {:project_width$}  {:org_width$}  {:>repo_width$}  Server",
        "Project",
        "Org",
        "Repos",
        project_width = project_width,
        org_width = org_width,
        repo_width = repo_width,
    );

    for project in projects {
        println!(
            "    {:project_width$}  {:org_width$}  {:>repo_width$}  {}",
            project.project_id,
            project.organization_slug,
            project.repo_dirs.len(),
            project.server_url,
            project_width = project_width,
            org_width = org_width,
            repo_width = repo_width,
        );
        for repo_dir in &project.repo_dirs {
            println!("      {}", repo_dir.display());
        }
        println!();
    }
}

fn format_config_file_update_status(status: supermanager::ConfigFileUpdateStatus) -> &'static str {
    match status {
        supermanager::ConfigFileUpdateStatus::Created => "created",
        supermanager::ConfigFileUpdateStatus::Updated => "updated",
        supermanager::ConfigFileUpdateStatus::Unchanged => "unchanged",
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
