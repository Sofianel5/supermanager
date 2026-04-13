import { betterAuth } from "better-auth";
import { bearer, deviceAuthorization, organization } from "better-auth/plugins";
import { apiKey } from "@better-auth/api-key";
import { Kysely, PostgresDialect } from "kysely";
import { Pool } from "pg";

import type { ServerConfig } from "./config";

export const AUTH_BASE_PATH = "/api/auth";
export const CLI_DEVICE_CLIENT_ID = "supermanager-cli";
export const ROOM_CONNECTION_KEY_CONFIG = "room-connection";
export const HOOK_WRITE_PERMISSIONS: Record<string, string[]> = {
  hook: ["write"],
};

export interface AuthServices {
  auth: SupermanagerAuth;
  close(): Promise<void>;
}

export type SupermanagerAuth = ReturnType<typeof createAuthInstance>;

export function createAuthServices(config: ServerConfig): AuthServices {
  const pool = new Pool({
    connectionString: config.databaseUrl,
    max: 10,
  });
  const db = new Kysely<Record<string, never>>({
    dialect: new PostgresDialect({ pool }),
  });
  const auth = createAuthInstance(config, db);

  return {
    auth,
    async close() {
      await db.destroy();
    },
  };
}

function createAuthInstance(
  config: ServerConfig,
  db: Kysely<Record<string, never>>,
) {
  return betterAuth({
    baseURL: trimUrl(config.publicApiUrl),
    basePath: AUTH_BASE_PATH,
    secret: config.auth.secret,
    trustedOrigins: [trimUrl(config.publicApiUrl), trimUrl(config.publicAppUrl)],
    advanced: {
      useSecureCookies: trimUrl(config.publicApiUrl).startsWith("https://"),
    },
    database: {
      db,
      type: "postgres",
      casing: "snake",
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
        verificationUri: `${trimUrl(config.publicAppUrl)}/device`,
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
          references: "organization",
          requireName: true,
        },
      ]),
    ],
  });
}

function trimUrl(url: string): string {
  return url.replace(/\/+$/, "");
}
