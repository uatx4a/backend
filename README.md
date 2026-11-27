# 换物集后端

基于 AFaster 0.0.16 和 afastorm 0.0.3，使用 AFast 二进制协议的 HTTP transport；小程序客户端通过 `wx.request` 调用。数据库使用 MySQL 8。

## 开发

小程序现有功能和建议后端接口清单见 [MINIPROGRAM_API_REQUIREMENTS.md](./MINIPROGRAM_API_REQUIREMENTS.md)。

按本机环境修改 `config.toml`，然后在 `backend/` 目录运行：

```powershell
if (-not (Test-Path config.toml)) { Copy-Item config.example.toml config.toml }
$bytes = New-Object byte[] 32
$rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
$rng.GetBytes($bytes)
$env:ORM_ADMIN_TOKEN = [Convert]::ToBase64String($bytes)
$rng.Dispose()
cargo run --bin generate_api
cargo run -- orm migrate --dir migrations/mysql
cargo run
```

`generate_api` 不连接数据库。MySQL schema 迁移由开发者显式执行，不会在服务启动时自动建表。当前生成的服务客户端位于 `../miniprogram/api/<service>/`。每个 service 使用 `src/services/<service>/mod.rs` 独立目录，并加入 `src/lib.rs` 的 `services()` 注册表；服务端和 API 生成命令共用该注册表。

AFaster ORM 管理端已启用，地址为 `/admin`。后端启动要求设置 `ORM_ADMIN_TOKEN`，至少 32 字节；上面的 PowerShell 示例为当前终端生成随机 token，浏览器打开管理端后使用该 token 认证。不要公开或提交 token。管理端目前仅注册 `User` 的只读列表/详情；`openid` 和明文手机号被隐藏，会话和协议同意表未开放。

二进制服务通过 `afast-http` 提供，默认监听 `0.0.0.0:6080`。小程序真机调试时，不能把服务端地址配置成 `localhost`，应使用开发机局域网地址。正式环境须配置 HTTPS 和小程序合法 request 域名。

## MySQL 8 与 ORM

AFaster 与 afastorm 均启用 MySQL ORM 驱动。连接配置位于 `[orm-mysql]`，包括主机、端口、用户名、密码和数据库名；当前目标数据库为 MySQL 8。远程连接启用了 SQLx Rustls 系统根证书验证。迁移文件位于 `migrations/mysql/`，SQLite 历史迁移保留在 `migrations/` 根目录，不用于 MySQL。

认证功能使用 AFaster 的 `state.wxlogin.mini_login()` 完成小程序 code 换 openid。Docker 配置将 `[wxlogin].base_url` 设为 `http://api.weixin.qq.com`，经云托管开放接口调用，避开其 HTTPS 自签名证书与 Rustls 的兼容问题；本地配置不指定时使用 AFaster 默认 HTTPS 地址。手机号授权仍使用 `get_user_phone_number()`。账号、授权同意记录和会话通过 afastorm Models/Repository 管理。手机号以明文保存在 `users.phone_number`，并建立唯一约束；脱敏号码仅在接口响应时计算，不额外存储。管理端隐藏原始手机号。若号码已绑定其他账号，用户确认后可转移绑定；旧账号解绑和当前账号绑定在同一 ORM 事务中完成。请限制数据库账号权限、管理端访问和备份访问。

MySQL 的 `1795620000000000000_recreate_auth_tables_with_plain_phone` 迁移会删除并重建认证表，应用时会清空用户、会话和协议同意记录。
`1795630000000000000_remove_stored_phone_mask` 会删除冗余的 `phone_masked` 列，API 仍返回即时计算的脱敏号码。

小程序 AppID/AppSecret 可通过 `WXLOGIN_MINI_ID` 和 `WXLOGIN_MINI_SECRET` 环境变量配置。
微信云托管对象存储通过 AFaster `wx-cloudrun` state 管理。`config.docker.toml` 的 `[wxcloudrun]` 配置环境 ID、上传目录前缀及临时访问链接有效期；Docker 镜像会将该模板作为运行时 `config.toml`。请在构建镜像前将 `env` 改为实际云托管环境 ID。不开启 COS SecretId/SecretKey，也不读取或返回 `cloudbase_access_token`。

`storage` service 提供需要登录态的 `get_upload_file_link`、`complete_upload`、`get_temporary_access_url`。对象路径由服务端按用户、客户端 MD5 和受限图片扩展名生成，上传记录落在 `uploaded_assets`；需要先应用 `migrations/mysql/1795741200000000000_create_uploaded_assets`。完成上传时，backend 会向 CloudRun 请求临时访问地址确认文件存在，之后按当前用户和 MD5 返回短期 URL。小程序直传 COS 时必须一并提交 `Signature`、`x-cos-security-token` 和 `x-cos-meta-fileid`。

部署前需在微信云托管控制台开启开放接口服务，为 `/tcb/uploadfile` 和 `/tcb/batchdownloadfile` 配置权限，并在开启后重新创建服务版本。开放接口必须从云托管实例或已配置的本地开发代理调用；普通本地进程会收到 `41001 access_token missing`，不能据此判断云端上传失败。

```powershell
$env:WXLOGIN_MINI_ID = "<小程序 AppID>"
$env:WXLOGIN_MINI_SECRET = "<小程序 AppSecret>"
```

新增模型和迁移时使用 ORM 元数据生成迁移，不要手写 `.up.sql` / `.down.sql`。已有数据库首次接入 schema diff 时，先确认历史迁移已应用，再使用 ORM 命令写入一次 schema 快照基线：

```powershell
cargo run -- orm baseline --dir migrations/mysql
cargo run -- orm makemigrations <name> --dir migrations/mysql
cargo run -- orm migrate --dir migrations/mysql
```

基线命令不会连接或修改数据库，也不会生成迁移 SQL；它只记录当前注册模型的 schema，不会检查实际数据库结构。历史 SQL 中存在 ORM 模型尚未表达的字段长度、字符集/排序规则或默认值；因此生成会影响这些旧字段的迁移前，必须先审计并对齐模型和数据库定义。之后模型变化时，`orm makemigrations` 会根据基线生成 up/down SQL，检查生成结果后再用 `orm migrate` 应用。历史迁移已创建 `(user_id, file_md5)` 唯一索引；ORM 元数据使用新的索引名，生成的迁移会安全地创建新索引后删除旧索引，不改变唯一约束字段。迁移命令按历史版本执行未应用文件，执行前应确认目标配置和迁移状态，尤其不要对已有数据的数据库重复执行早期重建认证表迁移。

backend 依赖 crates.io 的 AFaster 0.0.16 / afastorm 0.0.3。上传资产的 `(user_id, file_md5)` 联合唯一索引在模型上通过 `#[orm(index(user_id, file_md5), unique)]` 声明。该声明生成的索引名为 `uq_uploaded_assets_user_id_file_md5`；从旧索引名升级时，使用 `orm makemigrations` 生成重命名迁移，审查后再用 `orm migrate` 应用。

物品列表封面及物品详情图片通过 COS 数据万象下载时处理添加“换物集”半透明水印；原始对象不修改。用户上传后的临时预览地址不加水印。需确保对应 COS 存储桶已启用数据万象图片处理；此服务可能产生费用。

## Docker

backend 使用 crates.io 上的 AFaster 0.0.16，可从独立 backend 仓库根目录构建。Dockerfile 和 `Dockerfile.dockerignore` 已按仓库根目录构建上下文配置：

```powershell
docker buildx build --load -f .\Dockerfile -t huanhuan-backend .
```

`Dockerfile.dockerignore` 仅保留构建所需的 Cargo 文件、`src/`、迁移 SQL 和 Docker 配置模板，并排除数据库、日志、构建产物、Git 元数据、本地配置和环境变量文件。MySQL 和微信凭据由环境变量提供；运行时应设置 `ORM_ADMIN_TOKEN`。AFaster 支持配置文件缺失时从环境变量加载配置。服务启动不生成小程序客户端；仅在本地需要更新接口 JS 时运行 `cargo run --bin generate_api`。

CloudRun 镜像中的 `[wxlogin].base_url` 配置为 `http://api.weixin.qq.com`，使用开放接口服务时无需处理 HTTPS 自签名证书。请在云托管控制台启用开放接口，并在启用后重新构建、部署服务版本。`WXLOGIN_BASE_URL` 环境变量优先于 TOML 配置；云端应确认该变量未覆盖为 HTTPS。

先应用数据库迁移，再启动服务（两条命令需使用相同环境变量）。为手机号、微信和管理端配置密钥时，请通过安全的容器 secret 管理，不要提交到仓库：

```powershell
docker run --rm --env-file .\backend.env `
  huanhuan-backend orm migrate --dir migrations/mysql

docker run -d --name huanhuan-backend -p 6080:6080 `
  --env-file .\backend.env `
  huanhuan-backend
```

## 接口生成

客户端使用 `wx.request` 传输二进制协议。生成器代码在 `src/bin/generate_api.rs`，每个服务只生成 JS 客户端，输出目录由 `client_target()` 定义。

当前生成配置开启 debug，客户端会在控制台记录每次请求参数和响应结果。登录响应包含访问 token；调试日志可能将其写入开发者工具控制台。不要在共享/生产环境留存或转发这些日志，发布前可关闭 `client_target()` 中的 debug，或生成后在客户端构造参数中设置 `debug: false`。

后端启用 AFaster 的 `log` feature，日志输出到控制台和 `logs/`。注意微信登录请求会把 AppSecret 和一次性 code 放在 URL 查询参数中，AFaster 的请求错误日志可能包含完整 URL；请勿分享原始日志，并按需配置日志访问权限和保留期限。

生成的 JS 是 ESM 模块，并使用 `TextEncoder` / `TextDecoder` 编解码字符串。接入微信运行时前，请确认目标基础库提供这两个 API，或为小程序配置相应 polyfill。
