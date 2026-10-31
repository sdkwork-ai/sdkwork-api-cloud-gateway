# sdkwork-api-cloud-gateway standalone container image.
# runtimeTarget = "container", deploymentProfile = "standalone".
# Built with platform-foundation-suite-container (foundation-webserver omitted).
# Install layout matches Ubuntu: /opt/sdkwork/api-gateway + /etc/sdkwork/api-gateway.
# Public reverse proxy is owned by the independent sdkwork-webserver deployment —
# this image does not install or start nginx site mappings.
# Build context: an unpacked server install package directory containing
# bin/, config/, specs/, database-modules/, agent.app.config.json,
# INSTALL.md, install-manifest.json and container/ (entrypoint + metadata.json).
#
# This file is the build input for `pnpm build:container`
# (scripts/build-api-cloud-gateway-container.mjs). Build the image first with
# `pnpm build:container` (produces sdkwork-api-cloud-gateway:local), then
# `docker compose up -d` (external PostgreSQL + Redis by default).
#
# Local quick start without compose (the image bakes the production template at
# CONFIG_FILE, so no config mount is required):
#   docker run -d --name sdkwork-gateway -p 3910:3900 \
#     -e GATEWAY_ENVIRONMENT=development \
#     -e SDKWORK_API_CLOUD_GATEWAY_ENVIRONMENT=development \
#     -e SDKWORK_API_CLOUD_GATEWAY_PROFILE_ID=standalone.development \
#     -e SDKWORK_DATABASE_URL='postgresql://<user>:<pass>@host.docker.internal:5432/sdkwork_ai_dev?sslmode=disable&options=-c%20search_path%3Dsdkwork_ai_dev' \
#     -e SDKWORK_IAM_SIGNING_MASTER_SECRET="$(openssl rand -hex 32)" \
#     -e SDKWORK_IAM_PROVIDER_CREDENTIAL_MASTER_SECRET="$(openssl rand -hex 32)" \
#     -e SDKWORK_CLOUDROUTER_REDIS_HOST=host.docker.internal \
#     -e SDKWORK_CLOUDROUTER_REDIS_PASSWORD='<redis-password>' \
#     registry.sdkwork.com/apps/sdkwork-api-cloud-gateway:<version>
#   curl -fsS http://127.0.0.1:3910/healthz
# The entrypoint runs `--migrate-databases` before serving unless
# SDKWORK_API_CLOUD_GATEWAY_MIGRATE_ON_START=false.
#
# The `development` environment above is the zero-extra-dependency local shape.
# The production posture is also verified and needs three more inputs: a
# reachable Redis (`SDKWORK_API_CLOUD_GATEWAY_WEB_REDIS_URL` for the shared
# rate-limit/idempotency/admission stores), `SDKWORK_IAM_SIGNING_MASTER_SECRET`
# (or the *_FILE provisioning the entrypoint performs when
# SDKWORK_API_CLOUD_GATEWAY_PROVISION_IAM_SIGNING_MASTER_SECRET=true) and
# SDKWORK_IAM_PROVIDER_CREDENTIAL_MASTER_SECRET. In production the Host header
# must be a registered API domain (the production template enforces the
# allow-list and answers 421 otherwise), so probe health with
# `curl -H 'Host: api.sdkwork.com' ...`.

# Base image is pinned to an immutable multi-arch index digest
# (SUPPLY_CHAIN_SECURITY_SPEC: reproducible builds must not resolve a moving
# tag). The index covers linux/amd64 and linux/arm64, so the build follows the
# daemon platform; `scripts/build-api-cloud-gateway-container.mjs` records that
# architecture in the image manifest and refuses a mislabeled package.
# Refresh deliberately: it is the only place the base image version is fixed.
FROM debian:bookworm-slim@sha256:7c7b2c966bc9ee8cedfeef67e0e279108992c77681fa595db4a9d65c06ccc587

ARG GATEWAY_BINARY=sdkwork-api-cloud-gateway
ARG KNOWLEDGEBASE_RPC_BINARY=sdkwork-intelligence-knowledgebase-rpc-bin
ARG INSTALL_ROOT=/opt/sdkwork/api-gateway
ARG CONFIG_FILE=/etc/sdkwork/api-gateway/config.toml
ARG VERSION=0.0.0
ARG ARCHITECTURE=x64

# Runtime directory layout (RUNTIME_DIRECTORY_SPEC §4.5 Container Scope):
#   config    /etc/sdkwork/api-gateway/<process>.toml   (config mount)
#   secrets   /run/secrets/sdkwork/api-gateway          (secret mount)
#   data      /var/lib/sdkwork/api-gateway              (data volume)
#   cache     /var/cache/sdkwork/api-gateway            (cache volume)
#   run       /run/sdkwork/api-gateway                  (runtime state)
#   logs      /var/log/sdkwork/api-gateway              (file logs only when
#                                                         journald/stdout is not
#                                                         enough)
# The install root /opt/sdkwork/api-gateway holds the immutable package tree.
# Runtime config is materialized at /etc/sdkwork/api-gateway/config.toml
# (APPLICATION_DEPLOY_LAYOUT_SPEC). A mounted host config
# (./docker/config/config.toml) overrides it without rebuilding the image.
# Secret files injected by the platform (Docker secrets, mounts) land under
# /run/secrets/sdkwork/api-gateway per RUNTIME_DIRECTORY_SPEC §4.5 / §11.
# libssl3/ca-certificates are runtime dependencies of the gateway binary
# (PostgreSQL TLS and outbound HTTPS); the slim base image does not carry them.
# curl is used by the container healthcheck and operational diagnostics.
# Unlike the cloudrouter image, no gnupg/postgresql-client is installed: the
# gateway has no operator backup/restore CLI, so those tools would only bloat
# the image.
RUN apt-get update \
  && apt-get install -y --no-install-recommends libssl3 ca-certificates curl openssl \
  && rm -rf /var/lib/apt/lists/* \
  && groupadd --system sdkwork \
  && useradd --system --gid sdkwork --home-dir ${INSTALL_ROOT} sdkwork \
  && mkdir -p ${INSTALL_ROOT} /etc/sdkwork/api-gateway \
    /run/secrets/sdkwork/api-gateway /run/sdkwork/api-gateway \
    /var/lib/sdkwork/api-gateway /var/cache/sdkwork/api-gateway \
    /var/log/sdkwork/api-gateway \
  && chown -R sdkwork:sdkwork /etc/sdkwork/api-gateway \
    /run/secrets/sdkwork/api-gateway /run/sdkwork/api-gateway \
    /var/lib/sdkwork/api-gateway /var/cache/sdkwork/api-gateway \
    /var/log/sdkwork/api-gateway

WORKDIR ${INSTALL_ROOT}
COPY . ${INSTALL_ROOT}
RUN chmod 0755 ${INSTALL_ROOT}/bin/${GATEWAY_BINARY} \
    ${INSTALL_ROOT}/bin/${KNOWLEDGEBASE_RPC_BINARY} \
    ${INSTALL_ROOT}/container/entrypoint \
    ${INSTALL_ROOT}/container/knowledgebase-rpc-entrypoint \
  && mkdir -p /etc/sdkwork/api-gateway \
  && cp ${INSTALL_ROOT}/config/sdkwork-api-cloud-gateway.production.toml \
    ${CONFIG_FILE} \
  && chown -R sdkwork:sdkwork ${INSTALL_ROOT} \
  && chown sdkwork:sdkwork ${CONFIG_FILE}

ENV SDKWORK_API_GATEWAY_CONFIG_FILE=${CONFIG_FILE}
ENV SDKWORK_API_CLOUD_GATEWAY_CONFIG_FILE=${CONFIG_FILE}
ENV SDKWORK_API_CLOUD_GATEWAY_DEPLOYMENT_PROFILE=standalone
ENV SDKWORK_API_CLOUD_GATEWAY_RUNTIME_TARGET=container
# Application identity root: agent.app.config.json + specs/component.spec.json
# are installed at the install root; the gateway resolves the component
# contract and IAM tenant provisioning through it (application root ".").
ENV SDKWORK_APP_ROOT=${INSTALL_ROOT}
# Packaged database module root: each host resolves its packaged module under
# <install root>/database-modules/<workspace>/database (compile-time app roots
# do not exist inside the image). The per-workspace app roots below mirror the
# installer's module plan; the last segment of each value must equal the
# workspace name so hosts resolve packaged modules by app root file name.
ENV SDKWORK_DATABASE_MODULES_ROOT=${INSTALL_ROOT}/database-modules \
    SDKWORK_ACCOUNT_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-account \
    SDKWORK_AGENTS_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-agents \
    SDKWORK_AIOT_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-aiot \
    SDKWORK_APPSTORE_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-appstore \
    SDKWORK_BROWSER_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-browser \
    SDKWORK_CATALOG_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-catalog \
    SDKWORK_CLOUDROUTER_ROUTER_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-cloudrouter \
    SDKWORK_CMS_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-cms \
    SDKWORK_COMMUNITY_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-community \
    SDKWORK_COMPANY_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-company \
    SDKWORK_COURSE_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-course \
    SDKWORK_CUSTOMER_SERVICE_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-customerservice \
    SDKWORK_DEPLOY_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-deployments \
    SDKWORK_DEZHOU_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-dezhou \
    SDKWORK_DOCUMENTS_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-documents \
    SDKWORK_DOUDIZHU_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-doudizhu \
    SDKWORK_DRIVE_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-drive \
    SDKWORK_FORUM_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-forum \
    SDKWORK_FEEDS_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-feeds \
    SDKWORK_GAMES_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-gameengine \
    SDKWORK_IAM_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-iam \
    SDKWORK_IM_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-im \
    SDKWORK_IMAGE_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-image \
    SDKWORK_INVENTORY_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-inventory \
    SDKWORK_INVOICE_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-invoice \
    SDKWORK_KNOWLEDGEBASE_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-knowledgebase \
    SDKWORK_LLM_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-llm \
    SDKWORK_LOCAL_ROUTER_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-local-router \
    SDKWORK_LOG_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-log \
    SDKWORK_MAHJONG_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-mahjong \
    SDKWORK_MAIL_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-mail \
    SDKWORK_MANAGER_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-manager \
    SDKWORK_MCP_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-mcp \
    SDKWORK_MEMBERSHIP_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-membership \
    SDKWORK_MERCHANDISE_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-merchandise \
    SDKWORK_MESSAGING_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-messaging \
    SDKWORK_CANVAS_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-canvas \
    SDKWORK_CLOUDROUTER_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-cloudrouter \
    SDKWORK_GITHUB_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-github \
    SDKWORK_MODELS_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-models \
    SDKWORK_MODELKIT_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-modelkit \
    SDKWORK_NEWS_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-news \
    SDKWORK_NOTARY_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-notary \
    SDKWORK_NOTES_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-notes \
    SDKWORK_ORDER_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-order \
    SDKWORK_PAYMENT_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-payment \
    SDKWORK_PORTAL_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-portal \
    SDKWORK_PROMOTION_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-promotion \
    SDKWORK_PROMPTS_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-prompts \
    SDKWORK_RTC_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-rtc \
    SDKWORK_SEARCH_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-search \
    SDKWORK_SETTINGS_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-settings \
    SDKWORK_SHOP_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-shop \
    SDKWORK_SKILLS_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-skills \
    SDKWORK_VOICE_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-voice \
    SDKWORK_XIANGQI_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-xiangqi \
    # appbase hosts (base/edu/med data) all resolve the same packaged module
    # (mirrors sdkwork-cloudrouter's install plan).
    SDKWORK_BASE_DATA_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-appbase \
    SDKWORK_EDU_DATA_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-appbase \
    SDKWORK_MED_DATA_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-appbase \
    SDKWORK_PARTNER_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-partner \
    SDKWORK_WEB_STORE_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-web-framework
# Models catalog (sdkwork-models.json + models/ + overlays/) installed under
# <install root>/data/sdkwork-models; the cloudrouter foundation module loads
# it through SDKWORK_MODELS_CATALOG_ROOT (overridable with a mounted catalog).
ENV SDKWORK_MODELS_CATALOG_ROOT=${INSTALL_ROOT}/data/sdkwork-models

LABEL org.opencontainers.image.title="sdkwork-api-cloud-gateway (standalone container)"
LABEL org.opencontainers.image.version="${VERSION}"
LABEL org.opencontainers.image.architecture="${ARCHITECTURE}"
LABEL org.opencontainers.image.vendor="sdkwork"

USER sdkwork
EXPOSE 3900
ENTRYPOINT ["/opt/sdkwork/api-gateway/container/entrypoint"]
