import path from "node:path";

import { createApp } from "./app";
import { createAuthServices } from "./auth";
import { loadApiConfig, trimUrl } from "./config";
import { Db } from "./db";
import { runAppMigrations } from "./migrations";
import { indexUnembeddedEvents } from "./search/store";
import { FeedStreamHub } from "./sse";

async function main(): Promise<void> {
  const cwd = process.cwd();
  const config = await loadApiConfig(Bun.argv.slice(2), cwd);
  const db = await Db.connect(config.databaseUrl);
  const auth = createAuthServices(config);
  await auth.runMigrations();
  await runAppMigrations(db.client, path.join(cwd, "migrations"));

  const feedHub = new FeedStreamHub([
    trimUrl(config.publicApiUrl),
    trimUrl(config.publicAppUrl),
  ]);
  const app = createApp({
    auth: auth.auth,
    config,
    db,
    feedHub,
  });
  app.compile();

  const server = Bun.serve({
    hostname: config.bind.host,
    idleTimeout: 0,
    port: config.bind.port,
    fetch: app.handle,
  });

  let shutdownPromise: Promise<void> | null = null;
  const shutdown = (): Promise<void> => {
    if (!shutdownPromise) {
      shutdownPromise = (async () => {
        await server.stop(true).catch(() => undefined);
        await auth.close().catch(() => undefined);
        await db.close().catch(() => undefined);
      })();
    }
    return shutdownPromise;
  };

  process.once("SIGINT", () => {
    void shutdown().finally(() => process.exit(0));
  });
  process.once("SIGTERM", () => {
    void shutdown().finally(() => process.exit(0));
  });

  console.log(`coordination-server listening on ${server.url}`);
  void indexUnembeddedEvents(db)
    .then((indexedEvents) => {
      if (indexedEvents > 0) {
        console.log(`indexed ${indexedEvents} hook events for semantic search`);
      }
    })
    .catch((error) => {
      console.error(
        `[search] failed to index pending hook events: ${formatError(error)}`,
      );
    });
}

void main().catch((error) => {
  console.error(error);
  process.exit(1);
});

function formatError(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}
