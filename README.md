# Supermanager

> Turn AI coding sessions into team context.

Supermanager turns Claude Code and Codex activity into a private shared workspace with live project feeds, fast summaries, durable team memory, and a read-only MCP for follow-up questions with evidence.

[Website](https://supermanager.dev) · [Setup docs](https://supermanager.dev/docs) · [AWS infra](./infra/aws/README.md)

Supermanager is currently a research preview.

## What teams get today

- See what your team is doing across Claude Code and Codex without asking everyone for another status update.
- Catch up fast with organization-wide and project-level summaries instead of reading raw transcripts or scrolling terminal history.
- Understand who is driving what through live project feeds, member views, and organization activity in one private workspace.
- Turn repeated work into shared memories and reusable skills so good patterns do not stay trapped in one person's session.
- Bring the same shared context into Claude and Codex through a read-only MCP when you want to ask follow-up questions with evidence.
- Connect repos quickly from the CLI by creating or joining projects, installing the reporting hooks, and syncing shared context back into local agent setups.
- Onboard teams with Google or GitHub sign-in, invite links, and browser-approved CLI logins instead of manual credential wrangling.
- Run against the hosted service or self-host the stack if you want control over the backend and data plane.

## How it works

1. Install the CLI on each machine that runs Claude Code or Codex.
2. Run `supermanager login` and approve the device in your browser.
3. From inside a repo, run `supermanager create project` or `supermanager join <project-id>`.
4. Supermanager installs repo-local hooks so Claude Code and Codex activity starts reporting automatically.
5. The web app shows the live feed while the workflow worker turns that activity into summaries, updates, memories, and skills.
6. `supermanager mcp install` exposes the same organization context to Claude and Codex over MCP.

## Hosted quickstart

### 1. Install the CLI

```sh
curl -fsSL https://supermanager.dev/install.sh | sh
```

The installer downloads the latest GitHub Release for your platform, verifies the published SHA-256 checksum, and installs `supermanager` into `~/.local/bin` by default.

### 2. Sign in

```sh
supermanager login
```

This starts a device login, opens the approval URL in your browser, and stores the authenticated session locally.

If your account belongs to more than one organization, pick the active one with:

```sh
supermanager orgs configure
```

### 3. Connect a repo to a project

From inside the repo you want to track:

```sh
supermanager create project
```

Or, if a teammate already created the project:

```sh
supermanager join <project-id>
```

`join` installs repo-local Claude Code and Codex hooks, stores the repo-scoped API key on the machine, and starts syncing shared context back into your agent setup.

### 4. Optional: install the MCP

```sh
supermanager mcp install
```

That installs the authenticated Supermanager MCP into your global Claude and Codex config so agents can query accessible projects, summaries, and history directly.

### 5. Useful follow-up commands

```sh
supermanager list
supermanager context sync
supermanager update --check
supermanager leave
```

Published installs also check once per day for a newer release before normal interactive commands run. Set `SUPERMANAGER_AUTO_UPDATE=0` to disable that behavior.

## Local development

### Prerequisites

- Bun `1.2.x`
- Rust toolchain
- PostgreSQL
- Google and GitHub OAuth apps configured for your local API and app origins
- `CODEX_API_KEY` for embeddings and workflow execution
- The `vendor/codex` submodule initialized

### 1. Initialize the repo

```sh
git submodule update --init --recursive
```

### 2. Export local environment variables

Use [`.env.example`](./.env.example) as the base reference. A minimal local setup looks like this:

```sh
export DATABASE_URL='postgres://supermanager:password@127.0.0.1:5432/supermanager?sslmode=disable'
export BETTER_AUTH_SECRET='replace-me'
export GOOGLE_CLIENT_ID='replace-me'
export GOOGLE_CLIENT_SECRET='replace-me'
export GITHUB_CLIENT_ID='replace-me'
export GITHUB_CLIENT_SECRET='replace-me'
export CODEX_API_KEY='replace-me'
export SUPERMANAGER_PUBLIC_API_URL='http://127.0.0.1:8787'
export SUPERMANAGER_PUBLIC_APP_URL='http://127.0.0.1:5173'
export SUPERMANAGER_DATA_DIR="$PWD/.supermanager-data"
```

### 3. Start the coordination server

```sh
cd packages/server
bun install
bun run dev
```

### 4. Start the workflow worker

From the repo root:

```sh
cargo run --manifest-path crates/workflow-agent/Cargo.toml -- \
  --database-url "$DATABASE_URL" \
  --data-dir "$SUPERMANAGER_DATA_DIR"
```

Optional refresh intervals are available through these environment variables:

- `SUPERMANAGER_ORGANIZATION_SUMMARY_REFRESH_INTERVAL_SECONDS`
- `SUPERMANAGER_PROJECT_SUMMARY_POLL_INTERVAL_SECONDS`
- `SUPERMANAGER_PROJECT_MEMORY_EXTRACT_INTERVAL_SECONDS`
- `SUPERMANAGER_PROJECT_MEMORY_CONSOLIDATE_INTERVAL_SECONDS`
- `SUPERMANAGER_PROJECT_SKILLS_INTERVAL_SECONDS`
- `SUPERMANAGER_ORGANIZATION_MEMORY_CONSOLIDATE_INTERVAL_SECONDS`
- `SUPERMANAGER_ORGANIZATION_SKILLS_INTERVAL_SECONDS`

### 5. Start the web app

```sh
cd packages/web
bun install
VITE_API_BASE_URL='http://127.0.0.1:8787' bun run dev
```

Open `http://127.0.0.1:5173`.

### 6. Install the CLI locally

From the repo root:

```sh
cargo install --path crates/supermanager-cli
```

Then authenticate the local CLI against the local server:

```sh
supermanager login --server "http://127.0.0.1:8787"
supermanager create project --server "http://127.0.0.1:8787"
```

## Repo layout

```text
crates/
  reporter-protocol/      # Shared Rust types for projects, summaries, and hook events
  supermanager-cli/       # CLI for auth, repo join/leave, context sync, and MCP install
  workflow-agent/         # Workflow worker that maintains summaries, memories, and skills
packages/
  common/                 # Shared TypeScript protocol types
  server/                 # Bun + Elysia API, auth, hook ingest, search, and MCP endpoint
  web/                    # React + Vite marketing site and authenticated workspace
infra/aws/                # Terraform for the AWS deployment
Dockerfile                # Backend image build
```

## License

The Supermanager source in this repository is available under MIT. See
[LICENSE](./LICENSE).

The vendored `vendor/codex` submodule remains under Apache License 2.0 with
its upstream notices preserved in [NOTICE](./NOTICE) and the files inside that
submodule.
