import { parseArgs } from "node:util";

export interface BindAddress {
  host: string;
  port: number;
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

export interface ApiConfig {
  bind: BindAddress;
  databaseUrl: string;
  publicApiUrl: string;
  publicAppUrl: string;
  auth: AuthConfig;
}

export function loadApiConfig(argv: string[]): ApiConfig {
  const parsed = parseArgs({
    args: argv,
    options: {
      bind: { type: "string", default: "127.0.0.1:8787" },
      "database-url": { type: "string" },
    },
    allowPositionals: false,
  });

  const databaseUrl = parsed.values["database-url"] ?? Bun.env.DATABASE_URL;

  if (!databaseUrl) {
    throw new Error("missing required DATABASE_URL / --database-url");
  }

  return {
    bind: parseBindAddress(parsed.values.bind),
    databaseUrl,
    publicApiUrl: readRequiredEnv(["SUPERMANAGER_PUBLIC_API_URL"]),
    publicAppUrl: readRequiredEnv(["SUPERMANAGER_PUBLIC_APP_URL"]),
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
  };
}

function readRequiredEnv(names: string[]): string {
  for (const name of names) {
    const value = Bun.env[name]?.trim();
    if (value) {
      return trimUrl(value);
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

export function trimUrl(url: string): string {
  return url.replace(/\/+$/, "");
}
