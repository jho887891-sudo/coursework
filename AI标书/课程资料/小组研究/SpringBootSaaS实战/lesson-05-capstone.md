# Day 5：SaaS 核心机制独立 Demo

> 前 4 天你理解了 MyBatis-Plus、多租户、JWT+SSE、异步队列+Rust 代理。今天你从零组装一个 Docker Compose 一键启动的完整 SaaS 平台。

---

## 目标

搭建标书审核 SaaS 业务平台——多租户注册登录、项目管理、审核任务生命周期（PENDING→PROCESSING→COMPLETED）、SSE 实时推送、统一异常处理、Docker Compose 部署（MySQL+Redis+App）。

---

## 架构

```
docker compose up
    │
    ├── mysql:8          ← 数据存储（Druid 连接池）
    ├── redis:7          ← 缓存/队列
    └── app:8080         ← Spring Boot 3.2
           │
           ├── JwtTokenAdminInterceptor  ← 认证
           ├── BaseContext (ThreadLocal) ← 上下文
           ├── MyBatis-Plus (TenantLine) ← 多租户
           ├── SseHub (ConcurrentHashMap) ← 实时推送
           ├── AuditTaskDispatcher (Strategy) ← 任务队列
           └── RustApiClient → Rust Engine (:3001) ← AI 代理
```

---

## 大作业需求

### P0：完整 SaaS 核心链路（60 分）

| 功能 | 技术 |
|------|------|
| 注册/登录 | JWT 签发 → 返回 token + user |
| 多租户隔离 | BaseContext + MyBatis-Plus TenantLineInnerInterceptor |
| 项目管理 | CRUD (BaseMapper + LambdaQueryWrapper + 分页) |
| 审核任务 | create→PENDING→@Async dispatch→PROCESSING→COMPLETED |
| SSE 推送 | SseHub → Controller 返回 SseEmitter |
| 统一异常 | BizException → @RestControllerAdvice → Result<T> |
| Flyway 建表 | V1__init.sql 自动执行 |
| 健康检查 | /actuator/health → MySQL up, Redis up |

API 规范：
```json
// 所有响应统一格式
{ "code": 200, "message": "success", "data": {...}, "timestamp": 1720000000000 }

// POST /api/auth/login → { "code":200, "data": { "token":"eyJ...", "user":{...} } }
// POST /api/projects → { "code":200, "data": { "id":1, "name":"XX市政工程" } }
// POST /api/audit-tasks → { "code":200, "data": { "id":42, "status":"PENDING" } }
// GET /api/audit-tasks/42/stream → SSE events
```

### P1：Strategy 切换 + 回调（20 分）

- `application.yml` 切换 `audit.task.dispatcher` → 验证 Async/RedisList/RedisStream 三种都能跑
- Rust Mock Engine 回调 `POST /api/audit-tasks/callback` → 更新状态 → SSE 推送 complete

### P2：Docker Compose + Druid 监控（20 分）

- `docker compose up -d` → 三服务健康
- Druid 监控页 `/druid/sql.html` 可访问
- Actuator metrics: `/actuator/metrics/http.server.requests`

---

## 验收标准

| 验收项 | 权重 | 怎么测 |
|--------|------|--------|
| JWT 认证 | 10% | 登录→token→受保护 API→401 |
| 多租户隔离 | 10% | 两个租户互不可见 |
| 审核生命周期 | 20% | create → PENDING → PROCESSING → COMPLETED |
| SSE 推送 | 15% | curl 监听流 → 收到 progress + finding + complete |
| 统一异常 | 10% | 400/401/404/500 → Result<T> 格式 |
| Flyway 建表 | 10% | 首次启动自动建表 |
| Docker Compose | 15% | `docker compose up` → 全部健康 |
| 设计决策文档 | 10% | 为什么不 Security/为什么 MyBatis-Plus/为什么薄网关 |

---

## 设计决策文档

1. **为什么不用 Spring Security，用自定义 HandlerInterceptor？** — 纯 API 应用不需要 Session/CSRF/formLogin 等 Spring Security 预设。自定义拦截器 400 行代码，意图清晰，零否定配置
2. **为什么选 MyBatis-Plus 而不是 JPA？** — 项目需要复杂的分页+多租户+JSON列+批量操作。MyBatis-Plus 的 LambdaQueryWrapper + TypeHandler + 分页插件比 JPA 的 Criteria API 更简洁。复杂查询直接用 SQL——性能可控
3. **为什么 Java 是薄网关？** — AI 审核逻辑在 Rust 引擎中（已有 8000+ 行代码）。Java 层只负责认证/CRUD/任务调度/SSE 推送。薄网关也意味着：即使 Rust 引擎宕机，用户仍可登录、上传文件、查看历史报告
4. **为什么 Redis Streams 而不是 RabbitMQ？** — 已部署 Redis（缓存+Session+限流），Streams 零额外运维。MV 阶段日审核任务 < 1000，够用。Phase 2 若需复杂路由再迁 RabbitMQ

---

## 与标书审核项目的关系

这个 Demo 使用独立数据库、Mock Rust 服务和少量测试账号，验证薄网关、多租户、事务时序、任务状态与 SSE 的核心机制。项目 `backend-java/` 只用于只读对照，实验代码不写入现有业务目录。

小组必做只需完成：

1. 两个租户的数据隔离实验；
2. 一个任务从创建到完成的状态流转；
3. 一次事务提交前后分发的对照；
4. 一个 SSE 断线或重放实验。

Docker Compose、三种队列和完整 CRUD 页面作为骨干选做。
