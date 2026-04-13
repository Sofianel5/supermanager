import path from "node:path";
import { parseArgs } from "node:util";

export interface BindAddress {
  host: string;
  port: number;
}

export interface SummaryAgentCommand {
  command: string;
  args: string[];
  cwd: string;
}

export interface OAuthProviderConfig {
  clientId: string;
  clientSecret: string;
}

export interface AuthConfig {
  secret: string;
  google: OAuthProviderConfig;
  github: OAuthProviderConfig;
}

export interface ServerConfig {
  bind: BindAddress;
  databaseUrl: string;
  dataDir: string;
  publicApiUrl: string;
  publicAppUrl: string;
  auth: AuthConfig;
  summaryAgent: SummaryAgentCommand;
}

export async function loadConfig(argv: string[], cwd: string): Promise<ServerConfig> {
  const parsed = parseArgs({
    args: argv,
    options: {
      bind: { type: "string", default: "127.0.0.1:8787" },
      "database-url": { type: "string" },
      "data-dir": { type: "string" },
      "public-api-url": { type: "string", default: "http://127.0.0.1:8787" },
      "public-app-url": { type: "string", default: "http://127.0.0.1:5173" },
      "summary-agent-bin": { type: "string" },
    },
    allowPositionals: false,
  });

  const databaseUrl = parsed.values["database-url"] ?? Bun.env.DATABASE_URL;
  const dataDir = parsed.values["data-dir"] ?? Bun.env.SUPERMANAGER_DATA_DIR;

  if (!databaseUrl) {
    throw new Error("missing required DATABASE_URL / --database-url");
  }
  if (!dataDir) {
    throw new Error("missing required SUPERMANAGER_DATA_DIR / --data-dir");
  }

  return {
    bind: parseBindAddress(parsed.values.bind),
    databaseUrl,
    dataDir,
    publicApiUrl:
      parsed.values["public-api-url"] ?? Bun.env.SUPERMANAGER_PUBLIC_API_URL ?? "http://127.0.0.1:8787",
    publicAppUrl:
      parsed.values["public-app-url"] ?? Bun.env.SUPERMANAGER_PUBLIC_APP_URL ?? "http://127.0.0.1:5173",
    auth: {
      secret: readRequiredEnv(["BETTER_AUTH_SECRET", "AUTH_SECRET"]),
      google: {
        clientId: readRequiredEnv(["GOOGLE_CLIENT_ID"]),
        clientSecret: readRequiredEnv(["GOOGLE_CLIENT_SECRET"]),
      },
      github: {
        clientId: readRequiredEnv(["GITHUB_CLIENT_ID"]),
        clientSecret: readRequiredEnv(["GITHUB_CLIENT_SECRET"]),
      },
    },
    summaryAgent: await resolveSummaryAgentCommand(
      parsed.values["summary-agent-bin"] ?? Bun.env.SUPERMANAGER_SUMMARY_AGENT_BIN ?? null,
      cwd,
    ),
  };
}

function readRequiredEnv(names: string[]): string {
  for (const name of names) {
    const value = Bun.env[name]?.trim();
    if (value) {
      return value;
    }
  }

  throw new Error(`missing required environment variable: ${names.join(" or ")}`);
}

function parseBindAddress(raw: string): BindAddress {
  const separator = raw.lastIndexOf(":");
  if (separator <= 0 || separator === raw.length - 1) {
    throw new Error(`invalid bind address: ${raw}`);
  }

  const host = raw.slice(0, separator);
  const port = Number.parseInt(raw.slice(separator + 1), 10);
  if (!Number.isInteger(port) || port <= 0 || port > 65535) {
    throw new Error(`invalid bind port: ${raw}`);
  }

  return { host, port };
}

async function resolveSummaryAgentCommand(
  explicitBinary: string | null,
  cwd: string,
): Promise<SummaryAgentCommand> {
  if (explicitBinary) {
    return {
      command: explicitBinary,
      args: [],
      cwd,
    };
  }

  const cargoWorkspace = await findCargoWorkspace(cwd);
  if (cargoWorkspace) {
    return {
      command: "cargo",
      args: ["run", "-q", "-p", "summary-agent", "--"],
      cwd: cargoWorkspace,
    };
  }

  return {
    command: "summary-agent",
    args: [],
    cwd,
  };
}

async function findCargoWorkspace(cwd: string): Promise<string | null> {
  const candidates = [cwd, path.resolve(cwd, "..")];
  for (const candidate of candidates) {
    if (await Bun.file(path.join(candidate, "Cargo.toml")).exists()) {
      return candidate;
    }
  }
  return null;
}
