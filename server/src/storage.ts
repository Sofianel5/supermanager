import { mkdir, stat } from "node:fs/promises";
import path from "node:path";

export class StoragePaths {
  readonly dataDir: string;
  readonly codexHome: string;
  readonly organizationsDir: string;

  constructor(dataDir: string) {
    this.dataDir = dataDir;
    this.codexHome = path.join(dataDir, "codex");
    this.organizationsDir = path.join(dataDir, "organizations");
  }

  async initialize(): Promise<void> {
    await Promise.all(
      [this.dataDir, this.codexHome, this.organizationsDir].map(async (target) => {
        await mkdir(target, { recursive: true });
      }),
    );
  }

  async checkReady(): Promise<void> {
    await Promise.all(
      [this.dataDir, this.codexHome, this.organizationsDir].map(async (target) => {
        const details = await stat(target).catch(() => null);
        if (!details?.isDirectory()) {
          throw new Error(`storage dir missing or not a directory: ${target}`);
        }
      }),
    );
  }
}
