import { afterEach, describe, expect, test } from "bun:test";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { runMigrations } from "./migrations";

interface RecordedCall {
  text: string;
  values: unknown[];
}

type TaggedQuery = (
  strings: TemplateStringsArray,
  ...values: unknown[]
) => Promise<unknown>;

interface TransactionMock extends TaggedQuery {
  unsafe(sql: string): {
    simple(): Promise<void>;
  };
}

interface ClientMock extends TaggedQuery {
  begin(callback: (tx: TransactionMock) => Promise<void>): Promise<void>;
}

function renderSql(strings: TemplateStringsArray, values: unknown[]): string {
  let text = "";
  for (let index = 0; index < strings.length; index += 1) {
    text += strings[index];
    if (index < values.length) {
      text += `$${index + 1}`;
    }
  }
  return text.trim();
}

function createTaggedQueryHandler(handler: (call: RecordedCall) => Promise<unknown>) {
  return (strings: TemplateStringsArray, ...values: unknown[]) => {
    if (!Array.isArray(strings) || !("raw" in strings)) {
      throw new Error("expected tagged template call");
    }
    return handler({
      text: renderSql(strings, values),
      values,
    });
  };
}

describe("runMigrations", () => {
  const tempDirs: string[] = [];

  afterEach(() => {
    while (tempDirs.length > 0) {
      fs.rmSync(tempDirs.pop()!, { recursive: true, force: true });
    }
  });

  test("executes migration files via unsafe inside the transaction", async () => {
    const migrationsDir = fs.mkdtempSync(path.join(os.tmpdir(), "supermanager-migrations-"));
    tempDirs.push(migrationsDir);
    fs.writeFileSync(
      path.join(migrationsDir, "0001_test.sql"),
      "CREATE TABLE test_table (id TEXT PRIMARY KEY);\n",
    );

    const appliedRows = new Set<string>();
    const topLevelCalls: RecordedCall[] = [];
    const transactionCalls: RecordedCall[] = [];
    const unsafeCalls: string[] = [];

    const tx = createTaggedQueryHandler(async (call) => {
      transactionCalls.push(call);
      if (call.text.includes("INSERT INTO schema_migrations")) {
        appliedRows.add(String(call.values[0]));
      }
      return [];
    }) as unknown as TransactionMock;

    tx.unsafe = (sql: string) => ({
      async simple() {
        unsafeCalls.push(sql);
      },
    });

    const client = createTaggedQueryHandler(async (call) => {
      topLevelCalls.push(call);
      if (call.text.includes("SELECT filename")) {
        const filename = String(call.values[0]);
        return appliedRows.has(filename) ? [{ filename }] : [];
      }
      return [];
    }) as unknown as ClientMock;

    client.begin = async (callback: (tx: TransactionMock) => Promise<void>) => {
      await callback(tx);
    };

    await runMigrations(client as unknown as Bun.SQL, migrationsDir);

    expect(topLevelCalls.some((call) => call.text.includes("CREATE TABLE IF NOT EXISTS schema_migrations"))).toBe(true);
    expect(unsafeCalls).toEqual(["CREATE TABLE test_table (id TEXT PRIMARY KEY);\n"]);
    expect(
      transactionCalls.some((call) => call.text.includes("INSERT INTO schema_migrations")),
    ).toBe(true);
    expect(appliedRows.has("0001_test.sql")).toBe(true);
  });
});
