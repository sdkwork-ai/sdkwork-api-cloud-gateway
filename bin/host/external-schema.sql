-- sdkwork-api-cloud-gateway external-dependency mode (docker-compose.external.yml)
-- Run this against an EXISTING PostgreSQL instance before starting the gateway
-- in external mode (remote database or Ubuntu host-native PostgreSQL). The
-- gateway connects with SDKWORK_DATABASE_* to the canonical database
-- (`sdkwork_ai_test` / `sdkwork_ai_prod`); the sdkwork-database lifecycle pins
-- search_path to the same-named schema, so the database and its schema must
-- exist before module migrations run.
--
-- Applied automatically by bin/host/provision-external-databases.sh; run it by
-- hand only when provisioning a database out of band.
--
-- The script is parameterized with psql variables, so the same file serves
-- every environment: schema name = database name, owner = gateway user.
--
-- Example (as the postgres superuser; the vector extension requires pgvector
-- to be installed on the server, e.g. `postgresql-14-pgvector` on Ubuntu):
--   psql -h <host> -U <admin> -c "CREATE DATABASE sdkwork_ai_test;"
--   psql -h <host> -U <admin> -d sdkwork_ai_test -v db=sdkwork_ai_test \
--     -v app_user=sdkwork_ai_test -f bin/host/external-schema.sql
--
-- Optional psql variables (defaults match the built-in compose posture):
--   db         database name and schema name (default sdkwork_ai_prod)
--   app_user   gateway database role to grant schema privileges to
--              (default: same as db)
\if :{?db}
\else
\set db sdkwork_ai_prod
\endif
\if :{?app_user}
\else
\set app_user :db
\endif

-- Same-named schema (search_path target) and grants for the gateway role.
SELECT format('CREATE SCHEMA IF NOT EXISTS %I', :'db') \gexec

-- pgvector: the gateway pins search_path to the same-named schema without a
-- public fallback, so the vector type must live in that schema. ALTER keeps
-- upgrades from an older script (which installed the extension in public)
-- convergent; pgvector is relocatable. Superuser is required.
SELECT format('CREATE EXTENSION IF NOT EXISTS vector WITH SCHEMA %I', :'db') \gexec
SELECT format('ALTER EXTENSION vector SET SCHEMA %I', :'db') \gexec

SELECT format('GRANT ALL ON SCHEMA %I TO %I', :'db', :'app_user') \gexec
-- The gateway user also needs schema-level privileges for future migrations:
SELECT format('GRANT CREATE ON SCHEMA %I TO %I', :'db', :'app_user') \gexec
