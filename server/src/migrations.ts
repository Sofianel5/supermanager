import path from "node:path";

interface MigrationRow {
  filename: string;
}

export async function runAppMigrations(client: Bun.SQL, migrationsDir: string): Promise<void> {
  await client`
    CREATE TABLE IF NOT EXISTS app_migrations (
      filename TEXT PRIMARY KEY,
      applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
    )
  `;

  const files = Array.from(new Bun.Glob("*.sql").scanSync({ cwd: migrationsDir })).sort(
    (left, right) => left.localeCompare(right),
  );

  for (const filename of files) {
    const alreadyApplied = await client<MigrationRow[]>`
      SELECT filename
      FROM app_migrations
      WHERE filename = ${filename}
    `;
    if (alreadyApplied.length > 0) {
      continue;
    }

    const migrationSql = await Bun.file(path.join(migrationsDir, filename)).text();
    await client.begin(async (tx) => {
      await tx.unsafe(migrationSql).simple();
      await tx`
        INSERT INTO app_migrations (filename)
        VALUES (${filename})
      `;
    });
  }
}
