-- sdkwork-api-cloud-gateway external-dependency mode (docker-compose.external.yml)
-- Run this against an EXISTING PostgreSQL instance before starting the gateway
-- in external mode. The gateway connects with SDKWORK_DATABASE_* to the
-- canonical database `sdkwork_ai_prod`; the sdkwork-database lifecycle pins
-- search_path to the same-named schema, so both must exist before module
-- migrations run.
--
-- Example (as the postgres superuser):
--   psql -h <host> -U <admin> -c "CREATE DATABASE sdkwork_ai_prod;"
--   psql -h <host> -U <admin> -d sdkwork_ai_prod -f docker/postgres/external-schema.sql
CREATE SCHEMA IF NOT EXISTS sdkwork_ai_prod;
GRANT ALL ON SCHEMA sdkwork_ai_prod TO sdkwork_ai_prod;
-- The gateway user also needs schema-level privileges for future migrations:
GRANT CREATE ON SCHEMA sdkwork_ai_prod TO sdkwork_ai_prod;
