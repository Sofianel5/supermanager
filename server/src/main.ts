import path from "node:path";

import { createApp } from "./app";
import { createAuthServices } from "./auth";
import { loadConfig, trimUrl } from "./config";
import { Db } from "./db";
import { runAppMigrations } from "./migrations";
import { indexUnembeddedEvents } from "./search/store";
import { FeedStreamHub } from "./sse";
import { StoragePaths } from "./storage";
import { SummaryAgentHost } from "./summary/agent-host";

async function main(): Promise<void> {
  const cwd = process.cwd();
  const config = await loadConfig(Bun.argv.slice(2), cwd);
  const db = await Db.connect(config.databaseUrl);
  const auth = createAuthServices(config);
  await auth.runMigrations();
  await runAppMigrations(db.client, path.join(cwd, "migrations"));

  const storage = new StoragePaths(config.dataDir);
  await storage.initialize();
  const indexedEvents = await indexUnembeddedEvents(db);
  if (indexedEvents > 0) {
    console.log(`indexed ${indexedEvents} hook events for semantic search`);
  }

  const feedHub = new FeedStreamHub([
    trimUrl(config.publicApiUrl),
    trimUrl(config.publicAppUrl),
  ]);
  const agent = new SummaryAgentHost({
    config,
    db,
    feedHub,
    storage,
  });
  await agent.start();
  const app = createApp({
    auth: auth.auth,
    config,
    db,
    storage,
    agent,
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
        await agent.stop().catch(() => undefined);
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
}

void main().catch((error) => {
  console.error(error);
  process.exit(1);
});
