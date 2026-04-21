# Supermanager

Supermanager is an authenticated, organization-scoped coordination system for coding agents. The Bun/Elysia backend owns Better Auth, hook ingest, PostgreSQL state, and SSE. A separate Rust workflow worker owns project and organization workflow orchestration (summaries, memory extraction and consolidation, skills) plus Codex runtime state. The React frontend owns the public sign-in page, organization workspace, device approval flow, invite acceptance, and live project dashboard.

## Setup

### 1. Start the coordination server

```sh
cd packages/server
bun install
export DATABASE_URL='postgres://supermanager:password@127.0.0.1:5432/supermanager?sslmode=disable'
export BETTER_AUTH_SECRET='replace-me'
export GOOGLE_CLIENT_ID='replace-me'
export GOOGLE_CLIENT_SECRET='replace-me'
export GITHUB_CLIENT_ID='replace-me'
export GITHUB_CLIENT_SECRET='replace-me'
export SUPERMANAGER_PUBLIC_API_URL='http://127.0.0.1:8787'
export SUPERMANAGER_PUBLIC_APP_URL='http://127.0.0.1:5173'
bun run src/main.ts
```

The server reads runtime config from environment variables:

- `DATABASE_URL`
- `BETTER_AUTH_SECRET` or `AUTH_SECRET`
- `GOOGLE_CLIENT_ID`
- `GOOGLE_CLIENT_SECRET`
- `GITHUB_CLIENT_ID`
- `GITHUB_CLIENT_SECRET`
- `SUPERMANAGER_PUBLIC_API_URL`
- `SUPERMANAGER_PUBLIC_APP_URL`
- `CODEX_API_KEY`

### 2. Start the workflow worker

```sh
cd crates/workflow-agent
export DATABASE_URL='postgres://supermanager:password@127.0.0.1:5432/supermanager?sslmode=disable'
export SUPERMANAGER_DATA_DIR='../../.supermanager-data'
export SUPERMANAGER_ORGANIZATION_SUMMARY_REFRESH_INTERVAL_SECONDS='300'
export SUPERMANAGER_PROJECT_SUMMARY_POLL_INTERVAL_SECONDS='5'
export SUPERMANAGER_PROJECT_MEMORY_EXTRACT_INTERVAL_SECONDS='60'
export SUPERMANAGER_PROJECT_MEMORY_CONSOLIDATE_INTERVAL_SECONDS='900'
export SUPERMANAGER_PROJECT_SKILLS_INTERVAL_SECONDS='900'
export SUPERMANAGER_ORGANIZATION_MEMORY_CONSOLIDATE_INTERVAL_SECONDS='1800'
export SUPERMANAGER_ORGANIZATION_SKILLS_INTERVAL_SECONDS='1800'
export CODEX_API_KEY='replace-me'
cargo run -- --database-url "$DATABASE_URL" --data-dir "$SUPERMANAGER_DATA_DIR"
```

The workflow worker reads these runtime config values:

- `DATABASE_URL`
- `SUPERMANAGER_DATA_DIR`
- `SUPERMANAGER_ORGANIZATION_SUMMARY_REFRESH_INTERVAL_SECONDS`
- `SUPERMANAGER_PROJECT_SUMMARY_POLL_INTERVAL_SECONDS`
- `SUPERMANAGER_PROJECT_MEMORY_EXTRACT_INTERVAL_SECONDS`
- `SUPERMANAGER_PROJECT_MEMORY_CONSOLIDATE_INTERVAL_SECONDS`
- `SUPERMANAGER_PROJECT_SKILLS_INTERVAL_SECONDS`
- `SUPERMANAGER_ORGANIZATION_MEMORY_CONSOLIDATE_INTERVAL_SECONDS`
- `SUPERMANAGER_ORGANIZATION_SKILLS_INTERVAL_SECONDS`
- `CODEX_API_KEY`

For production packaging, compile the server to a standalone Bun executable:

```sh
cd packages/server
bun run build
SUPERMANAGER_PUBLIC_API_URL='https://api.supermanager.dev' \
SUPERMANAGER_PUBLIC_APP_URL='https://supermanager.dev' \
./.build/supermanager-server
cd ../../
CARGO_PROFILE_RELEASE_LTO=true cargo build --release -p workflow-agent
SUPERMANAGER_DATA_DIR='/srv/supermanager' \
CODEX_API_KEY='replace-me' \
./target/release/workflow-agent
```

### 3. Start the frontend

```sh
cd packages/web
VITE_API_BASE_URL='http://127.0.0.1:8787' bun install
VITE_API_BASE_URL='http://127.0.0.1:8787' bun run dev
```

### 4. Install the CLI once per machine

From this repo:

```sh
cargo install --path crates/supermanager-cli
```

Or directly from Git:

```sh
curl -fsSL https://supermanager.dev/install.sh | sh
```

The installer downloads the latest GitHub Release for your platform, verifies the published SHA-256 checksum, and installs `supermanager` into `~/.local/bin` by default.

Published CLI installs also self-update in place. Normal interactive commands check once per day for a newer GitHub Release and install it automatically before continuing. To check manually at any time:

```sh
supermanager update --check
supermanager update
```

Set `SUPERMANAGER_AUTO_UPDATE=0` to disable the automatic daily check.

### 4. Sign in and create an organization

Open `http://127.0.0.1:5173`, continue with Google or GitHub, and create the private organization that will own your projects.

Then authenticate the CLI on any machine that will report repo activity:

```sh
supermanager login --server "http://127.0.0.1:8787"
```

Login is global. If your account belongs to multiple organizations, configure
the active organization after logging in:

```sh
supermanager orgs configure --server "http://127.0.0.1:8787"
```

To install the authenticated Supermanager MCP into your global Claude and
Codex configs:

```sh
supermanager mcp install
```

### 5. Create a project from the CLI

Create the project from inside a git repo:

```sh
supermanager create project --server "http://127.0.0.1:8787"
```

That uses the active organization and the current git repo name by default, joins the current repo automatically, prints a dashboard URL, and prints a join command for additional repos. To pick the project name explicitly:

```sh
supermanager create project "My Team" --server "http://127.0.0.1:8787"
```

### 6. Join more repos to the project

The repo where you ran `supermanager create project` is already connected. Run the join command inside each additional git repo you want connected:

```sh
supermanager join <project-id> --server "http://127.0.0.1:8787" --org "<org-slug>"
```

That command verifies org membership, mints a repo-scoped API key, installs repo-local Claude Code and Codex hooks for the current repo only, and stores the repo key machine-locally in `$HOME/.supermanager/repos.json`. Claude uses `.claude/settings.local.json`; Codex uses `.codex/hooks.json` and ensures `[features]` contains `codex_hooks = true` in `.codex/config.toml`.

To remove the repo from supermanager later:

```sh
supermanager leave
```

To list every project this machine is currently joined to:

```sh
supermanager list
```

To inspect or change the active organization from the CLI:

```sh
supermanager orgs list --server "http://127.0.0.1:8787"
supermanager orgs configure --server "http://127.0.0.1:8787"
supermanager orgs create --server "http://127.0.0.1:8787"
```

### 7. Use the dashboard

Open the workspace in the browser:

```sh
open "http://127.0.0.1:5173/app"
```

From there you can create project-scoped workspaces, generate organization invite links, approve CLI device logins, and open project dashboards at `/p/<project-id>`. Signed-out users are redirected back to login; wrong-org project access returns `403`.

## API

| Endpoint | Method | Description |
|---|---|---|
| `/api/auth/*` | various | Better Auth session, social OAuth, organization, device authorization, and API-key endpoints |
| `/health` | GET | Health check |
| `/v1/me` | GET | Signed-in user plus organization memberships |
| `/v1/projects` | GET | List projects for the selected organization |
| `/v1/projects` | POST | Create a project in the selected organization |
| `/v1/projects/{project_id}` | GET | Project metadata |
| `/v1/projects/{project_id}/feed` | GET | Raw project hook events, newest first |
| `/v1/projects/{project_id}/feed/stream` | GET | SSE stream of hook-event and summary-status events |
| `/v1/projects/{project_id}/connections` | POST | Mint a repo-scoped API key for the project |
| `/v1/hooks/turn` | POST | Submit a hook-captured turn event using `x-api-key` |
| `/v1/organizations/{organization_slug}/summary` | GET | Read the current org summary JSON (`bluf_markdown`, `projects[]`, `members[]`) plus status |
| `/v1/projects/{project_id}/summary` | GET | Read the current project summary (`bluf_markdown`, `detailed_summary_markdown`, `members[]`) |
| `/mcp` | POST | Streamable HTTP MCP endpoint with manager-facing read-only tools |

The MCP endpoint currently exposes these tools:

- `list_projects`
- `get_organization_summary`
- `get_project_summary`
- `get_project_feed`
- `query_events`
- `search_events`

## Project structure

```text
crates/
  reporter-protocol/      # Shared project and hook-event types
  workflow-agent/         # Rust workflow worker (summaries, memory, skills)
  supermanager-cli/       # Global CLI for joining/leaving repos
packages/
  common/                 # Shared TypeScript types (consumed by server + web)
  server/                 # Bun + TypeScript coordination server
  web/                    # React + Vite frontend
Dockerfile                # Production image
infra/aws/                # Terraform for the AWS backend
```

## Notes

- Workflow generation runs on the server after new hook turns arrive and on periodic timers. The current workflows are project summaries, organization summaries, organization memories, and organization skills.
- Stop-hook reports can embed a bounded transcript excerpt. The server stores that attachment separately from the hook event body so transcript-driven workflows can process it later without inflating the core event record.
- Durable workflow-agent state lives under `SUPERMANAGER_DATA_DIR`. The Bun server keeps a shared Codex home at `<data-dir>/codex`, and the Rust workflow agent keeps per-workflow thread state under `<data-dir>/workflow-threads/{project-summary|organization-summary|organization-memories|organization-skills}/<ID>/`.
- Organization memory and organization skills workflow documents are stored in PostgreSQL, not in the workflow-agent sandbox filesystem.
- The stored org summary is structured JSON. Summary workflows receive the current snapshot plus fresh updates and can return partial section updates instead of rewriting the whole summary each time.

## Licensing

Unless noted otherwise, the source in this repository is available under the MIT
license in `LICENSE`.

This repository also vendors `vendor/codex`, which remains available under the
Apache License 2.0 with its upstream notices preserved in `vendor/codex/LICENSE`
and `vendor/codex/NOTICE`. The top-level `NOTICE` file carries forward the
required Codex attribution for this distribution.

## Deploying the backend to AWS

This repo now deploys the backend as:

- ECR image
- ECS Fargate API service
- ECS Fargate summary worker service
- ALB on `https://api.supermanager.dev`
- RDS PostgreSQL
- EFS mounted at `/srv/supermanager` for durable summary-worker state
- Secrets Manager for `DATABASE_URL`, auth secrets, and `CODEX_API_KEY`

Files involved:

- `.github/workflows/deploy-server.yml`
- `infra/aws/**`

### Provision infrastructure

Apply the Terraform stack in `infra/aws` first. The companion guide is at `infra/aws/README.md`.

Key inputs:

- `acm_certificate_arn`
- `better_auth_secret_arn`
- `google_client_id_secret_arn`
- `google_client_secret_arn`
- `github_client_id_secret_arn`
- `github_client_secret_arn`
- `openai_api_key_secret_arn`
- optional `github_oidc_provider_arn` to create the deploy role

### GitHub configuration

Add these repository variables under `Settings -> Secrets and variables -> Actions -> Variables`:

- `AWS_REGION` from `aws_region`
- `AWS_DEPLOY_ROLE_ARN` from `github_actions_role_arn`
- `AWS_ECR_REPOSITORY` from `ecr_repository_name`
- `AWS_ECS_CLUSTER` from `ecs_cluster_name`
- `AWS_ECS_SERVICE` from `ecs_service_name`
- `AWS_ECS_SUMMARY_WORKER_SERVICE` from `ecs_summary_worker_service_name`

The deploy workflow runs only from `master`, uses GitHub OIDC with `aws-actions/configure-aws-credentials`, pushes the server image to ECR as `:latest`, rolls the API service first so it can apply migrations, then restarts the summary worker service against the same image.

The API task definition should be managed in Terraform and point at the ECR repository's `:latest` tag. The API runtime environment is still supplied there:

- `DATABASE_URL`
- `BETTER_AUTH_SECRET`
- `GOOGLE_CLIENT_ID`
- `GOOGLE_CLIENT_SECRET`
- `GITHUB_CLIENT_ID`
- `GITHUB_CLIENT_SECRET`
- `CODEX_API_KEY`
- `SUPERMANAGER_PUBLIC_API_URL=https://api.supermanager.dev`
- `SUPERMANAGER_PUBLIC_APP_URL=https://supermanager.dev`

The summary worker task definition mounts EFS and keeps the Codex runtime state:

- `DATABASE_URL`
- `CODEX_API_KEY`
- `SUPERMANAGER_DATA_DIR=/srv/supermanager`

The API service now uses rolling deploys with `desired_count = 1`, `deployment_minimum_healthy_percent = 100`, and `deployment_maximum_percent = 200`. Project summarization replays from Postgres using `project_summaries.last_processed_seq`, so the worker can be restarted independently without losing summary progress.

## Deploying the frontend to Cloudflare Pages

This repo includes a dedicated workflow for the React frontend:

- `.github/workflows/deploy-web.yml`

### GitHub configuration

Add these repository variables:

- `SUPERMANAGER_PUBLIC_API_URL`: public backend API origin used at frontend build time, for example `https://api.supermanager.dev`
- `CLOUDFLARE_PAGES_PROJECT_NAME`: Cloudflare Pages project name

Add these repository secrets:

- `CLOUDFLARE_API_TOKEN`
- `CLOUDFLARE_ACCOUNT_ID`

The workflow runs on pushes to `master` when `packages/web/**` or `packages/common/**` changes, builds the Vite app, and deploys `packages/web/dist` to Pages.

## CLI release distribution

`install.sh` is served from `packages/web/public/install.sh`, so the Pages deployment publishes it at `https://supermanager.dev/install.sh` once the custom domain points at the Pages project.

Tagging a version like `v0.2.0` triggers `.github/workflows/release-cli.yml`, which:

- builds release archives for macOS Apple Silicon, macOS Intel, and Linux x86_64
- generates `supermanager-checksums.txt`
- uploads the archives and checksums to the GitHub Release for that tag

The installer downloads from the release endpoint:

- `https://github.com/Sofianel5/supermanager/releases/latest/download/supermanager-<target>.tar.gz`
- `https://github.com/Sofianel5/supermanager/releases/latest/download/supermanager-checksums.txt`
