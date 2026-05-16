#set text(size: 10pt)

= API 接口文档 v2.0

== 认证

所有请求需要在 Header 中携带 `Authorization: Bearer <token>`。

=== 获取 Token

```
POST /api/v2/auth/token
Content-Type: application/json

{
  "client_id": "your_client_id",
  "client_secret": "your_secret"
}
```

返回：
```json
{
  "access_token": "eyJhbGci...",
  "expires_in": 3600,
  "token_type": "Bearer"
}
```

== 用户管理

=== 获取用户列表

```
GET /api/v2/users?page=1&limit=20
```

#table(
  columns: (1fr, 1fr, 3fr),
  table.header([*参数*], [*类型*], [*说明*]),
  [`page`], [int], [页码，从 1 开始],
  [`limit`], [int], [每页数量，默认 20，最大 100],
  [`role`], [string], [按角色过滤：`admin`、`user`、`viewer`],
)

=== 错误码

#table(
  columns: (1fr, 2fr, 2fr),
  table.header([*状态码*], [*错误类型*], [*说明*]),
  [400], [Bad Request], [请求参数格式错误],
  [401], [Unauthorized], [Token 无效或过期],
  [403], [Forbidden], [权限不足],
  [404], [Not Found], [资源不存在],
  [429], [Rate Limited], [请求频率超限（60次/分钟）],
  [500], [Internal Error], [服务器内部错误],
)

== 数据模型

用户对象结构：

```json
{
  "id": "usr_abc123",
  "name": "张三",
  "email": "zhang@example.com",
  "role": "admin",
  "created_at": "2026-01-15T08:30:00Z"
}
```

> *注意*：`id` 字段使用前缀格式（`usr_` 前缀），请勿依赖其内部结构。
