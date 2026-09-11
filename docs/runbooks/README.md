# Runbooks

按 `OPERATIONS_SPEC.md` §7 维护的运维手册。所有命令可复制即用。

| 手册 | 内容 |
|---|---|
| [deploy.md](deploy.md) | 安装 / 升级 / 回退 / 下线 |
| [log-reference.md](log-reference.md) | 日志读取、导出、级别与常见特征 |
| [troubleshooting.md](troubleshooting.md) | `bin/doctor.sh` 排障路径 |
| [backup-restore.md](backup-restore.md) | 备份、校验、恢复、演练 |

英文版见对应 `.en.md`。


<!-- scaffold-module-runbooks:index -->
## Docker 运维四件套（bin/ 标准，OPERATIONS_SPEC.md §7）

| Runbook | 内容 |
| --- | --- |
| [deploy.md](deploy.md) / [deploy.en.md](deploy.en.md) | 安装 / 升级 / 回滚 / 下线（bin/docker-deploy.sh + bin/docker-image.sh） |
| [troubleshooting.md](troubleshooting.md) / [troubleshooting.en.md](troubleshooting.en.md) | 症状 → doctor 检查 → 处置 |
| [backup-restore.md](backup-restore.md) / [backup-restore.en.md](backup-restore.en.md) | 备份 / 校验 / 恢复 / 演练（bin/backup.sh） |
| [log-reference.md](log-reference.md) / [log-reference.en.md](log-reference.en.md) | 健康日志特征与失败签名（bin/docker-deploy.sh logs） |
