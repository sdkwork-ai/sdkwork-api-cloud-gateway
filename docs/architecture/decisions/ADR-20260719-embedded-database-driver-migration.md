# ADR-20260719: Embedded Database Driver Migration

- Status: temporary exception
- Canonical standard: `../../../sdkwork-specs/DATABASE_SPEC_PROCESS_SHARED_POOL.md`
- Owners: sdkwork-drive and sdkwork-im maintainers
- Removal milestone: before the next API Cloud Gateway production release

The gateway installs one canonical `sqlx::PgPool` before IAM and optional foundation bootstrap. IAM, Commerce, and every foundation module using `sdkwork-database-sqlx` reuse that handle when their normalized database identity matches.

Drive still embeds one process-singleton `sqlx::AnyPool`; IM still contains an SQLx lifecycle pool plus an r2d2 synchronous pool. These are declared migration exceptions, not single-pool compliance. Every enabled module must target `sdkwork_ai_dev` and schema `sdkwork_ai_dev`; an identity mismatch fails gateway startup.

Drive's compatibility pool is admitted only when `SDKWORK_DATABASE_TEMPORARY_ANY_POOL_EXCEPTION=true`; the shared database framework validates its identity against the installed canonical pool before opening it.

The gateway declares two temporary driver pools through
`SDKWORK_DATABASE_TEMPORARY_DRIVER_POOL_COUNT=2`. With the development process maximum of 10, the
framework reserves at most 4 connections for canonical SQLx, 3 for Drive AnyPool, and 3 for IM
r2d2. The embedded modules cannot enlarge these allocations.

The exceptions are removed when Drive and IM consume the canonical PostgreSQL handle, their process contracts no longer list alternate drivers, and live gateway startup/shutdown evidence shows only the canonical process pool.
