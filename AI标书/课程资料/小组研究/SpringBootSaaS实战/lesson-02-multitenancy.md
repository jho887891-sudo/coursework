# Day 2：多租户 + BaseContext + Druid + Flyway

> Day 1 你理解了 MyBatis-Plus。今天你让它在所有 SQL 上自动加上 `WHERE tenant_id = ?`——开发者不需要记得多租户，MyBatis-Plus 拦截器自动注入。

---

## 学习目标

1. 实现 ThreadLocal 的 BaseContext 并在 Filter 中自动提取+清理
2. 用 MyBatis-Plus TenantLineInnerInterceptor 实现透明多租户
3. 配置 Druid 连接池的 SQL 监控和慢查询告警
4. 用 Flyway 管理数据库版本迁移

---

## 核心概念

### 1. BaseContext — 项目已有的 ThreadLocal

```java
// 项目 common/BaseContext.java — 当前用户的"请求级全局变量"
public class BaseContext {
    private static final ThreadLocal<Long> THREAD_LOCAL = new ThreadLocal<>();
    
    public static void setCurrentId(Long id) { THREAD_LOCAL.set(id); }
    public static Long getCurrentId() { return THREAD_LOCAL.get(); }
    public static void removeCurrentId() { THREAD_LOCAL.remove(); }
}
```

**与 JWT 拦截器的协作**：

```java
// JwtTokenAdminInterceptor.preHandle()
Long userId = JwtUtil.getUserIdFromToken(token);
BaseContext.setCurrentId(userId);  // 写入上下文
// ... filterChain 执行 ...
// afterCompletion() → BaseContext.removeCurrentId();  // 必须清理！
```

**为什么必须 `remove()`**：Tomcat 线程池复用。线程处理完请求 A（用户 42）后如果不清理，下次复用处理请求 B（用户 99）时 `getCurrentId()` 还是 42。

### 2. MyBatis-Plus 多租户拦截器

MyBatis-Plus 提供了 `TenantLineInnerInterceptor`——自动在所有 SQL 上追加租户过滤条件：

```java
@Bean
public MybatisPlusInterceptor mybatisPlusInterceptor() {
    MybatisPlusInterceptor interceptor = new MybatisPlusInterceptor();
    
    // 多租户拦截器
    interceptor.addInnerInterceptor(new TenantLineInnerInterceptor(new TenantLineHandler() {
        @Override
        public Expression getTenantId() {
            // 从 BaseContext 获取当前请求的租户ID
            Long tenantId = BaseContext.getCurrentTenantId();
            if (tenantId == null) {
                throw new BizException(401, "未登录或租户信息缺失");
            }
            return new LongValue(tenantId);
        }
        
        @Override
        public String getTenantIdColumn() {
            return "tenant_id";  // 数据库列名
        }
        
        @Override
        public boolean ignoreTable(String tableName) {
            // 全局配置表不需要租户隔离
            return List.of("sys_config", "flyway_schema_history").contains(tableName);
        }
    }));
    
    // 分页插件（Day 1）+ 乐观锁插件
    interceptor.addInnerInterceptor(new PaginationInnerInterceptor(DbType.MYSQL));
    interceptor.addInnerInterceptor(new OptimisticLockerInnerInterceptor());
    
    return interceptor;
}
```

**效果**：

```sql
-- 原始 SQL（你写的）
SELECT * FROM audit_task WHERE status = 'PENDING'

-- 拦截器自动改写为
SELECT * FROM audit_task WHERE status = 'PENDING' AND tenant_id = 42
```

开发者不需要手动写 `tenant_id`——拦截器保证永远不会跨租户查询。

### 3. Druid 连接池 — 不只是连接池

项目用 Druid 而不是 HikariCP（Spring Boot 默认）。Druid 的核心价值：

**SQL 监控**：访问 `/druid/sql.html` → 实时看到所有 SQL 的执行时间、执行次数、并发数、慢 SQL 列表。

**Wall 过滤器**——防止 SQL 注入：

```yaml
spring:
  datasource:
    druid:
      filters: stat,wall    # stat=SQL监控, wall=SQL注入防御
      stat-view-servlet:
        enabled: true       # 开启监控页面 /druid/*
        login-username: admin
        login-password: admin
      filter:
        wall:
          config:
            multi-statement-allow: false  # 禁止批量 SQL
```

**慢 SQL 日志**：配置 `connectionProperties: druid.stat.slowSqlMillis=1000` → 所有超过 1 秒的 SQL 打印到日志。

### 4. Flyway — 数据库版本迁移

在项目 `pom.xml` 中：`flyway-core` + `flyway-mysql`。Spring Boot 自动检测 `classpath:db/migration/` 下的 SQL 脚本：

```
src/main/resources/db/migration/
├── V1__init_schema.sql        ← 初始建表
├── V2__add_audit_report.sql   ← 新增审核报告表
├── V3__add_tenant_id.sql      ← 添加租户列
└── V4__seed_data.sql          ← 初始化种子数据
```

```sql
-- V1__init_schema.sql
CREATE TABLE IF NOT EXISTS sys_user (
  id BIGINT AUTO_INCREMENT PRIMARY KEY,
  username VARCHAR(50) NOT NULL UNIQUE,
  password_hash VARCHAR(64) NOT NULL,
  tenant_id BIGINT NOT NULL,
  created_at DATETIME DEFAULT CURRENT_TIMESTAMP
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE IF NOT EXISTS audit_task (
  id BIGINT AUTO_INCREMENT PRIMARY KEY,
  project_id BIGINT NOT NULL,
  tenant_id BIGINT NOT NULL,
  status VARCHAR(20) NOT NULL DEFAULT 'PENDING',
  version INT DEFAULT 0,
  created_at DATETIME DEFAULT CURRENT_TIMESTAMP
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
```

Flyway 在 `flyway_schema_history` 表中记录已执行的脚本——启动时只执行新脚本，已执行的不会重复执行。

---

## 动手

### 任务 1：BaseContext + Filter

实现完整的 Filter→提取 JWT tenant_id→写入 BaseContext→finally 清理。用两个请求验证隔离性。

### 任务 2：MyBatis-Plus 租户拦截器

配置 `TenantLineInnerInterceptor`。写一个 `SELECT * FROM projects` → 查看 SQL 日志确认自动加了 `WHERE tenant_id = ?`。配置忽略表——验证 `sys_config` 表不被过滤。

### 任务 3：Druid 监控 + Flyway

启用 Druid 监控页→跑 10 条查询→在 `/druid/sql.html` 中看到统计。写 2 个 Flyway 迁移脚本→启动→验证 `flyway_schema_history` 表记录了执行日志。

---

## 验收标准

- [ ] BaseContext 在两个请求的线程中互不污染
- [ ] MyBatis-Plus SQL 日志确认自动注入 `WHERE tenant_id = ?`
- [ ] Druid 监控页可访问、SQL 统计正确
- [ ] Flyway 自动建表、重复启动不重复执行

---

## 思考题

1. ThreadLocal 在线程池中复用——为什么 `finally { remove() }` 还不够？（提示：`@Async` 方法在新线程中，父线程的 ThreadLocal 值不会自动拷贝）
2. MyBatis-Plus 的 `TenantLineInnerInterceptor` 在 `SELECT` 中自动加 `WHERE tenant_id = ?`。但 `INSERT` 呢？谁来设置 `tenant_id`？
3. Druid 的 Wall 过滤器怎么检测 SQL 注入？它的 `multi-statement-allow=false` 能阻止哪种攻击？

---

## 与标书审核项目的关系

项目的 `BaseContext` 在 `common/BaseContext.java`。MyBatis-Plus 多租户配置在 `config/MybatisPlusConfig.java`。Druid 监控在 `application.yml` 中配置。Flyway 迁移脚本在 `src/main/resources/db/migration/`。你今天的代码就是项目中这些文件的原理版本。
