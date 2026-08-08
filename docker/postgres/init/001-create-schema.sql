-- sdkwork-api-cloud-gateway standalone compose (docker-compose.yml)
-- Creates the canonical workspace PostgreSQL schema used by the container.
-- The postgres image only creates the database from POSTGRES_DB; the
-- sdkwork-database lifecycle pins search_path to the same-named schema, so
-- the schema must exist before module migrations run.
CREATE SCHEMA IF NOT EXISTS sdkwork_ai_prod;
GRANT ALL ON SCHEMA sdkwork_ai_prod TO sdkwork_ai_prod;
