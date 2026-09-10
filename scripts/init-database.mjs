#!/usr/bin/env node
/**
 * Unified database initialization for sdkwork-api-cloud-gateway deployments.
 *
 * Fresh installs and version upgrades need admin-level preparation before the
 * gateway can initialize its 46 embedded database modules:
 *   1. create the database and the business schema (idempotent),
 *   2. create PostgreSQL extensions that embedded modules require but business
 *      users cannot install (`vector`, `pg_trgm`, `zhparser`, `pg_bigm`),
 *   3. grant the business schema to the configured application user.
 *
 * The gateway itself then initializes every embedded module on first start
 * (`[database] autoMigrate = true` default): schema baseline, migrations, and
 * drift checks run automatically and idempotently, so after this script a
 * single `pnpm dev` / gateway start is enough to reach commercial readiness.
 *
 * Requires a local `psql` client and a superuser connection. Admin credentials
 * come from `SDKWORK_DATABASE_ADMIN_USERNAME` / `SDKWORK_DATABASE_ADMIN_PASSWORD`
 * (falling back to the application credentials), or from `PGPASSWORD`.
 *
 * Usage: node scripts/init-database.mjs
 */

import { execFileSync } from "node:child_process";
import { statSync } from "node:fs";

const env = process.env;

function envOr(key, fallback = "") {
  return env[key] ?? fallback;
}

function psqlArgs(database) {
  const args = [
    "-h",
    envOr("SDKWORK_DATABASE_HOST", "127.0.0.1"),
    "-p",
    envOr("SDKWORK_DATABASE_PORT", "5432"),
    "-U",
    envOr(
      "SDKWORK_DATABASE_ADMIN_USERNAME",
      envOr("SDKWORK_DATABASE_USERNAME"),
    ),
    "-d",
    database,
    "-v",
    "ON_ERROR_STOP=0",
  ];
  if (envOr("SDKWORK_DATABASE_ADMIN_PASSWORD", envOr("SDKWORK_DATABASE_PASSWORD"))) {
    args.push("-w");
  }
  return args;
}

function runPsql(database, sql, label) {
  try {
    execFileSync("psql", [...psqlArgs(database), "-tAc", sql], {
      stdio: ["ignore", "pipe", "inherit"],
      env: {
        ...env,
        PGPASSWORD:
          envOr("SDKWORK_DATABASE_ADMIN_PASSWORD", envOr("SDKWORK_DATABASE_PASSWORD")),
      },
    });
    console.log(`[init-database] ok: ${label}`);
  } catch (error) {
    console.error(`[init-database] skipped: ${label} (${error.message})`);
  }
}

const database = envOr("SDKWORK_DATABASE_NAME");
const schema = envOr("SDKWORK_DATABASE_SCHEMA", database);
const applicationUser = envOr("SDKWORK_DATABASE_USERNAME", database);

if (!database) {
  console.error("[init-database] SDKWORK_DATABASE_NAME is required");
  process.exit(1);
}
// These three values are interpolated into double-quoted SQL identifiers and
// single-quoted literals below, so anything outside a PostgreSQL identifier
// character set must be rejected up front (a stray quote would corrupt the
// statement rather than fail with a confusing syntax error).
const IDENTIFIER_PATTERN = /^[A-Za-z_][A-Za-z0-9_]*$/u;
for (const [label, value] of [["database", database], ["schema", schema], ["application user", applicationUser]]) {
  if (!IDENTIFIER_PATTERN.test(value) || value.length > 63) {
    console.error(
      `[init-database] ${label} must be a bare PostgreSQL identifier (letters, digits, _, <=63 chars): '${value}'`,
    );
    process.exit(1);
  }
}

console.log(`[init-database] database=${database} schema=${schema} user=${applicationUser}`);

// 1. Database (idempotent) — connect to the maintenance database.
runPsql("postgres", `SELECT 1 FROM pg_database WHERE datname = '${database}'`, "database exists");
try {
  execFileSync("psql", [...psqlArgs("postgres"), "-tAc", `CREATE DATABASE "${database}"`], {
    stdio: "inherit",
    env: {
      ...env,
      PGPASSWORD: envOr("SDKWORK_DATABASE_ADMIN_PASSWORD", envOr("SDKWORK_DATABASE_PASSWORD")),
    },
  });
  console.log(`[init-database] created database ${database}`);
} catch {
  console.log(`[init-database] database ${database} already exists`);
}

// 2. Schema (idempotent).
runPsql(database, `CREATE SCHEMA IF NOT EXISTS "${schema}"`, `schema ${schema}`);

// 3. Extensions required by embedded modules (best effort; a missing OS
//    extension package only blocks the module that needs it, which surfaces
//    at startup with a clear error). Extensions are created inside the
//    business schema so embedded module queries (whose search_path is the
//    business schema) can resolve their types.
for (const extension of ["vector", "pg_trgm", "zhparser", "pg_bigm"]) {
  runPsql(
    database,
    `CREATE EXTENSION IF NOT EXISTS ${extension} WITH SCHEMA "${schema}"`,
    `extension ${extension}`,
  );
}

// 4. Grant the business schema to the application user (idempotent).
runPsql(
  database,
  `GRANT USAGE, CREATE ON SCHEMA "${schema}" TO "${applicationUser}"`,
  `grant schema to ${applicationUser}`,
);
runPsql(
  database,
  `ALTER SCHEMA "${schema}" OWNER TO "${applicationUser}"`,
  `schema owner ${applicationUser}`,
);

// 5. sdkwork-models catalog schema (idempotent). The models module is not a
//    gateway embedded database module; its baseline DDL pre-creates the
//    Cloud Router catalog tables (`ai_model` and friends) so development and
//    test deployments pass the catalog schema check. Resolves the baseline
//    from the packaged install (database-modules/) first, then the sibling
//    repository for source checkouts.
const modelsBaseline = [
  "database-modules/sdkwork-models/database/ddl/baseline/postgres/0001_sdkwork-models_baseline.sql",
  "../../sdkwork-models/database/ddl/baseline/postgres/0001_sdkwork-models_baseline.sql",
].map((relative) => new URL(relative, import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1"));
const modelsSql = modelsBaseline.find((candidate) => {
  try {
    return statSync(candidate).isFile();
  } catch {
    return false;
  }
});
if (modelsSql) {
  try {
    execFileSync(
      "psql",
      [...psqlArgs(database), "-c", `SET search_path TO "${schema}", public`, "-v", "ON_ERROR_STOP=0", "-f", modelsSql],
      {
        stdio: ["ignore", "pipe", "inherit"],
        env: {
          ...env,
          PGPASSWORD:
            envOr("SDKWORK_DATABASE_ADMIN_PASSWORD", envOr("SDKWORK_DATABASE_PASSWORD")),
        },
      },
    );
    console.log("[init-database] ok: sdkwork-models catalog schema");
  } catch (error) {
    console.error(`[init-database] skipped: sdkwork-models catalog schema (${error.message})`);
  }
  runPsql(
    database,
    `GRANT ALL ON ALL TABLES IN SCHEMA "${schema}" TO "${applicationUser}"`,
    `grant catalog tables to ${applicationUser}`,
  );
  runPsql(
    database,
    `GRANT ALL ON ALL SEQUENCES IN SCHEMA "${schema}" TO "${applicationUser}"`,
    `grant catalog sequences to ${applicationUser}`,
  );
} else {
  console.warn("[init-database] sdkwork-models baseline not found; Cloud Router catalog schema check may fail in development/test");
}

console.log(
  `[init-database] done. Start the gateway (pnpm dev / the packaged binary) and it will\n` +
    `  initialize all embedded database modules automatically ([database] autoMigrate = true).\n` +
    `  Set SDKWORK_DATABASE_AUTO_MIGRATE=false only when schema lifecycle is managed explicitly.`,
);
