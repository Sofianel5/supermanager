import { createAuthServices } from "./auth";
import type { ServerConfig } from "./config";

const services = createAuthServices({
  auth: {
    secret:
      process.env.BETTER_AUTH_SECRET ??
      process.env.AUTH_SECRET ??
      "0123456789abcdef0123456789abcdef",
    google: {
      clientId: process.env.GOOGLE_CLIENT_ID ?? "google-client-id",
      clientSecret: process.env.GOOGLE_CLIENT_SECRET ?? "google-client-secret",
    },
    github: {
      clientId: process.env.GITHUB_CLIENT_ID ?? "github-client-id",
      clientSecret: process.env.GITHUB_CLIENT_SECRET ?? "github-client-secret",
    },
  },
  bind: { host: "127.0.0.1", port: 8787 },
  dataDir: process.env.SUPERMANAGER_DATA_DIR ?? "./.supermanager-data",
  databaseUrl:
    process.env.DATABASE_URL ?? "postgres://postgres:postgres@127.0.0.1:5432/postgres",
  publicApiUrl: process.env.SUPERMANAGER_PUBLIC_API_URL ?? "http://127.0.0.1:8787",
  publicAppUrl: process.env.SUPERMANAGER_PUBLIC_APP_URL ?? "http://127.0.0.1:5173",
  summaryAgent: {
    args: [],
    command: "summary-agent",
    cwd: process.cwd(),
  },
} satisfies ServerConfig);

export const auth = services.auth;
