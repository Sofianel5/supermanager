# Supermanager

Supermanager is an authenticated, organization-scoped coordination system for coding agents. The Bun/Elysia backend owns Better Auth, room storage, hook ingest, PostgreSQL state, SSE, and agent orchestration. A separate Rust `summary-agent` process owns the in-process Codex runtime. The React frontend owns the public sign-in page, organization workspace, device approval flow, invite acceptance, and live room dashboard.

## Setup

### 1. Start the coordination server

```sh
cd server
bun install
export DATABASE_URL='postgres://supermanager:password@127.0.0.1:5432/supermanager?sslmode=disable'
export SUPERMANAGER_DATA_DIR='../.supermanager-data'
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
- `SUPERMANAGER_DATA_DIR`
- `SUPERMANAGER_PUBLIC_API_URL`
- `SUPERMANAGER_PUBLIC_APP_URL`
- `SUPERMANAGER_SUMMARY_AGENT_BIN`
- `CODEX_API_KEY`

In local development the Bun server automatically starts the Rust summary agent through `cargo run -p summary-agent`. For packaged environments, point `SUPERMANAGER_SUMMARY_AGENT_BIN` at a compiled `summary-agent` binary.

For production packaging, compile the server to a standalone Bun executable:

```sh
cd server
bun run build
SUPERMANAGER_PUBLIC_API_URL='https://api.supermanager.dev' \
SUPERMANAGER_PUBLIC_APP_URL='https://supermanager.dev' \
./.build/supermanager-server
```

### 2. Start the frontend

```sh
cd web
VITE_API_BASE_URL='http://127.0.0.1:8787' bun install
VITE_API_BASE_URL='http://127.0.0.1:8787' bun run dev
```

### 3. Install the CLI once per machine

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

Open `http://127.0.0.1:5173`, continue with Google or GitHub, and create the private organization that will own your rooms.

Then authenticate the CLI on any machine that will report repo activity:

```sh
supermanager login --server "http://127.0.0.1:8787"
```

Login is global. If your account belongs to multiple organizations, configure
the active organization after logging in:

```sh
supermanager orgs configure --server "http://127.0.0.1:8787"
```

### 5. Create a room from the CLI

Create the room from inside a git repo:

```sh
supermanager create room --server "http://127.0.0.1:8787"
```

That uses the active organization and the current git repo name by default, joins the current repo automatically, prints a dashboard URL, and prints a join command for additional repos. To pick the room name explicitly:

```sh
supermanager create room "My Team" --server "http://127.0.0.1:8787"
```

### 6. Join more repos to the room

The repo where you ran `supermanager create room` is already connected. Run the join command inside each additional git repo you want connected:

```sh
supermanager join <room-id> --server "http://127.0.0.1:8787" --org "<org-slug>"
```

That command verifies org membership, mints a repo-scoped API key, installs repo-local Claude Code and Codex hooks for the current repo only, and stores the repo key machine-locally in `$HOME/.supermanager/repos.json`. Claude uses `.claude/settings.local.json`; Codex uses `.codex/hooks.json` and ensures `[features]` contains `codex_hooks = true` in `.codex/config.toml`.

To remove the repo from supermanager later:

```sh
supermanager leave
```

To list every room this machine is currently joined to:

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

From there you can create room-scoped workspaces, generate organization invite links, approve CLI device logins, and open room dashboards at `/r/<room-id>`. Signed-out users are redirected back to login; wrong-org room access returns `403`.

## API

| Endpoint | Method | Description |
|---|---|---|
| `/api/auth/*` | various | Better Auth session, social OAuth, organization, device authorization, and API-key endpoints |
| `/health` | GET | Health check |
| `/v1/me` | GET | Signed-in user plus organization memberships |
| `/v1/rooms` | GET | List rooms for the selected organization |
| `/v1/rooms` | POST | Create a room in the selected organization |
| `/v1/rooms/{room_id}` | GET | Room metadata |
| `/v1/rooms/{room_id}/feed` | GET | Raw room hook events, newest first |
| `/v1/rooms/{room_id}/feed/stream` | GET | SSE stream of hook-event and summary-status events |
| `/v1/rooms/{room_id}/connections` | POST | Mint a repo-scoped API key for the room |
| `/v1/hooks/turn` | POST | Submit a hook-captured turn event using `x-api-key` |
| `/v1/rooms/{room_id}/summary` | GET | Read the current room summary JSON (`bluf_markdown`, `overview_markdown`, `employees[]`) |
| `/mcp` | POST | Streamable HTTP MCP endpoint with manager-facing read-only tools |

The MCP endpoint currently exposes these tools:

- `list_rooms`
- `get_room_summary`
- `get_room_feed`
- `query_events`
- `search_events`

## Project structure

```text
crates/
  reporter-protocol/      # Shared room and hook-event types
  summary-agent/          # Rust Codex room summarizer
  supermanager-cli/       # Global CLI for joining/leaving repos
server/                   # Bun + TypeScript coordination server
web/                      # React + Vite frontend
Dockerfile                # Production image
infra/aws/                # Terraform for the AWS backend
```

## Notes

- Summary generation runs on the server after new hook turns arrive.
- Durable summary-agent state lives under `SUPERMANAGER_DATA_DIR`. The Bun server keeps a shared Codex home at `<data-dir>/codex`, and the Rust summary agent keeps per-room working directories and thread state under `<data-dir>/rooms/<ROOM_ID>/`.
- The stored room summary is structured JSON. The model receives the current summary plus fresh updates and can return partial section updates instead of rewriting the whole room summary each time.

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
- ECS Fargate service
- ALB on `https://api.supermanager.dev`
- RDS PostgreSQL
- EFS mounted at `/srv/supermanager` for durable room-agent state
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

The deploy workflow runs only from `master`, uses GitHub OIDC with `aws-actions/configure-aws-credentials`, pushes the server image to ECR as `:latest`, then forces a new ECS deployment so the service pulls that tag.

The ECS task definition should be managed in Terraform and point at the ECR repository's `:latest` tag. The backend runtime environment is still supplied there:

- `DATABASE_URL`
- `BETTER_AUTH_SECRET`
- `GOOGLE_CLIENT_ID`
- `GOOGLE_CLIENT_SECRET`
- `GITHUB_CLIENT_ID`
- `GITHUB_CLIENT_SECRET`
- `CODEX_API_KEY`
- `SUPERMANAGER_DATA_DIR=/srv/supermanager`
- `SUPERMANAGER_PUBLIC_API_URL=https://api.supermanager.dev`
- `SUPERMANAGER_PUBLIC_APP_URL=https://supermanager.dev`
- `SUPERMANAGER_SUMMARY_AGENT_BIN=/usr/local/bin/summary-agent`

The ECS service is intentionally single-writer during deploys: `desired_count = 1`, `deployment_minimum_healthy_percent = 0`, and `deployment_maximum_percent = 100`. That allows the durable Codex state on EFS to survive task replacement cleanly.

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

The workflow runs on pushes to `master` when `web/**` changes, builds the Vite app, and deploys `web/dist` to Pages.

## CLI release distribution

`install.sh` is served from `web/public/install.sh`, so the Pages deployment publishes it at `https://supermanager.dev/install.sh` once the custom domain points at the Pages project.

Tagging a version like `v0.2.0` triggers `.github/workflows/release-cli.yml`, which:

- builds release archives for macOS Apple Silicon, macOS Intel, and Linux x86_64
- generates `supermanager-checksums.txt`
- uploads the archives and checksums to the GitHub Release for that tag

The installer downloads from the release endpoint:

- `https://github.com/Sofianel5/supermanager/releases/latest/download/supermanager-<target>.tar.gz`
- `https://github.com/Sofianel5/supermanager/releases/latest/download/supermanager-checksums.txt`
