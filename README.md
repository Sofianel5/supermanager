# Supermanager

Supermanager is a room-based coordination system for coding agents. The Rust server owns room creation, hook ingest, SQLite storage, SSE, and summary generation. A separate React frontend owns the landing page and live room dashboard.

## Setup

### 1. Start the server API

```sh
cargo run -p coordination-server
```

By default it listens on `http://127.0.0.1:8787`, expects the frontend on `http://127.0.0.1:5173`, and writes to `supermanager.db`.

To customize the public URLs and one-time CLI install command:

```sh
cargo run -p coordination-server -- \
  --public-api-url 'http://127.0.0.1:8787' \
  --public-app-url 'http://127.0.0.1:5173' \
  --cli-install-command 'cargo install --git https://github.com/your-org/supermanager.git supermanager'
```

You can also configure these through environment variables:

- `SUPERMANAGER_PUBLIC_API_URL`
- `SUPERMANAGER_PUBLIC_APP_URL`
- `SUPERMANAGER_CLI_INSTALL_COMMAND`

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
cargo install --git https://github.com/Sofianel5/supermanager.git supermanager
```

### 4. Create a room

Create the room from the CLI:

```sh
supermanager create room
```

That uses the current repo or directory name as the room name by default. To pick one explicitly:

```sh
supermanager create room "My Team"
```

The command prints the room code, the join command, the dashboard URL, and copies the dashboard URL to your clipboard.

You can still create a room in the browser or call the API directly:

```sh
curl -sS http://127.0.0.1:8787/v1/rooms \
  -H 'Content-Type: application/json' \
  -d '{"name":"My Team"}'
```

The response includes:

- `install_command`
- `dashboard_url`
- `room_id` as a 6-character case-insensitive alphanumeric code
- `join_command`

### 5. Join repos to the room

Run the join command inside each repo you want connected:

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

### 6. Use the dashboard

Open the room dashboard:

```sh
open "http://127.0.0.1:5173/r/<room-code>"
```

The frontend reads room metadata, feed, and summary from the API and watches summary generation status over SSE.

## API

| Endpoint | Method | Description |
|---|---|---|
| `/config` | GET | Public frontend bootstrap config |
| `/health` | GET | Health check |
| `/v1/rooms` | POST | Create a room |
| `/r/{room_id}` | GET | Room metadata |
| `/r/{room_id}/feed` | GET | Get raw room hook events, newest first |
| `/r/{room_id}/feed/stream` | GET | SSE stream of hook-event and summary-status events |
| `/r/{room_id}/hooks/turn` | POST | Submit a hook-captured turn event |
| `/r/{room_id}/summary` | GET | Read the current room summary |

## Project structure

```text
crates/
  coordination-server/    # HTTP server, APIs, summaries, SSE
  reporter-protocol/      # Shared room and hook-event types
  supermanager-cli/       # Global CLI for joining/leaving repos
web/                      # React + Vite frontend
Dockerfile                # Production image
fly.toml                  # Fly deployment config
```

## Notes

- Summary generation runs on the server after new hook turns arrive.

## Deploying to Fly with GitHub Actions and AWS CodeBuild

This repo includes a deployment path where GitHub Actions starts an AWS CodeBuild project, and CodeBuild runs `flyctl deploy --remote-only` against the checked-out commit.

Files involved:

- `.github/workflows/deploy-server.yml`
- `buildspec.deploy.yml`
- `scripts/deploy-fly.sh`

### GitHub configuration

Add these repository variables under `Settings -> Secrets and variables -> Actions -> Variables`:

- `AWS_REGION`: AWS region that hosts the CodeBuild project, for example `us-west-2`
- `AWS_CODEBUILD_PROJECT_NAME`: existing CodeBuild project name that should run the deploy
- `AWS_DEPLOY_ROLE_ARN`: IAM role ARN that GitHub Actions assumes through OIDC

The workflow runs on pushes to `master` when server deployment files change, and it also supports manual `workflow_dispatch`.

### AWS configuration

Create a CodeBuild project that points at this repository and make sure its service role can access the repository source. Configure the project with:

- a Linux image that includes `bash` and `curl`
- `buildspec.deploy.yml` allowed via buildspec override
- an environment variable named `FLY_ACCESS_TOKEN` containing a Fly deploy token for the `supermanager` app

The GitHub Actions assumed role needs permission to start the CodeBuild project and read its logs:

- `codebuild:StartBuild`
- `codebuild:BatchGetBuilds`
- `logs:GetLogEvents`

The workflow uses `aws-actions/configure-aws-credentials` with GitHub OIDC, so no long-lived AWS keys are required in GitHub.

Set the backend runtime environment on Fly so the API can generate correct dashboard links and join commands:

- `SUPERMANAGER_PUBLIC_API_URL`
- `SUPERMANAGER_PUBLIC_APP_URL`
- `OPENAI_API_KEY`

## Deploying the frontend to Cloudflare Pages

This repo includes a dedicated workflow for the React frontend:

- `.github/workflows/deploy-web.yml`

### GitHub configuration

Add these repository variables:

- `SUPERMANAGER_PUBLIC_API_URL`: public backend API origin used at frontend build time
- `CLOUDFLARE_PAGES_PROJECT_NAME`: Cloudflare Pages project name

Add these repository secrets:

- `CLOUDFLARE_API_TOKEN`
- `CLOUDFLARE_ACCOUNT_ID`

The workflow runs on pushes to `master` when `web/**` changes, builds the Vite app, and deploys `web/dist` to Pages.
