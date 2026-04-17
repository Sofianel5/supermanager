import { mkdir } from "node:fs/promises";
import path from "node:path";

export class StoragePaths {
  readonly dataDir: string;
  readonly codexHome: string;
  readonly summaryThreadsDir: string;

  constructor(dataDir: string) {
    this.dataDir = dataDir;
    this.codexHome = path.join(dataDir, "codex");
    this.summaryThreadsDir = path.join(dataDir, "summary-threads");
  }

  async initialize(): Promise<void> {
    await Promise.all(
      [this.dataDir, this.codexHome, this.summaryThreadsDir].map(async (target) => {
        await mkdir(target, { recursive: true });
      }),
    );
  }
}
