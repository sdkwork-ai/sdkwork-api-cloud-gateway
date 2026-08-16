# sdkwork-api-cloud-gateway standalone container image.
# runtimeTarget = "container", deploymentProfile = "standalone".
# Build context: an unpacked server install package directory containing
# bin/, config/, specs/, database-modules/, sdkwork.app.config.json,
# INSTALL.md, install-manifest.json and container/ (entrypoint + metadata.json).
#
# This file is the build input for `pnpm build:container`
# (scripts/build-api-cloud-gateway-container.mjs). Build the image first with
# `pnpm build:container` (produces sdkwork-api-cloud-gateway:local), then
# `docker compose up -d`.

FROM debian:bookworm-slim

ARG GATEWAY_BINARY=sdkwork-api-cloud-gateway
ARG KNOWLEDGEBASE_RPC_BINARY=sdkwork-intelligence-knowledgebase-rpc-bin
ARG INSTALL_ROOT=/opt/sdkwork/api-gateway
ARG CONFIG_FILE=/etc/sdkwork/api-gateway/sdkwork-api-cloud-gateway.toml
ARG VERSION=0.0.0

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
# The runtime config file is materialized at the canonical §4.5 path from the
# packaged production template; a mounted host config (docker compose
# ./docker/config/sdkwork-api-cloud-gateway.toml) overrides it without
# rebuilding the image. Secret files injected by the platform (Docker secrets,
# mounts) land under /run/secrets/sdkwork/api-gateway per §11 Secrets Standard.
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
  && chown sdkwork:sdkwork ${CONFIG_FILE}

ENV SDKWORK_API_CLOUD_GATEWAY_CONFIG_FILE=${CONFIG_FILE}
ENV SDKWORK_API_CLOUD_GATEWAY_DEPLOYMENT_PROFILE=standalone
ENV SDKWORK_API_CLOUD_GATEWAY_RUNTIME_TARGET=container
# Application identity root: sdkwork.app.config.json + specs/component.spec.json
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
    SDKWORK_COURSE_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-course \
    SDKWORK_CUSTOMER_SERVICE_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-customerservice \
    SDKWORK_DEPLOY_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-deployments \
    SDKWORK_DEZHOU_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-dezhou \
    SDKWORK_DOCUMENTS_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-documents \
    SDKWORK_DOUDIZHU_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-doudizhu \
    SDKWORK_DRIVE_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-drive \
    SDKWORK_FORUM_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-forum \
    SDKWORK_GAMES_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-gameengine \
    SDKWORK_IAM_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-iam \
    SDKWORK_IM_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-im \
    SDKWORK_IMAGE_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-image \
    SDKWORK_INVENTORY_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-inventory \
    SDKWORK_INVOICE_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-invoice \
    SDKWORK_KNOWLEDGEBASE_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-knowledgebase \
    SDKWORK_LLM_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-llm \
    SDKWORK_LOCAL_ROUTER_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-local-router \
    SDKWORK_MAHJONG_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-mahjong \
    SDKWORK_MAIL_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-mail \
    SDKWORK_MANAGER_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-manager \
    SDKWORK_MCP_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-mcp \
    SDKWORK_MEMBERSHIP_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-membership \
    SDKWORK_MERCHANDISE_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-merchandise \
    SDKWORK_MODELS_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-models \
    SDKWORK_MODELKIT_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-modelkit \
    SDKWORK_NEWS_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-news \
    SDKWORK_NOTARY_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-notary \
    SDKWORK_ORDER_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-order \
    SDKWORK_PAYMENT_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-payment \
    SDKWORK_PORTAL_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-portal \
    SDKWORK_PROMOTION_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-promotion \
    SDKWORK_PROMPTS_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-prompts \
    SDKWORK_RTC_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-rtc \
    SDKWORK_SHOP_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-shop \
    SDKWORK_SKILLS_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-skills \
    SDKWORK_VOICE_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-voice \
    SDKWORK_WEBSERVER_APP_ROOT=${INSTALL_ROOT}/database-modules/sdkwork-webserver \
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
LABEL org.opencontainers.image.vendor="sdkwork"

USER sdkwork
EXPOSE 3900
ENTRYPOINT ["/opt/sdkwork/api-gateway/container/entrypoint"]
