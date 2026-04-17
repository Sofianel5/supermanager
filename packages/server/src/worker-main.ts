import { loadSummaryWorkerConfig } from "./config";
import { Db } from "./db";
import { StoragePaths } from "./storage";
import { SummaryAgentHost } from "./summary/agent-host";

async function main(): Promise<void> {
  const cwd = process.cwd();
  const config = await loadSummaryWorkerConfig(Bun.argv.slice(2), cwd);
  const db = await Db.connect(config.databaseUrl);
  const storage = new StoragePaths(config.dataDir);
  await storage.initialize();

  const agent = new SummaryAgentHost({
    config,
    db,
    storage,
  });
  await agent.start();

  let shutdownPromise: Promise<void> | null = null;
  const shutdown = (): Promise<void> => {
    if (!shutdownPromise) {
      shutdownPromise = (async () => {
        await agent.stop().catch(() => undefined);
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

  console.log("summary worker started");
  await new Promise<void>(() => undefined);
}

void main().catch((error) => {
  console.error(error);
  process.exit(1);
});
