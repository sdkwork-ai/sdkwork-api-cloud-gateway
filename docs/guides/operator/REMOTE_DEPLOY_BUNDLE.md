# 安装包远程部署操作手册（bundle → 远程 Ubuntu 主机）

> 版本：2026-09-04 · 适用：`sdkwork-api-cloud-gateway` 统一安装镜像（bundle ≥ 0.1.1）
> 默认远程主机：`ubuntu@api.sdkwork.com` · 工具：`scripts/docker/remote-deploy.sh`
> 规范依据：`sdkwork-specs/DEPLOYMENT_SPEC.md` §6 · English version: [REMOTE_DEPLOY_BUNDLE.en.md](./REMOTE_DEPLOY_BUNDLE.en.md)

## 0. 什么时候用这份文档

本机已经完成打包（打包流程见 [docker-install.md](./docker-install.md) §2，产物在
`dist/docker-install/sdkwork-api-cloud-gateway-install-<版本>.bundle/`），现在要把
安装包部署到远程 Ubuntu Docker 主机。**全链路一条命令**：本地打包（可跳过）→
SSH 推送 → 远程物化配置 → 远程 `deploy.sh` → 健康检查。

远程主机要求只有三条：**Ubuntu + Docker（含 compose 插件）**、**`ubuntu` 用户可免密
sudo（`sudo -n`）**、**对本机免密 SSH**。

## 1. 一次性前置（首次部署前做一次，约 5 分钟）

在本机（打包机）执行：

```bash
# ① 生成 SSH 密钥（已有 ~/.ssh/id_ed25519 可跳过）
ssh-keygen -t ed25519

# ② 把公钥装到远程主机（输入一次 ubuntu 密码，以后免密）
ssh-copy-id ubuntu@api.sdkwork.com

# ③ 三项体检：免密 SSH / 免密 sudo / Docker。三个都 ok 才继续
ssh -o BatchMode=yes ubuntu@api.sdkwork.com \
  'echo ssh-ok; sudo -n true && echo sudo-ok; docker --version && docker compose version'
```

| 体检项 | 不过怎么办 |
| --- | --- |
| `ssh-ok` 没出现 | 重跑 `ssh-copy-id ubuntu@api.sdkwork.com`；非默认密钥/端口加 `--ssh-key` / `--ssh-port` |
| `sudo-ok` 没出现 | 在远程给 ubuntu 用户配免密 sudo（`sudo visudo` 加一行 `ubuntu ALL=(ALL) NOPASSWD:ALL`）；或改用 `--config-root /home/ubuntu/sdkwork-config`（用户可写目录，`/opt` 安装目录同样需要 sudo，建议直接配免密 sudo） |
| `docker` 没出现 | 远程先装 Docker：`curl -fsSL https://get.docker.com \| sudo sh` |

本机可选装 `rsync`（断点续传、只传差异）；没有也行，脚本自动降级用 `scp`。

## 2. 一条命令部署（默认主机，复制即用）

在**本机仓库根目录**执行：

```bash
# development（开发，单实例，默认外置依赖）
bash scripts/docker/remote-deploy.sh --host ubuntu@api.sdkwork.com --environment development

# test
bash scripts/docker/remote-deploy.sh --host ubuntu@api.sdkwork.com --environment test

# staging
bash scripts/docker/remote-deploy.sh --host ubuntu@api.sdkwork.com --environment staging

# production（3 实例，实例 1 负责迁移）
bash scripts/docker/remote-deploy.sh --host ubuntu@api.sdkwork.com --environment production --replicas 3
```

| 环境 | 健康检查端口（实例 1） | 建议节点 ID |
| --- | --- | --- |
| development | 3910 | 21 |
| test | 3911 | 22 |
| staging | 3912 | 23 |
| production | 3913 | 24 |

> `remote-deploy.sh` 支持 development / test / staging / production 四个环境；
> demo 环境请把 bundle 手工拷到远程后直接跑 bundle 内 `deploy.sh`（见 docker-install.md §3–§5）。

脚本自动完成 7 步：

1. **SSH 探活**：确认免密登录可用；
2. **本地打包**：`package-install-bundle.mjs` 打出/复用最新 bundle（想跳过见 §4）；
3. **推送**：`rsync -az --delete` 把 bundle 同步到远程 `/opt/sdkwork/deploy/sdkwork-api-cloud-gateway/`；
4. **物化配置**：本机 `docker/env/<env>.env`（缺失则用 `.example`）首次推到远程 `/etc/sdkwork/api-gateway/docker/<env>.env`（权限 0600）；
5. **远程部署**：SSH 执行远程 `deploy.sh --environment <env> --env-file <远程env> --replicas N`；
6. **健康检查**：远程 `curl /healthz`，失败即退出非 0；
7. 结束输出 `done: ubuntu@api.sdkwork.com <env> replicas=N`。

**上线前先预演**（只打印将执行的命令，不落盘）：

```bash
bash scripts/docker/remote-deploy.sh --host ubuntu@api.sdkwork.com \
  --environment production --replicas 3 --dry-run
```

**非默认 SSH 密钥 / 端口**：`--ssh-key ~/.ssh/prod_ed25519 --ssh-port 2222`；
也可用环境变量 `SDKWORK_SSH_KEY` / `SDKWORK_SSH_PORT` 固化。

## 3. 部署后的配置管理（唯一要编辑的文件）

远程配置文件：`/etc/sdkwork/api-gateway/docker/<env>.env`（权限 0600，首推自动生成）。
**这是该环境所有实例读取的唯一 env 文件**：

```bash
ssh ubuntu@api.sdkwork.com 'sudo nano /etc/sdkwork/api-gateway/docker/production.env'
```

- **production 必改项**：7 个 `<CHANGE_ME>`（§4.1 一键生成命令见
  docker-install.md §4.1）+ 确认 `GATEWAY_POSTGRES_SSL_MODE=require`；
  改完用 `grep -n "<CHANGE_ME>"` 自检（应无输出）。
- **改完配置如何生效**：重新执行 §2 同一条命令（幂等重跑即可，远程 env 不会被覆盖）。
- **重复执行不会覆盖远程配置**——远程文件已存在时以远程为准（远程改的数据库密码等
  不会丢）。确实要用本地 env 覆盖远程时，显式加 `--sync-env`。
- **多环境**：同一主机每个环境跑一遍 §2 对应命令即可，端口/网络/卷/库名天然隔离；
  节点 ID 已按上表在各 env 模板中预置互不相同。

## 4. 复用已有 bundle（跳过本地打包）

打包耗时长或构建机繁忙时，显式指定 bundle 目录即可跳过第 2 步打包：

```bash
bash scripts/docker/remote-deploy.sh --host ubuntu@api.sdkwork.com \
  --environment production --replicas 3 \
  --bundle dist/docker-install/sdkwork-api-cloud-gateway-install-0.1.1.bundle
```

离线主机在推送前可先校验完整性（远程执行）：

```bash
ssh ubuntu@api.sdkwork.com \
  'cd /opt/sdkwork/deploy/sdkwork-api-cloud-gateway && sha256sum -c image.sha256'
# 必须输出 image.tar.gz: OK
```

## 5. Day-2 日常运维（同样一条命令，动作转发到远程）

```bash
# 看状态 / 跟日志（N=实例序号）
bash scripts/docker/remote-deploy.sh --host ubuntu@api.sdkwork.com --environment production --ps
bash scripts/docker/remote-deploy.sh --host ubuntu@api.sdkwork.com --environment production --logs 1

# 重启（等健康才返回）/ 停止（保留容器与卷）/ 启动
bash scripts/docker/remote-deploy.sh --host ubuntu@api.sdkwork.com --environment production --restart
bash scripts/docker/remote-deploy.sh --host ubuntu@api.sdkwork.com --environment production --stop
bash scripts/docker/remote-deploy.sh --host ubuntu@api.sdkwork.com --environment production --start

# 摘除（删容器保留卷）/ 摘除并删卷（内置库数据不可恢复！）
bash scripts/docker/remote-deploy.sh --host ubuntu@api.sdkwork.com --environment production --down
bash scripts/docker/remote-deploy.sh --host ubuntu@api.sdkwork.com --environment production --down --purge
```

注意：**改了远程 env 后 `--restart` 不会生效**（restart 只重启进程不重建容器），
重跑一次 §2 的 apply 即可。更多原生命令（`docker ps --filter`、`docker compose -p ... ps`）
见 docker-install.md §5.1。

## 6. 升级与回滚

**升级**（数据在外置库或 Docker 卷里，升级不丢数据）：

```bash
# ① 本机打新版本包（改 sdkwork.app.config.json 的 release.currentVersion 或 --version）
bash scripts/build-install-bundle.sh
# ② 同一条部署命令重跑：新 bundle 自动推送，远程 env 配置原样保留
bash scripts/docker/remote-deploy.sh --host ubuntu@api.sdkwork.com --environment production --replicas 3
# ③ 验证
ssh ubuntu@api.sdkwork.com 'curl -fsS --noproxy "*" http://127.0.0.1:3913/healthz && curl -fsS --noproxy "*" http://127.0.0.1:3913/readyz'
```

**升级前三件套**（env 备份 / 保留旧 bundle / 数据库备份）见 docker-install.md §5.3。

**回滚**（两个办法，选其一）：

```bash
# 办法 A：把旧版本 bundle 再推一次（旧 bundle 的 image.tar.gz 随包携带，离线可回滚）
bash scripts/docker/remote-deploy.sh --host ubuntu@api.sdkwork.com \
  --environment production --replicas 3 \
  --bundle dist/docker-install/sdkwork-api-cloud-gateway-install-0.1.0.bundle

# 办法 B：新旧镜像都在远程本机时，直接指定旧 tag（更快）
bash scripts/docker/remote-deploy.sh --host ubuntu@api.sdkwork.com \
  --environment production --replicas 3 --image-tag 0.1.0
```

## 7. 故障速查

| 症状 | 原因 | 处置 |
| --- | --- | --- |
| `passwordless SSH failed` | 免密未配置 / 密钥或端口不对 | `ssh-copy-id ubuntu@api.sdkwork.com`；非默认加 `--ssh-key` / `--ssh-port` |
| `remote mkdir failed (need passwordless sudo)` | ubuntu 无免密 sudo | 远程 `sudo visudo` 加 `ubuntu ALL=(ALL) NOPASSWD:ALL`，或 `--config-root` 指向用户可写目录 |
| `healthz FAILED` | 数据库/Redis 不可达、密钥缺失、TLS 未配 | SSH 上去看日志：`ssh ubuntu@api.sdkwork.com 'docker logs --tail 200 sdkwork-api-cloud-gateway-<env>-i1-gateway-1'`，对照 docker-install.md §9.1 速查表 |
| 远程改动莫名被还原 | 部署命令带了 `--sync-env` | `--sync-env` 是显式覆盖，平时不要带；远程文件已存在时默认不会被覆盖 |
| rsync 卡在大量小文件 | bundle 首推正常现象；后续只传差异 | 无需处理；或本机确认装有 rsync（避免 scp 全量） |
| 端口被占用 | 旧舰队残留 | 远程 `docker ps --format '{{.Names}}\t{{.Ports}}'` 排查 |

完整症状对照（容器重启循环 / 迁移等待 / 代理污染等）见
[docker-install.md](./docker-install.md) §9；Day-2 与文件位置速查见 §5.1–§5.2。

---

## 附：本机 → 远程 资源对照

| 内容 | 本机（构建/打包机） | 远程 `ubuntu@api.sdkwork.com` |
| --- | --- | --- |
| 安装包 | `dist/docker-install/...bundle/` | `/opt/sdkwork/deploy/sdkwork-api-cloud-gateway/` |
| 环境配置 | `docker/env/<env>.env`（首推源） | `/etc/sdkwork/api-gateway/docker/<env>.env`（0600，运行时唯一生效） |
| 部署入口 | `scripts/docker/remote-deploy.sh` | bundle 内 `deploy.sh`（脚本自动调用） |
| Docker 资源 | 仅打包用（构建镜像） | 网络与卷 `sdkwork-api-cloud-gateway-<env>-*`，容器与日志（§5.2 速查表） |
