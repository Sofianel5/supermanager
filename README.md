# Supermanager

Supermanager is a room-based coordination system for coding agents. The coordination server is a Bun/TypeScript service that owns room creation, hook ingest, PostgreSQL storage, SSE, and agent orchestration. A separate Rust `summary-agent` process owns the in-process Codex runtime. A React frontend owns the landing page and live room dashboard.

## Setup

### 1. Start the coordination server

```sh
cd server
bun install
export DATABASE_URL='postgres://supermanager:password@127.0.0.1:5432/supermanager?sslmode=disable'
export SUPERMANAGER_DATA_DIR='../.supermanager-data'
bun run src/main.ts
```

By default it listens on `http://127.0.0.1:8787` and expects the frontend on `http://127.0.0.1:5173`.

To customize the public URLs explicitly:

```sh
bun run src/main.ts \
  --database-url 'postgres://supermanager:password@127.0.0.1:5432/supermanager?sslmode=disable' \
  --data-dir '../.supermanager-data' \
  --public-api-url 'http://127.0.0.1:8787' \
  --public-app-url 'http://127.0.0.1:5173'
```

You can also configure these through environment variables:

- `DATABASE_URL`
- `SUPERMANAGER_DATA_DIR`
- `SUPERMANAGER_PUBLIC_API_URL`
- `SUPERMANAGER_PUBLIC_APP_URL`
- `SUPERMANAGER_SUMMARY_AGENT_BIN`
- `OPENAI_API_KEY`

In local development the Bun server automatically starts the Rust summary agent through `cargo run -p summary-agent`. For packaged environments, point `SUPERMANAGER_SUMMARY_AGENT_BIN` at a compiled `summary-agent` binary.

### 2. Start the frontend

```sh
cd web
VITE_API_BASE_URL='http://127.0.0.1:8787' corepack pnpm install
VITE_API_BASE_URL='http://127.0.0.1:8787' corepack pnpm dev
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

### 4. Create a room

Create the room from the CLI:

```sh
supermanager create room
```

That uses the current git repo name as the room name by default, and it must be run inside a git repo. To pick one explicitly:

```sh
supermanager create room "My Team"
```

The command creates the room, automatically joins the current repo to it, prints the room code, a join command you can run in other repos, the dashboard URL, and copies the dashboard URL to your clipboard.

You can still create a room in the browser or call the API directly:

```sh
curl -sS http://127.0.0.1:8787/v1/rooms \
  -H 'Content-Type: application/json' \
  -d '{"name":"My Team"}'
```

The response includes:

- `dashboard_url`
- `room_id` as a 6-character case-insensitive alphanumeric code
- `join_command`

### 5. Join repos to the room

The repo where you ran `supermanager create room` is already connected. Run the join command inside each additional git repo you want connected:

```sh
supermanager join <room-code>
```

That command verifies the room exists, configures the repo hooks, prints the dashboard URL, and copies the dashboard URL to your clipboard.

For local development or custom deployments, override the API and app origins explicitly:

```sh
supermanager join <room-code> \
  --server "http://127.0.0.1:8787" \
  --app-url "http://127.0.0.1:5173"
```

That command installs repo-local Claude Code and Codex hooks for the current repo only. Claude uses `.claude/settings.local.json`; Codex uses `.codex/hooks.json` and ensures `[features]` contains `codex_hooks = true` in `.codex/config.toml`. Both hooks call the native `supermanager hook-report` subcommand, and room settings are stored machine-locally in `$HOME/.supermanager/repos.json`.

To remove the repo from supermanager later:

```sh
supermanager leave
```

To list every room this machine is currently joined to:

```sh
supermanager list
```

### 6. Use the dashboard

Open the room dashboard:

```sh
open "http://127.0.0.1:5173/r/<room-code>"
```

The frontend reads room metadata, feed, and a structured room summary from the API and watches summary generation status over SSE.

## API

| Endpoint | Method | Description |
|---|---|---|
| `/health` | GET | Health check |
| `/v1/rooms` | POST | Create a room |
| `/r/{room_id}` | GET | Room metadata |
| `/r/{room_id}/feed` | GET | Get raw room hook events, newest first |
| `/r/{room_id}/feed/stream` | GET | SSE stream of hook-event and summary-status events |
| `/r/{room_id}/hooks/turn` | POST | Submit a hook-captured turn event |
| `/r/{room_id}/summary` | GET | Read the current room summary JSON (`bluf_markdown`, `overview_markdown`, `employees[]`) |

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
- Secrets Manager for `DATABASE_URL` and `OPENAI_API_KEY`

Files involved:

- `.github/workflows/deploy-server.yml`
- `infra/aws/**`

### Provision infrastructure

Apply the Terraform stack in `infra/aws` first. The companion guide is at `infra/aws/README.md`.

Key inputs:

- `acm_certificate_arn`
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
- `OPENAI_API_KEY`
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
