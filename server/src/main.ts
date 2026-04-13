import path from "node:path";

import { createFetchHandler } from "./app.js";
import { loadConfig } from "./config.js";
import { Db } from "./db.js";
import { runMigrations } from "./migrations.js";
import { FeedStreamHub } from "./sse.js";
import { StoragePaths } from "./storage.js";
import { SummaryAgentHost } from "./summary/agent-host.js";

async function main(): Promise<void> {
  const cwd = process.cwd();
  const config = await loadConfig(process.argv.slice(2), cwd);
  const db = await Db.connect(config.databaseUrl);
  await runMigrations(db.client, path.join(cwd, "migrations"));

  const storage = new StoragePaths(config.dataDir);
  await storage.initialize();

  const feedHub = new FeedStreamHub();
  const agent = new SummaryAgentHost({
    config,
    db,
    feedHub,
    storage,
  });
  await agent.start();

  const shutdown = async (): Promise<void> => {
    await agent.stop().catch(() => undefined);
    await db.close().catch(() => undefined);
  };
  process.once("SIGINT", () => {
    void shutdown().finally(() => process.exit(0));
  });
  process.once("SIGTERM", () => {
    void shutdown().finally(() => process.exit(0));
  });

  Bun.serve({
    hostname: config.bind.host,
    port: config.bind.port,
    fetch: createFetchHandler({
      config,
      db,
      storage,
      agent,
      feedHub,
    }),
  });
  console.log(`coordination-server listening on http://${config.bind.host}:${config.bind.port}`);
}

void main().catch((error) => {
  console.error(error);
  process.exit(1);
});
