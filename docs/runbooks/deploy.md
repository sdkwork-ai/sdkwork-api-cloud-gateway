# 部署 Runbook（sdkwork-api-cloud-gateway）

适用环境：`development|test|staging|demo|production`。所有命令默认在本机 WSL 执行，远程主机加 `--host ssh://[user@]host[:port]`。

## 1. 构建与打包

```bash
bin/docker-image.sh build                       # tag 取自 sdkwork.app.config.json
bin/docker-image.sh save -o dist/image.tar.gz   # 产出 .tar.gz + .sha256
bin/apps-package.sh pc production               # 浏览器产物
bin/apps-package.sh h5 production
```

## 2. 安装 / 升级

```bash
# 生产环境必须带 --yes
bin/docker-deploy.sh install --environment development
bin/docker-deploy.sh install --environment demo --replicas 2
bin/docker-deploy.sh install --environment production --yes
bin/docker-deploy.sh upgrade  --environment demo --image-tag 0.1.1
```

install 会同步 bundle 到 `/opt/deploy/sdkwork-api-cloud-gateway/bundle`，加载镜像，按实例启动并等待健康门禁。

## 3. 验证

```bash
bin/docker-deploy.sh status --environment demo
bin/doctor.sh --environment demo          # 聚合诊断（9 项检查）
```

## 4. 版本回退

```bash
bin/docker-deploy.sh rollback --environment demo              # 回到上一版本
bin/docker-deploy.sh rollback --environment demo --to 0.1.0   # 回到指定版本
bin/docker-deploy.sh status  --environment demo
```

## 5. 下线

```bash
bin/docker-deploy.sh down --environment demo
bin/docker-deploy.sh stop    --environment demo                  # 停止（保留容器与卷，不重打包）
bin/docker-deploy.sh start   --environment demo                  # 启动已停止的栈（先起嵌入式依赖）
bin/docker-deploy.sh restart --environment demo                  # 只重启应用实例（依赖与网关不中断）
bin/docker-deploy.sh down --environment demo --purge --yes
```
