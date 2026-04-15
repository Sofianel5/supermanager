import { readFileSync } from "node:fs";

import { betterAuth } from "better-auth";
import { getMigrations } from "better-auth/db/migration";
import { bearer, deviceAuthorization, organization } from "better-auth/plugins";
import { apiKey } from "@better-auth/api-key";
import { Kysely, PostgresDialect } from "kysely";
import { Pool } from "pg";

import { trimUrl, type ServerConfig } from "./config";

export const AUTH_BASE_PATH = "/api/auth";
export const CLI_DEVICE_CLIENT_ID = "supermanager-cli";
export const ROOM_CONNECTION_KEY_CONFIG = "room-connection";
const RDS_CA_BUNDLE_PATH = "/etc/ssl/certs/rds-global-bundle.pem";
export const HOOK_WRITE_PERMISSIONS: Record<string, string[]> = {
  hook: ["write"],
};

export type SupermanagerAuth = ReturnType<typeof createAuth>;

export interface AuthServices {
  auth: SupermanagerAuth;
  runMigrations(): Promise<void>;
  close(): Promise<void>;
}

export function createAuthServices(config: ServerConfig): AuthServices {
  const poolConfig = buildAuthPoolConfig(config.databaseUrl);
  const db = new Kysely<Record<string, never>>({
    dialect: new PostgresDialect({
      pool: new Pool({ ...poolConfig, max: 5 }),
    }),
  });
  const auth = createAuth(config, db);

  return {
    auth,
    async runMigrations() {
      const { runMigrations } = await getMigrations(auth.options);
      await runMigrations();
    },
    async close() {
      await db.destroy();
    },
  };
}

function buildAuthPoolConfig(databaseUrl: string) {
  const parsed = new URL(databaseUrl);
  if (!parsed.hostname.endsWith(".rds.amazonaws.com")) {
    return { connectionString: databaseUrl };
  }

  parsed.searchParams.delete("sslmode");

  return {
    connectionString: parsed.toString(),
    ssl: {
      ca: readFileSync(RDS_CA_BUNDLE_PATH, "utf8"),
      rejectUnauthorized: true,
    },
  };
}

function createAuth(config: ServerConfig, db: Kysely<Record<string, never>>) {
  const baseUrl = trimUrl(config.publicApiUrl);
  const appUrl = trimUrl(config.publicAppUrl);

  return betterAuth({
    baseURL: baseUrl,
    basePath: AUTH_BASE_PATH,
    secret: config.auth.secret,
    trustedOrigins: [baseUrl, appUrl],
    advanced: {
      useSecureCookies: baseUrl.startsWith("https://"),
    },
    database: {
      db,
      type: "postgres",
    },
    socialProviders: {
      google: {
        clientId: config.auth.google.clientId,
        clientSecret: config.auth.google.clientSecret,
      },
      github: {
        clientId: config.auth.github.clientId,
        clientSecret: config.auth.github.clientSecret,
      },
    },
    plugins: [
      organization({
        allowUserToCreateOrganization: true,
      }),
      bearer(),
      deviceAuthorization({
        validateClient: async (clientId) => clientId === CLI_DEVICE_CLIENT_ID,
        verificationUri: `${appUrl}/login`,
      }),
      apiKey([
        {
          apiKeyHeaders: "x-api-key",
          configId: ROOM_CONNECTION_KEY_CONFIG,
          defaultKeyLength: 48,
          defaultPrefix: "smrk_",
          enableMetadata: true,
          permissions: {
            defaultPermissions: HOOK_WRITE_PERMISSIONS,
          },
          rateLimit: {
            enabled: true,
            maxRequests: 100_000,
            timeWindow: 86_400_000,
          },
          references: "user",
          requireName: true,
        },
      ]),
    ],
  });
}
