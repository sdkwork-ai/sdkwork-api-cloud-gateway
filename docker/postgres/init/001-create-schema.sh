#!/bin/sh
# sdkwork-api-cloud-gateway standalone compose (docker-compose.yml)
# Creates the canonical workspace PostgreSQL schema used by the container.
# The postgres image only creates the database from POSTGRES_DB; the
# sdkwork-database lifecycle pins search_path to the same-named schema, so
# the schema must exist before module migrations run.
#
# The schema name and owner follow the container environment (POSTGRES_DB /
# POSTGRES_USER), so the same init script serves every compose environment:
# test -> sdkwork_ai_test, production -> sdkwork_ai_prod (same-named schema).
psql -v ON_ERROR_STOP=1 --username "${POSTGRES_USER}" --dbname "${POSTGRES_DB}" <<-EOSQL
CREATE SCHEMA IF NOT EXISTS ${POSTGRES_DB};
GRANT ALL ON SCHEMA ${POSTGRES_DB} TO ${POSTGRES_USER};
EOSQL
