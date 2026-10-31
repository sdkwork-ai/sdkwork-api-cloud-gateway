//! Installer-time PostgreSQL provisioning for the platform gateway.
//!
//! `DATABASE_FRAMEWORK_SPEC.md` §4.1 assigns role/database/schema provision to
//! the workspace bootstrap phase, while §4.4 keeps table lifecycle with each
//! module owner. Development orchestration already runs that bootstrap before
//! `pnpm dev`, but the installed container/systemd paths only ran
//! `--migrate-databases`; a fresh production database therefore failed before
//! the first module migration, and only the development environment reached a
//! fully initialized schema.
//!
//! The explicit `--provision-databases` command closes that gap for every
//! lifecycle environment. It is idempotent, never runs on the serve path, and
//! converges:
//!
//! 1. the application role (`SDKWORK_DATABASE_*`),
//! 2. the database (owned by that role),
//! 3. the business schema and its grants,
//! 4. the extensions embedded modules require (`vector`, `pg_trgm`, and the
//!    optional `zhparser` / `pg_bigm` search extensions).
//!
//! Admin credentials come from the standard `SDKWORK_DATABASE_ADMIN_*` family
//! (`ENVIRONMENT_SPEC.md` §7.1). Without them the application credentials are
//! used, which is sufficient when the role already owns the database; a fresh
//! database then fails closed with the exact remediation instead of leaving the
//! operator with an opaque first-migration error.

use std::{
    collections::HashMap,
    fmt, fs,
    future::Future,
    path::{Path, PathBuf},
    str::FromStr,
    time::{Duration, Instant},
};

use sdkwork_api_cloud_gateway_config::GatewayRuntimeConfig;
use sqlx::{
    postgres::{PgConnectOptions, PgConnection, PgSslMode},
    Connection,
};

use crate::GatewayRuntimeError;

const DEFAULT_ADMIN_DATABASE: &str = "postgres";
const DEFAULT_DATABASE_PORT: u16 = 5432;
const DEFAULT_CONNECT_TIMEOUT_SECONDS: u64 = 60;
const CONNECT_RETRY_INTERVAL: Duration = Duration::from_secs(2);
const CONNECT_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(10);
const STATEMENT_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_IDENTIFIER_LENGTH: usize = 63;

const PROVISION_ENV_KEY: &str = "SDKWORK_DATABASE_PROVISION";
const CONNECT_TIMEOUT_ENV_KEY: &str = "SDKWORK_DATABASE_PROVISION_TIMEOUT_SECONDS";
const ADMIN_URL_ENV_KEY: &str = "SDKWORK_DATABASE_ADMIN_URL";

/// Extensions every deployment attempts; `required` is decided from the
/// composed dependency surfaces so a deployment without the owning module is
/// never blocked by an unavailable search extension package.
const EXTENSIONS: [(&str, &str); 4] = [
    ("vector", "sdkwork-knowledgebase"),
    ("pg_trgm", "sdkwork-search"),
    ("zhparser", ""),
    ("pg_bigm", ""),
];

/// Result of one provisioning run. Reported as a structured log line by the
/// explicit lifecycle command and asserted by unit tests.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ProvisioningReport {
    /// Role was created by this run.
    pub role_created: bool,
    /// Database was created by this run.
    pub database_created: bool,
    /// Business schema was created by this run.
    pub schema_created: bool,
    /// Extensions created or relocated by this run.
    pub extensions_converged: Vec<String>,
    /// Non-fatal provisioning gaps (optional extensions, non-admin grants).
    pub warnings: usize,
}

/// Returns whether the explicit provisioning command should run. Defaults to
/// `true`; operators that own schema lifecycle out of band can set
/// `SDKWORK_DATABASE_PROVISION=false` in the deployment env file.
fn provisioning_enabled(env: &HashMap<String, String>) -> bool {
    env.get(PROVISION_ENV_KEY)
        .map(|value| !value.trim().eq_ignore_ascii_case("false"))
        .unwrap_or(true)
}

/// Provision role, database, schema, grants, and extensions for the configured
/// workspace identity.
///
/// This function must only be called by the explicit `--provision-databases`
/// command (installer/pre-start orchestration). The serve path must not create
/// database objects (`DATABASE_FRAMEWORK_SPEC.md` §4.4.1).
pub async fn provision_databases(
    config: &GatewayRuntimeConfig,
) -> Result<ProvisioningReport, GatewayRuntimeError> {
    let env = std::env::vars().collect::<HashMap<_, _>>();
    if !provisioning_enabled(&env) {
        tracing::info!(
            target: "sdkwork_api_cloud_gateway::database_provisioning",
            "{PROVISION_ENV_KEY}=false; skipping workspace database provisioning"
        );
        return Ok(ProvisioningReport::default());
    }

    let settings = ProvisioningSettings::from_env(&env)?;
    settings.provision(config).await
}

#[derive(Clone)]
struct ConnectionTarget {
    host: String,
    port: u16,
    database: String,
    username: String,
    password: Option<String>,
    url: Option<String>,
    ssl_mode: PgSslMode,
    ssl_root_cert: Option<PathBuf>,
}

impl ConnectionTarget {
    fn connect_options(
        &self,
        database_override: Option<&str>,
    ) -> Result<PgConnectOptions, GatewayRuntimeError> {
        if let Some(url) = self.url.as_deref().filter(|value| !value.trim().is_empty()) {
            let mut options = PgConnectOptions::from_str(url).map_err(|error| {
                provisioning_error(
                    "parse-connection-url",
                    format!("configured PostgreSQL URL is invalid: {error}"),
                )
            })?;
            if let Some(database) = database_override {
                options = options.database(database);
            }
            options = options.ssl_mode(self.ssl_mode);
            if let Some(cert) = &self.ssl_root_cert {
                options = options.ssl_root_cert(cert);
            }
            return Ok(options);
        }

        let mut options = PgConnectOptions::new()
            .host(&self.host)
            .port(self.port)
            .username(&self.username)
            .database(database_override.unwrap_or(&self.database))
            .ssl_mode(self.ssl_mode);
        if let Some(password) = self.password.as_deref() {
            options = options.password(password);
        }
        if let Some(cert) = &self.ssl_root_cert {
            options = options.ssl_root_cert(cert);
        }
        Ok(options)
    }
}

#[derive(Clone)]
struct ProvisioningSettings {
    admin: ConnectionTarget,
    target: ConnectionTarget,
    schema: String,
    /// True when an explicit admin connection is configured; it is used for
    /// the target-database connection even when it names the application role.
    admin_configured: bool,
    /// True when the admin identity differs from the application identity;
    /// only then may role/database ownership and default privileges be altered
    /// authoritatively.
    admin_is_distinct: bool,
    connect_timeout: Duration,
}

impl fmt::Debug for ProvisioningSettings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Never render credentials: this type is Debug-required by test
        // diagnostics and may be logged by future callers.
        formatter
            .debug_struct("ProvisioningSettings")
            .field("database", &self.target.database)
            .field("schema", &self.schema)
            .field("username", &self.target.username)
            .field("admin_configured", &self.admin_configured)
            .field("admin_is_distinct", &self.admin_is_distinct)
            .finish_non_exhaustive()
    }
}

impl ProvisioningSettings {
    fn from_env(env: &HashMap<String, String>) -> Result<Self, GatewayRuntimeError> {
        // Host-native installs keep the PostgreSQL profile in the workspace
        // database config directory (`/etc/sdkwork/database`), not in process
        // env. Resolve the canonical URL through sdkwork-database-config and
        // derive missing identity fields from it, so provisioning behaves the
        // same for container env vars and installed TOML/env profiles.
        let explicit_url = optional_value(env, "SDKWORK_DATABASE_URL");
        let config_dir_configured = optional_value(env, "SDKWORK_DATABASE_CONFIG_DIR").is_some();
        let resolved_url = if explicit_url.is_none() && config_dir_configured {
            match sdkwork_database_config::workspace_database::resolve_workspace_database_url_for(
                Some("CLOUD"),
            ) {
                Ok(url) => Some(url),
                Err(error) if optional_value(env, "SDKWORK_DATABASE_NAME").is_none() => {
                    return Err(provisioning_error(
                        "resolve database profile",
                        error.to_string(),
                    ));
                }
                Err(_) => None,
            }
        } else {
            None
        };
        let url = explicit_url.or(resolved_url);
        let parsed = match url.as_deref() {
            Some(value) => Some(parse_database_url(value)?),
            None => None,
        };

        let database = optional_identifier(env, "SDKWORK_DATABASE_NAME")?
            .or_else(|| parsed.as_ref().and_then(|value| value.database.clone()))
            .ok_or_else(|| {
                provisioning_error(
                    "validate configuration",
                    "SDKWORK_DATABASE_NAME is required for workspace database provisioning; \
                     use the unified SDKWORK_DATABASE_* split fields (ENVIRONMENT_SPEC.md §7.1) \
                     or configure SDKWORK_DATABASE_CONFIG_DIR / SDKWORK_DATABASE_URL.",
                )
            })?;
        validate_identifier("database name", &database)?;
        let schema = match optional_identifier(env, "SDKWORK_DATABASE_SCHEMA")? {
            Some(value) => value,
            None => database.clone(),
        };
        let username = optional_identifier(env, "SDKWORK_DATABASE_USERNAME")?
            .or_else(|| parsed.as_ref().and_then(|value| value.username.clone()))
            .unwrap_or_else(|| database.clone());
        validate_identifier("database username", &username)?;
        let host = optional_value(env, "SDKWORK_DATABASE_HOST")
            .or_else(|| parsed.as_ref().and_then(|value| value.host.clone()))
            .unwrap_or_else(|| "127.0.0.1".to_owned());
        let port = optional_port(env, "SDKWORK_DATABASE_PORT")?
            .or_else(|| parsed.as_ref().and_then(|value| value.port))
            .unwrap_or(DEFAULT_DATABASE_PORT);
        let parsed_ssl_mode = url
            .as_deref()
            .and_then(|value| PgConnectOptions::from_str(value).ok())
            .map(|options| options.get_ssl_mode());
        let ssl_mode = optional_ssl_mode(env, "SDKWORK_DATABASE_SSL_MODE")?
            .or(parsed_ssl_mode)
            .unwrap_or(PgSslMode::Prefer);
        let ssl_root_cert = ssl_root_cert(env);
        let password = resolve_password(
            env,
            "SDKWORK_DATABASE_PASSWORD",
            "SDKWORK_DATABASE_PASSWORD_FILE",
        )?
        .or_else(|| parsed.as_ref().and_then(|value| value.password.clone()));
        let url = match url {
            Some(value) => Some(value),
            None => Some(build_database_url(
                &host,
                port,
                &database,
                &username,
                password.as_deref(),
            )),
        };

        let target = ConnectionTarget {
            host,
            port,
            database,
            username,
            password,
            url,
            ssl_mode,
            ssl_root_cert: ssl_root_cert.clone(),
        };

        let admin_url = optional_value(env, ADMIN_URL_ENV_KEY);
        let admin_username = optional_value(env, "SDKWORK_DATABASE_ADMIN_USERNAME")
            .unwrap_or_else(|| target.username.clone());
        validate_identifier("database admin username", &admin_username)?;
        let admin_host = optional_value(env, "SDKWORK_DATABASE_ADMIN_HOST")
            .unwrap_or_else(|| target.host.clone());
        let admin_port = optional_port(env, "SDKWORK_DATABASE_ADMIN_PORT")?.unwrap_or(target.port);
        let admin_database = optional_value(env, "SDKWORK_DATABASE_ADMIN_DATABASE")
            .unwrap_or_else(|| DEFAULT_ADMIN_DATABASE.to_owned());
        validate_identifier("database admin database", &admin_database)?;
        let admin_url_ssl_mode = admin_url
            .as_deref()
            .and_then(|value| PgConnectOptions::from_str(value).ok())
            .map(|options| options.get_ssl_mode());
        let admin_ssl_mode = optional_ssl_mode(env, "SDKWORK_DATABASE_ADMIN_SSL_MODE")?
            .or(admin_url_ssl_mode)
            .unwrap_or(target.ssl_mode);
        let admin_password = resolve_password(
            env,
            "SDKWORK_DATABASE_ADMIN_PASSWORD",
            "SDKWORK_DATABASE_ADMIN_PASSWORD_FILE",
        )?
        .or_else(|| target.password.clone());

        let admin = ConnectionTarget {
            host: admin_host,
            port: admin_port,
            database: admin_database,
            username: admin_username,
            password: admin_password,
            url: admin_url,
            ssl_mode: admin_ssl_mode,
            ssl_root_cert,
        };

        let admin_configured = admin_is_configured(env);
        let admin_is_distinct =
            admin_configured && (admin.url.is_some() || admin.username != target.username);
        let connect_timeout = Duration::from_secs(
            optional_u64(env, CONNECT_TIMEOUT_ENV_KEY)?.unwrap_or(DEFAULT_CONNECT_TIMEOUT_SECONDS),
        );

        Ok(Self {
            admin,
            target,
            schema,
            admin_configured,
            admin_is_distinct,
            connect_timeout,
        })
    }

    async fn provision(
        &self,
        config: &GatewayRuntimeConfig,
    ) -> Result<ProvisioningReport, GatewayRuntimeError> {
        let mut report = ProvisioningReport::default();
        tracing::info!(
            target: "sdkwork_api_cloud_gateway::database_provisioning",
            database = %self.target.database,
            schema = %self.schema,
            user = %self.target.username,
            admin_is_distinct = self.admin_is_distinct,
            "provisioning workspace PostgreSQL identity"
        );

        if self.admin_is_distinct {
            let mut admin = self.connect(&self.admin, None, "admin database").await?;
            report.role_created = self.ensure_role(&mut admin).await?;
            if let Some(password) = self.target.password.as_deref() {
                // Keep the role password converged with the deployment env,
                // matching the development initializer. Only an explicitly
                // configured admin identity may rotate the application role.
                let sql = format!(
                    "ALTER ROLE {} WITH LOGIN PASSWORD {}",
                    quote_ident(&self.target.username),
                    quote_literal(password)
                );
                execute(&mut admin, &sql, "converge role password").await?;
            }
            report.database_created = self.ensure_database(&mut admin).await?;
        }

        let provisioner = if self.admin_configured {
            &self.admin
        } else {
            &self.target
        };
        let mut connection = self
            .connect(provisioner, Some(&self.target.database), "target database")
            .await?;

        report.schema_created = self.ensure_schema(&mut connection).await?;
        self.ensure_grants(&mut connection, &mut report).await?;
        report.extensions_converged = self
            .ensure_extensions(&mut connection, config, &mut report)
            .await?;
        drop(connection);

        // Prove the runtime credentials (not the admin connection) work. An
        // install that converges the schema but leaves the application role
        // unable to authenticate would otherwise fail only on the first
        // request, after the container reports healthy.
        let application = self
            .connect(
                &self.target,
                Some(&self.target.database),
                "application role validation",
            )
            .await?;
        let _ = tokio::time::timeout(Duration::from_secs(5), application.close()).await;

        tracing::info!(
            target: "sdkwork_api_cloud_gateway::database_provisioning",
            role_created = report.role_created,
            database_created = report.database_created,
            schema_created = report.schema_created,
            extensions_converged = ?report.extensions_converged,
            warnings = report.warnings,
            "workspace PostgreSQL provisioning completed"
        );
        Ok(report)
    }

    async fn connect(
        &self,
        target: &ConnectionTarget,
        database_override: Option<&str>,
        label: &str,
    ) -> Result<PgConnection, GatewayRuntimeError> {
        let options = target.connect_options(database_override)?;
        let deadline = Instant::now() + self.connect_timeout;
        loop {
            let attempt = tokio::time::timeout(
                CONNECT_ATTEMPT_TIMEOUT,
                PgConnection::connect_with(&options),
            )
            .await;
            match attempt {
                Ok(Ok(connection)) => return Ok(connection),
                Ok(Err(error)) => {
                    if let Some(reason) = non_retryable_connection_error(&error) {
                        return Err(provisioning_error(
                            "connect",
                            format!(
                                "could not connect to the PostgreSQL {label}: {reason} ({error})"
                            ),
                        ));
                    }
                    if Instant::now() >= deadline {
                        return Err(provisioning_error(
                            "connect",
                            format!(
                                "could not connect to the PostgreSQL {label} within {}s: {error}. \
                                 Verify SDKWORK_DATABASE_* (and SDKWORK_DATABASE_ADMIN_* when the \
                                 role/database must be created), the server endpoint, and pg_hba.conf.",
                                self.connect_timeout.as_secs()
                            ),
                        ));
                    }
                    tracing::warn!(
                        target: "sdkwork_api_cloud_gateway::database_provisioning",
                        database = %options.get_database().unwrap_or("<default>"),
                        user = %options.get_username(),
                        error = %error,
                        "PostgreSQL not ready yet; retrying database provisioning connection"
                    );
                }
                Err(_) => {
                    if Instant::now() >= deadline {
                        return Err(provisioning_error(
                            "connect",
                            format!(
                                "could not connect to the PostgreSQL {label} within {}s \
                                 (last attempt timed out after {}s). Verify the server endpoint \
                                 and pg_hba.conf.",
                                self.connect_timeout.as_secs(),
                                CONNECT_ATTEMPT_TIMEOUT.as_secs()
                            ),
                        ));
                    }
                    tracing::warn!(
                        target: "sdkwork_api_cloud_gateway::database_provisioning",
                        database = %options.get_database().unwrap_or("<default>"),
                        user = %options.get_username(),
                        "PostgreSQL connection attempt timed out; retrying"
                    );
                }
            }
            tokio::time::sleep(CONNECT_RETRY_INTERVAL).await;
        }
    }

    async fn ensure_role(
        &self,
        connection: &mut PgConnection,
    ) -> Result<bool, GatewayRuntimeError> {
        let exists: bool = with_statement_timeout(
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = $1)")
                .bind(&self.target.username)
                .fetch_one(&mut *connection),
            "read role",
        )
        .await?;
        if exists {
            tracing::info!(
                target: "sdkwork_api_cloud_gateway::database_provisioning",
                user = %self.target.username,
                "application role already exists"
            );
            return Ok(false);
        }
        if !self.admin_is_distinct {
            return Err(provisioning_error(
                "create role",
                format!(
                    "application role `{}` does not exist and no distinct \
                     SDKWORK_DATABASE_ADMIN_* identity is configured. Set \
                     SDKWORK_DATABASE_ADMIN_USERNAME/PASSWORD (or \
                     SDKWORK_DATABASE_ADMIN_URL) to a role with CREATEROLE.",
                    self.target.username
                ),
            ));
        }

        let sql = format!(
            "CREATE ROLE {} LOGIN PASSWORD {}",
            quote_ident(&self.target.username),
            quote_literal(self.target.password.as_deref().unwrap_or_default())
        );
        execute(connection, &sql, "create role").await?;
        tracing::info!(
            target: "sdkwork_api_cloud_gateway::database_provisioning",
            user = %self.target.username,
            "created application role"
        );
        Ok(true)
    }

    async fn ensure_database(
        &self,
        connection: &mut PgConnection,
    ) -> Result<bool, GatewayRuntimeError> {
        let exists: bool = with_statement_timeout(
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_database WHERE datname = $1)")
                .bind(&self.target.database)
                .fetch_one(&mut *connection),
            "read database",
        )
        .await?;
        if exists {
            tracing::info!(
                target: "sdkwork_api_cloud_gateway::database_provisioning",
                database = %self.target.database,
                "application database already exists"
            );
            return Ok(false);
        }

        let sql = format!(
            "CREATE DATABASE {} OWNER {}",
            quote_ident(&self.target.database),
            quote_ident(&self.target.username)
        );
        execute(connection, &sql, "create database").await?;
        tracing::info!(
            target: "sdkwork_api_cloud_gateway::database_provisioning",
            database = %self.target.database,
            owner = %self.target.username,
            "created application database"
        );
        Ok(true)
    }

    async fn ensure_schema(
        &self,
        connection: &mut PgConnection,
    ) -> Result<bool, GatewayRuntimeError> {
        let exists: bool = with_statement_timeout(
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = $1)")
                .bind(&self.schema)
                .fetch_one(&mut *connection),
            "read schema",
        )
        .await?;
        if exists {
            if self.admin_is_distinct {
                let sql = format!(
                    "ALTER SCHEMA {} OWNER TO {}",
                    quote_ident(&self.schema),
                    quote_ident(&self.target.username)
                );
                execute(connection, &sql, "converge schema owner").await?;
            }
            return Ok(false);
        }

        let sql = format!(
            "CREATE SCHEMA IF NOT EXISTS {} AUTHORIZATION {}",
            quote_ident(&self.schema),
            quote_ident(&self.target.username)
        );
        execute(connection, &sql, "create schema").await?;
        tracing::info!(
            target: "sdkwork_api_cloud_gateway::database_provisioning",
            schema = %self.schema,
            owner = %self.target.username,
            "created application schema"
        );
        Ok(true)
    }

    async fn ensure_grants(
        &self,
        connection: &mut PgConnection,
        report: &mut ProvisioningReport,
    ) -> Result<(), GatewayRuntimeError> {
        let schema = quote_ident(&self.schema);
        let user = quote_ident(&self.target.username);
        execute(
            connection,
            &format!("GRANT USAGE, CREATE ON SCHEMA {schema} TO {user}"),
            "grant schema privileges",
        )
        .await?;

        if self.admin_is_distinct {
            let database = quote_ident(&self.target.database);
            execute(
                connection,
                &format!("GRANT CONNECT, TEMPORARY ON DATABASE {database} TO {user}"),
                "grant database privileges",
            )
            .await?;
            for object in ["TABLES", "SEQUENCES", "FUNCTIONS"] {
                execute(
                    connection,
                    &format!("GRANT ALL PRIVILEGES ON ALL {object} IN SCHEMA {schema} TO {user}"),
                    "grant existing schema object privileges",
                )
                .await?;
                execute(
                    connection,
                    &format!(
                        "ALTER DEFAULT PRIVILEGES IN SCHEMA {schema} \
                         GRANT ALL PRIVILEGES ON {object} TO {user}"
                    ),
                    "grant default schema privileges",
                )
                .await?;
            }
            execute(
                connection,
                &format!("ALTER ROLE {user} SET search_path TO {schema}, public"),
                "converge role search_path",
            )
            .await?;
        } else {
            // Without a distinct admin identity the application role may be
            // least-privilege. Existing installations already carry these
            // grants; report a gap instead of failing a working deployment.
            let has_connect: bool = with_statement_timeout(
                sqlx::query_scalar(
                    "SELECT has_database_privilege($1, current_database(), 'CONNECT')",
                )
                .bind(&self.target.username)
                .fetch_one(&mut *connection),
                "read database privilege",
            )
            .await?;
            if !has_connect {
                report.warnings += 1;
                tracing::warn!(
                    target: "sdkwork_api_cloud_gateway::database_provisioning",
                    user = %self.target.username,
                    "application role lacks CONNECT on the database; configure SDKWORK_DATABASE_ADMIN_* to converge grants"
                );
            }
        }
        Ok(())
    }

    async fn ensure_extensions(
        &self,
        connection: &mut PgConnection,
        config: &GatewayRuntimeConfig,
        report: &mut ProvisioningReport,
    ) -> Result<Vec<String>, GatewayRuntimeError> {
        let mut converged = Vec::new();
        for (extension, owning_workspace) in EXTENSIONS {
            let required = extension_required(owning_workspace, config);
            let installed_schema: Option<String> = with_statement_timeout(
                sqlx::query_scalar(
                    "SELECT n.nspname \
                     FROM pg_extension e JOIN pg_namespace n ON n.oid = e.extnamespace \
                     WHERE e.extname = $1",
                )
                .bind(extension)
                .fetch_optional(&mut *connection),
                "read extension",
            )
            .await?;

            match installed_schema.as_deref() {
                Some(schema) if schema == self.schema => continue,
                Some(schema) => {
                    // Never move an extension another workspace may depend on
                    // just because this deployment happens to see it.
                    if !required {
                        tracing::debug!(
                            target: "sdkwork_api_cloud_gateway::database_provisioning",
                            extension,
                            schema = %schema,
                            "optional extension is installed outside the business schema; leaving it in place"
                        );
                        continue;
                    }
                    let sql = format!(
                        "ALTER EXTENSION {} SET SCHEMA {}",
                        quote_ident(extension),
                        quote_ident(&self.schema)
                    );
                    match execute(connection, &sql, "relocate extension").await {
                        Ok(()) => {
                            tracing::info!(
                                target: "sdkwork_api_cloud_gateway::database_provisioning",
                                extension,
                                from = %schema,
                                to = %self.schema,
                                "relocated extension into the business schema"
                            );
                            converged.push(extension.to_owned());
                        }
                        Err(error) if self.admin_is_distinct => return Err(error),
                        Err(error) => {
                            // Existing deployments without admin credentials
                            // may not own the extension. Do not turn a
                            // previously running install into a boot failure;
                            // module migration still fails closed if the type
                            // is genuinely required.
                            report.warnings += 1;
                            tracing::warn!(
                                target: "sdkwork_api_cloud_gateway::database_provisioning",
                                extension,
                                error = %error,
                                "could not relocate extension without admin credentials; module migration will fail closed if it is required"
                            );
                        }
                    }
                }
                None => {
                    let available: bool = with_statement_timeout(
                        sqlx::query_scalar(
                            "SELECT EXISTS (SELECT 1 FROM pg_available_extensions WHERE name = $1)",
                        )
                        .bind(extension)
                        .fetch_one(&mut *connection),
                        "read available extension",
                    )
                    .await?;
                    if !available {
                        if required {
                            let package = match extension {
                                "vector" => "postgresql-<version>-pgvector",
                                "pg_trgm" => "postgresql-contrib",
                                _ => "the matching PostgreSQL extension package",
                            };
                            return Err(provisioning_error(
                                "create extension",
                                format!(
                                    "extension `{extension}` is required by the composed \
                                     `{owning_workspace}` surface but is not installed on the \
                                     PostgreSQL server. Install {package}, then rerun \
                                     --provision-databases."
                                ),
                            ));
                        }
                        tracing::debug!(
                            target: "sdkwork_api_cloud_gateway::database_provisioning",
                            extension,
                            "optional extension is not installed on the server; skipping"
                        );
                        continue;
                    }
                    let sql = format!(
                        "CREATE EXTENSION IF NOT EXISTS {} WITH SCHEMA {}",
                        quote_ident(extension),
                        quote_ident(&self.schema)
                    );
                    match execute(connection, &sql, "create extension").await {
                        Ok(()) => {
                            tracing::info!(
                                target: "sdkwork_api_cloud_gateway::database_provisioning",
                                extension,
                                schema = %self.schema,
                                "created required PostgreSQL extension"
                            );
                            converged.push(extension.to_owned());
                        }
                        Err(error) if required => {
                            return Err(provisioning_error(
                                "create extension",
                                format!(
                                    "extension `{extension}` is required by the composed \
                                     `{owning_workspace}` surface but could not be created: \
                                     {error}. Install the server extension package and grant \
                                     the admin role superuser (extension creation is not \
                                     delegated to application roles), then rerun \
                                     --provision-databases."
                                ),
                            ));
                        }
                        Err(error) => {
                            report.warnings += 1;
                            tracing::warn!(
                                target: "sdkwork_api_cloud_gateway::database_provisioning",
                                extension,
                                error = %error,
                                "optional extension could not be created; skipping"
                            );
                        }
                    }
                }
            }
        }
        Ok(converged)
    }
}

fn extension_required(owning_workspace: &str, config: &GatewayRuntimeConfig) -> bool {
    if owning_workspace.is_empty() {
        return false;
    }
    config
        .dependency_surfaces
        .iter()
        .any(|surface| surface.workspace == owning_workspace)
}

#[derive(Clone)]
struct ParsedDatabaseUrl {
    host: Option<String>,
    port: Option<u16>,
    database: Option<String>,
    username: Option<String>,
    password: Option<String>,
}

/// Parse the connection identity out of the canonical workspace URL. This is
/// the fallback for installed config-directory profiles, where identity lives
/// in `database.toml`/`database.env` rather than in process environment.
fn parse_database_url(value: &str) -> Result<ParsedDatabaseUrl, GatewayRuntimeError> {
    let url = url::Url::parse(value).map_err(|error| {
        provisioning_error(
            "parse-connection-url",
            format!("configured PostgreSQL URL is invalid: {error}"),
        )
    })?;
    let database = url.path().trim_start_matches('/');
    if database.is_empty() {
        return Err(provisioning_error(
            "parse-connection-url",
            format!("configured PostgreSQL URL does not name a database: {value}"),
        ));
    }
    Ok(ParsedDatabaseUrl {
        host: url.host_str().map(ToOwned::to_owned),
        port: url.port(),
        database: Some(percent_decode(database)),
        username: Some(percent_decode(url.username())),
        password: url.password().map(percent_decode),
    })
}

fn build_database_url(
    host: &str,
    port: u16,
    database: &str,
    username: &str,
    password: Option<&str>,
) -> String {
    let port = port.to_string();
    sdkwork_database_config::workspace_database::build_postgres_database_url(
        host,
        Some(&port),
        database,
        username,
        password.unwrap_or_default(),
        None,
    )
}

/// Userinfo components are percent-encoded in a PostgreSQL URL; SQL role
/// creation needs the decoded credential.
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
            {
                decoded.push(high * 16 + low);
                index += 3;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

/// Authentication/authorization/catalog failures will not heal by waiting;
/// surface them immediately with the operator remediation instead of retrying
/// for the full connect budget.
fn non_retryable_connection_error(error: &sqlx::Error) -> Option<&'static str> {
    let code = error.as_database_error()?.code()?;
    match code.as_ref() {
        "28P01" => Some(
            "password authentication failed; align the role password with the deployment env \
             or configure SDKWORK_DATABASE_ADMIN_* so provisioning can converge it",
        ),
        "28000" => Some(
            "the server rejected the connection identity (pg_hba.conf / SSL policy); \
             allow this host and role to authenticate",
        ),
        "3D000" => Some(
            "the target database does not exist; configure a distinct \
             SDKWORK_DATABASE_ADMIN_* identity so provisioning can create it",
        ),
        _ => None,
    }
}

fn admin_is_configured(env: &HashMap<String, String>) -> bool {
    // The admin database/SSL mode are shape defaults, not evidence that an
    // admin identity exists: compose always forwards `postgres` as the admin
    // database even when no admin credentials are configured.
    optional_value(env, ADMIN_URL_ENV_KEY).is_some()
        || optional_value(env, "SDKWORK_DATABASE_ADMIN_HOST").is_some()
        || optional_value(env, "SDKWORK_DATABASE_ADMIN_PORT").is_some()
        || optional_value(env, "SDKWORK_DATABASE_ADMIN_USERNAME").is_some()
        || optional_value(env, "SDKWORK_DATABASE_ADMIN_PASSWORD").is_some()
        || optional_value(env, "SDKWORK_DATABASE_ADMIN_PASSWORD_FILE").is_some()
}

fn optional_identifier(
    env: &HashMap<String, String>,
    key: &str,
) -> Result<Option<String>, GatewayRuntimeError> {
    let Some(value) = optional_value(env, key) else {
        return Ok(None);
    };
    validate_identifier(key, &value)?;
    Ok(Some(value))
}

fn validate_identifier(label: &str, value: &str) -> Result<(), GatewayRuntimeError> {
    let invalid = value.is_empty()
        || value.len() > MAX_IDENTIFIER_LENGTH
        || !value
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_');
    if invalid {
        return Err(provisioning_error(
            "validate configuration",
            format!(
                "{label} must be a bare PostgreSQL identifier (letters, digits, `_`, \
                 at most {MAX_IDENTIFIER_LENGTH} characters): `{value}`"
            ),
        ));
    }
    Ok(())
}

fn optional_value(env: &HashMap<String, String>, key: &str) -> Option<String> {
    env.get(key)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn optional_port(
    env: &HashMap<String, String>,
    key: &str,
) -> Result<Option<u16>, GatewayRuntimeError> {
    let Some(value) = optional_value(env, key) else {
        return Ok(None);
    };
    value.parse::<u16>().map(Some).map_err(|_| {
        provisioning_error(
            "validate configuration",
            format!("{key} must be a TCP port (1..65535): `{value}`"),
        )
    })
}

fn optional_u64(
    env: &HashMap<String, String>,
    key: &str,
) -> Result<Option<u64>, GatewayRuntimeError> {
    let Some(value) = optional_value(env, key) else {
        return Ok(None);
    };
    value.parse::<u64>().map(Some).map_err(|_| {
        provisioning_error(
            "validate configuration",
            format!("{key} must be a non-negative integer: `{value}`"),
        )
    })
}

fn optional_ssl_mode(
    env: &HashMap<String, String>,
    key: &str,
) -> Result<Option<PgSslMode>, GatewayRuntimeError> {
    let Some(value) = optional_value(env, key) else {
        return Ok(None);
    };
    let mode = match value.to_ascii_lowercase().as_str() {
        "disable" => PgSslMode::Disable,
        "allow" => PgSslMode::Allow,
        "prefer" => PgSslMode::Prefer,
        "require" => PgSslMode::Require,
        "verify-ca" => PgSslMode::VerifyCa,
        "verify-full" => PgSslMode::VerifyFull,
        _ => {
            return Err(provisioning_error(
                "validate configuration",
                format!(
                    "{key} must be disable|allow|prefer|require|verify-ca|verify-full: `{value}`"
                ),
            ));
        }
    };
    Ok(Some(mode))
}

fn ssl_root_cert(env: &HashMap<String, String>) -> Option<PathBuf> {
    optional_value(env, "SDKWORK_DATABASE_SSL_ROOT_CERT")
        .or_else(|| optional_value(env, "SDKWORK_API_CLOUD_GATEWAY_POSTGRES_CA_FILE"))
        .map(PathBuf::from)
        .filter(|path| path.is_file())
}

fn resolve_password(
    env: &HashMap<String, String>,
    direct_key: &str,
    file_key: &str,
) -> Result<Option<String>, GatewayRuntimeError> {
    // Passwords are opaque: only emptiness is trimmed, never the value itself
    // (a leading/trailing space is a valid credential character).
    let direct = env
        .get(direct_key)
        .filter(|value| !value.trim().is_empty())
        .cloned();
    let file = optional_value(env, file_key);
    if direct.is_some() && file.is_some() {
        return Err(provisioning_error(
            "validate configuration",
            format!("{direct_key} and {file_key} are mutually exclusive"),
        ));
    }
    if let Some(password) = direct {
        return Ok(Some(password));
    }
    let Some(path) = file else {
        return Ok(None);
    };
    let path = Path::new(&path);
    let content = fs::read_to_string(path).map_err(|error| {
        provisioning_error(
            "read password file",
            format!("could not read {file_key} `{}`: {error}", path.display()),
        )
    })?;
    let password = content.trim_end_matches(['\r', '\n']).to_owned();
    if password.is_empty() {
        return Err(provisioning_error(
            "read password file",
            format!("{file_key} `{}` is empty", path.display()),
        ));
    }
    Ok(Some(password))
}

fn quote_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

async fn execute(
    connection: &mut PgConnection,
    sql: &str,
    step: &str,
) -> Result<(), GatewayRuntimeError> {
    // Every statement is assembled from a validated bare identifier or an
    // escaped string literal in this module; no caller-supplied SQL reaches
    // here. `raw_sql` keeps DDL (CREATE ROLE/DATABASE/EXTENSION) on the simple
    // query protocol instead of the prepared-statement path.
    with_statement_timeout(
        sqlx::raw_sql(sqlx::AssertSqlSafe(sql)).execute(&mut *connection),
        step,
    )
    .await
    .map(|_| ())
}

/// Bound every provisioning statement so an unreachable or lock-blocked
/// database cannot stall install/start indefinitely (RUST_CODE_SPEC async
/// timeout rule).
async fn with_statement_timeout<T>(
    future: impl Future<Output = Result<T, sqlx::Error>>,
    step: &str,
) -> Result<T, GatewayRuntimeError> {
    tokio::time::timeout(STATEMENT_TIMEOUT, future)
        .await
        .map_err(|_| {
            provisioning_error(
                step,
                format!("statement timed out after {}s", STATEMENT_TIMEOUT.as_secs()),
            )
        })?
        .map_err(|error| provisioning_error(step, error.to_string()))
}

fn provisioning_error(step: &str, message: impl Into<String>) -> GatewayRuntimeError {
    GatewayRuntimeError::DatabaseProvisioning {
        step: step.to_owned(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn env(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn requires_the_workspace_database_identity() {
        let error =
            ProvisioningSettings::from_env(&env(&[])).expect_err("database name is required");
        assert!(error.to_string().contains("SDKWORK_DATABASE_NAME"));
    }

    #[test]
    fn defaults_schema_and_user_to_the_database_name() {
        let settings = ProvisioningSettings::from_env(&env(&[
            ("SDKWORK_DATABASE_NAME", "sdkwork_ai_prod"),
            ("SDKWORK_DATABASE_PASSWORD", "secret"),
            // Compose always forwards this shape default even without admin
            // credentials; it must not by itself select the admin connection.
            ("SDKWORK_DATABASE_ADMIN_DATABASE", "postgres"),
        ]))
        .expect("settings parse");
        assert_eq!(settings.target.database, "sdkwork_ai_prod");
        assert_eq!(settings.schema, "sdkwork_ai_prod");
        assert_eq!(settings.target.username, "sdkwork_ai_prod");
        assert_eq!(settings.admin.database, DEFAULT_ADMIN_DATABASE);
        assert_eq!(settings.admin.username, "sdkwork_ai_prod");
        assert!(!settings.admin_configured);
        assert!(!settings.admin_is_distinct);
    }

    #[test]
    fn an_explicit_admin_identity_is_distinct_and_uses_admin_defaults() {
        let settings = ProvisioningSettings::from_env(&env(&[
            ("SDKWORK_DATABASE_NAME", "sdkwork_ai_prod"),
            ("SDKWORK_DATABASE_USERNAME", "sdkwork_ai_prod"),
            ("SDKWORK_DATABASE_PASSWORD", "app-secret"),
            ("SDKWORK_DATABASE_ADMIN_USERNAME", "postgres"),
            ("SDKWORK_DATABASE_ADMIN_PASSWORD", "admin-secret"),
        ]))
        .expect("settings parse");
        assert!(settings.admin_is_distinct);
        assert_eq!(settings.admin.username, "postgres");
        assert_eq!(settings.admin.database, "postgres");
        assert_eq!(settings.admin.password.as_deref(), Some("admin-secret"));
    }

    #[test]
    fn rejects_unsafe_identifiers_before_they_reach_sql() {
        for (key, value) in [
            (
                "SDKWORK_DATABASE_NAME",
                "sdkwork_ai_prod; DROP DATABASE postgres",
            ),
            ("SDKWORK_DATABASE_SCHEMA", "1invalid"),
            ("SDKWORK_DATABASE_USERNAME", "user-name"),
        ] {
            let pairs = [
                ("SDKWORK_DATABASE_NAME", "sdkwork_ai_prod"),
                ("SDKWORK_DATABASE_PASSWORD", "secret"),
                (key, value),
            ];
            let error = ProvisioningSettings::from_env(&env(&pairs))
                .expect_err("unsafe identifier must be rejected");
            assert!(error.to_string().contains("bare PostgreSQL identifier"));
        }
    }

    #[test]
    fn reads_password_files_and_rejects_ambiguous_password_sources() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "sdkwork-gateway-provision-password-{}-{unique}",
            std::process::id()
        ));
        fs::write(&path, "file-secret\n").expect("write password file");
        let path_text = path.to_string_lossy().into_owned();

        let settings = ProvisioningSettings::from_env(&env(&[
            ("SDKWORK_DATABASE_NAME", "sdkwork_ai_prod"),
            ("SDKWORK_DATABASE_PASSWORD_FILE", &path_text),
        ]))
        .expect("settings parse");
        assert_eq!(settings.target.password.as_deref(), Some("file-secret"));

        let error = ProvisioningSettings::from_env(&env(&[
            ("SDKWORK_DATABASE_NAME", "sdkwork_ai_prod"),
            ("SDKWORK_DATABASE_PASSWORD", "direct"),
            ("SDKWORK_DATABASE_PASSWORD_FILE", &path_text),
        ]))
        .expect_err("direct and file passwords must not coexist");
        assert!(error.to_string().contains("mutually exclusive"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn derives_identity_from_the_workspace_url_when_split_fields_are_absent() {
        let settings = ProvisioningSettings::from_env(&env(&[(
            "SDKWORK_DATABASE_URL",
            "postgresql://sdkwork_ai_prod:p%40ssword@db.internal:6543/sdkwork_ai_prod?sslmode=require",
        )]))
        .expect("settings parse");
        assert_eq!(settings.target.database, "sdkwork_ai_prod");
        assert_eq!(settings.schema, "sdkwork_ai_prod");
        assert_eq!(settings.target.username, "sdkwork_ai_prod");
        assert_eq!(settings.target.host, "db.internal");
        assert_eq!(settings.target.port, 6543);
        assert_eq!(settings.target.password.as_deref(), Some("p@ssword"));
        assert!(matches!(settings.target.ssl_mode, PgSslMode::Require));
    }

    #[test]
    fn rejects_a_url_without_a_database_and_decodes_identity() {
        let parsed = parse_database_url(
            "postgresql://sdkwork_ai_prod:p%40ss%2Fword@db.internal:5432/sdkwork_ai_prod",
        )
        .expect("url parses");
        assert_eq!(parsed.database.as_deref(), Some("sdkwork_ai_prod"));
        assert_eq!(parsed.password.as_deref(), Some("p@ss/word"));

        let error = match parse_database_url("postgresql://user:pass@db.internal:5432") {
            Ok(_) => panic!("url must name a database"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("does not name a database"));
    }

    #[test]
    fn builds_a_canonical_url_without_losing_credentials() {
        let url = build_database_url(
            "db.internal",
            5432,
            "sdkwork_ai_prod",
            "sdkwork_ai_prod",
            Some("p@ss/word"),
        );
        let parsed = parse_database_url(&url).expect("built url parses");
        assert_eq!(parsed.host.as_deref(), Some("db.internal"));
        assert_eq!(parsed.database.as_deref(), Some("sdkwork_ai_prod"));
        assert_eq!(parsed.username.as_deref(), Some("sdkwork_ai_prod"));
        assert_eq!(parsed.password.as_deref(), Some("p@ss/word"));
    }

    #[test]
    fn rejects_invalid_ports_and_ssl_modes() {
        let error = ProvisioningSettings::from_env(&env(&[
            ("SDKWORK_DATABASE_NAME", "sdkwork_ai_prod"),
            ("SDKWORK_DATABASE_PORT", "not-a-port"),
        ]))
        .expect_err("port must be numeric");
        assert!(error.to_string().contains("SDKWORK_DATABASE_PORT"));

        let error = ProvisioningSettings::from_env(&env(&[
            ("SDKWORK_DATABASE_NAME", "sdkwork_ai_prod"),
            ("SDKWORK_DATABASE_SSL_MODE", "bogus"),
        ]))
        .expect_err("ssl mode must be known");
        assert!(error.to_string().contains("SDKWORK_DATABASE_SSL_MODE"));
    }

    #[test]
    fn provisioning_can_be_disabled_for_operator_managed_schemas() {
        assert!(provisioning_enabled(&env(&[])));
        assert!(!provisioning_enabled(&env(&[(PROVISION_ENV_KEY, "false")])));
    }

    #[test]
    fn extension_requirements_follow_the_composed_surfaces() {
        let mut config = GatewayRuntimeConfig::default();
        assert!(!extension_required("sdkwork-knowledgebase", &config));
        config
            .dependency_surfaces
            .push(surface("sdkwork-knowledgebase"));
        assert!(extension_required("sdkwork-knowledgebase", &config));
        assert!(!extension_required("sdkwork-search", &config));
        config.dependency_surfaces.push(surface("sdkwork-im"));
        assert!(!extension_required("sdkwork-search", &config));
        config.dependency_surfaces.push(surface("sdkwork-search"));
        assert!(extension_required("sdkwork-search", &config));
        assert!(!extension_required("", &config));
    }

    #[test]
    fn quotes_identifiers_and_literals() {
        assert_eq!(quote_ident("sdkwork_ai_prod"), "\"sdkwork_ai_prod\"");
        assert_eq!(quote_ident("weird\"name"), "\"weird\"\"name\"");
        assert_eq!(quote_literal("o'brien"), "'o''brien'");
    }

    fn surface(workspace: &str) -> sdkwork_api_cloud_gateway_config::DependencyApiSurfaceConfig {
        sdkwork_api_cloud_gateway_config::DependencyApiSurfaceConfig {
            service_id: String::new(),
            workspace: workspace.to_owned(),
            sdk_family: String::new(),
            api_authority: String::new(),
            surface: String::new(),
            api_prefix: String::new(),
            runtime_mode: Default::default(),
            same_origin_allowed: true,
            executable_export: None,
            cargo_feature: None,
            cargo_dependency: None,
            coverage: String::new(),
        }
    }
}
