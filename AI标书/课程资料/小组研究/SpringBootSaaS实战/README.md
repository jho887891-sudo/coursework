# Spring Boot SaaS 平台实战：从 MyBatis-Plus 到 Rust 透明代理

> 5 天深度原理课。通过独立 Spring Boot Demo 理解 MyBatis-Plus、多租户、JWT、SSE、任务队列和 Rust 服务代理；项目代码只读对照。

本课遵循[专项课程统一学习规范](../../专项课程统一学习规范.md)。实验使用独立数据库和 Mock Rust 服务，不修改现有 `backend-java/`。

---

## 你需要准备什么

| 要求 | 说明 |
|------|------|
| Java 基础 | 会用 Spring Boot 写简单 CRUD |
| SQL 基础 | SELECT/INSERT/JOIN |
| Docker | MySQL 8 + Redis 7 |

### 验证环境

```bash
java --version      # ≥ 17
docker --version

# 启动 MySQL + Redis
docker run -d --name mysql-dev -p 3306:3306 \
  -e MYSQL_ROOT_PASSWORD=password mysql:8
docker run -d --name redis-dev -p 6379:6379 redis:7

# 从项目 backend-java/ 启动
cd backend-java
mvn spring-boot:run   # http://localhost:8080
```

---

## 你会学到什么

| 天次 | 主题 | 你会理解或验证什么 |
|------|------|------------|
| Day 1 | **Spring Boot 3 + MyBatis-Plus** | IoC/AOP 内核 + LambdaQueryWrapper + 分页 + TypeHandler |
| Day 2 | **多租户 + ThreadLocal** | BaseContext 实现 + MyBatis-Plus 租户拦截器 + Druid 监控 + Flyway 迁移 |
| Day 3 | **JWT 认证 + SSE 推送** | 自定义 JwtTokenAdminInterceptor + SseHub + 事件持久化 + SSE replay |
| Day 4 | **异步任务队列 + Rust 代理** | Strategy 模式 3 种队列实现 + RustApiClient + RustSseClient |
| Day 5 | **🎓 SaaS 机制 Demo** | Mock 审核生命周期 + 租户隔离 + 任务状态 + SSE 最小闭环 |

---

## 代码怎么写

**Java 17 + Lombok + MyBatis-Plus。** 遵循项目规范：Service 接口+实现分离、统一 `Result<T>` 响应、`BizException` 业务异常、`@RestControllerAdvice` 全局处理。

---

## 独立 Demo 参考架构

```
Controller (@RestController)  ← REST API + Result<T>
    ↓
Service (接口 + impl/)  ← 业务逻辑
    ↓
Mapper (MyBatis-Plus BaseMapper)  ← 数据库访问
    ↓
Entity (@TableName)  ← 数据库表映射

横切层：
├── JwtTokenAdminInterceptor  ← 自定义 JWT 验证（替代 Spring Security）
├── BaseContext (ThreadLocal)  ← 用户上下文
├── GlobalExceptionHandler  ← 统一异常 → Result
├── SseHub  ← ConcurrentHashMap<SseEmitter>
└── RustApiClient / RustSseClient  ← HTTP 代理到 Rust 引擎
```

---

## 参考资源

- [MyBatis-Plus 文档](https://baomidou.com/)
- [Druid 连接池](https://github.com/alibaba/druid)
- [Redis Streams 官方文档](https://redis.io/docs/data-types/streams-tutorial/)
- [Spring Boot Actuator](https://docs.spring.io/spring-boot/docs/current/reference/html/actuator.html)
