import { mkdir, stat } from "node:fs/promises";
import path from "node:path";

export class StoragePaths {
  readonly dataDir: string;
  readonly codexHome: string;
  readonly roomsDir: string;

  constructor(dataDir: string) {
    this.dataDir = dataDir;
    this.codexHome = path.join(dataDir, "codex");
    this.roomsDir = path.join(dataDir, "rooms");
  }

  async initialize(): Promise<void> {
    await Promise.all(
      [this.dataDir, this.codexHome, this.roomsDir].map(async (target) => {
        await mkdir(target, { recursive: true });
      }),
    );
  }

  async checkReady(): Promise<void> {
    await Promise.all(
      [this.dataDir, this.codexHome, this.roomsDir].map(async (target) => {
        const details = await stat(target).catch(() => null);
        if (!details?.isDirectory()) {
          throw new Error(`storage dir missing or not a directory: ${target}`);
        }
      }),
    );
  }
}
