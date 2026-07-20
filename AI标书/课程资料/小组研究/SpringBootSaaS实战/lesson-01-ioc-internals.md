# Day 1：Spring Boot 3 + MyBatis-Plus — IoC 内核与 ORM 选择

> 项目没用 JPA/Hibernate——用了 MyBatis-Plus。今天你理解 IoC 容器的 refresh() 13 步，然后深入 MyBatis-Plus 的 LambdaQueryWrapper、分页、TypeHandler、乐观锁——以及为什么项目选它而不是 JPA。

---

## 学习目标

1. 理解 IoC 容器 refresh() 13 步和 AOP 代理机制
2. 掌握 MyBatis-Plus BaseMapper / LambdaQueryWrapper / 分页 / TypeHandler / @Version
3. 理解 MyBatis-Plus 与 JPA 的架构差异和选择理由

---

## 核心概念

### 1. IoC 容器与 AOP（同 React 课程 Day 1 深度）

IoC 的核心是 `AbstractApplicationContext.refresh()`——13 个步骤。与 AOP 的关系：Step 6 注册 `BeanPostProcessor`→Step 11 实例化 singleton Bean→BeanPostProcessor.postProcessAfterInitialization() 生成代理。

**项目中的 AOP 应用**：`@Transactional` 在 Service 层。`@Async` 在任务分发。

### 2. MyBatis-Plus — 为什么不用 JPA

项目选 MyBatis-Plus 的三个核心原因：

```
JPA 的问题：
  1. 复杂查询必须用 @Query 手写 JPQL → JPQL 不是 SQL → 不支持 LIMIT/OFFSET 方言
  2. @OneToMany 自动 JOIN → N+1 查询陷阱
  3. EntityManager 自动 flush → 你不知道 SQL 什么时候执行

MyBatis-Plus 的优势：
  1. BaseMapper<T> 提供 17 个内置 CRUD → 一行 SQL 不用写
  2. LambdaQueryWrapper → 类型安全的动态查询（字段名是编译期检查的）
  3. 复杂查询直接写 SQL（@Select 注解或 XML mapper）→ 性能完全可控
```

### 3. MyBatis-Plus 核心 API

#### BaseMapper<T> — 内置 CRUD

```java
@Mapper
public interface AuditTaskMapper extends BaseMapper<AuditTask> {
    // 不需要写任何方法——BaseMapper 自带：
    // insert(T), deleteById(id), updateById(T), selectById(id), selectList(wrapper)
    // 17 个内置方法，覆盖 90% 的 CRUD 场景
}

// 使用
@Autowired
private AuditTaskMapper taskMapper;

// 插入
AuditTask task = AuditTask.builder().projectId(1L).status("PENDING").build();
taskMapper.insert(task);  // task.id 自动回填

// 查询
AuditTask task = taskMapper.selectById(42L);
```

#### LambdaQueryWrapper — 类型安全查询

```java
// ❌ 传统方式：字段名是字符串，拼写错误只在运行时发现
QueryWrapper<AuditTask> qw = new QueryWrapper<>();
qw.eq("status", "PENDING").gt("created_at", yesterday);

// ✅ LambdaQueryWrapper：字段名引用自实体类的 getter，编译期检查
LambdaQueryWrapper<AuditTask> lqw = new LambdaQueryWrapper<>();
lqw.eq(AuditTask::getStatus, "PENDING")
   .gt(AuditTask::getCreatedAt, yesterday)
   .orderByDesc(AuditTask::getCreatedAt);

List<AuditTask> tasks = taskMapper.selectList(lqw);
```

#### 分页插件

```java
// MybatisPlusConfig.java — 项目中实际配置
@Bean
public MybatisPlusInterceptor mybatisPlusInterceptor() {
    MybatisPlusInterceptor interceptor = new MybatisPlusInterceptor();
    interceptor.addInnerInterceptor(new PaginationInnerInterceptor(DbType.MYSQL));
    interceptor.addInnerInterceptor(new OptimisticLockerInnerInterceptor());  // @Version
    return interceptor;
}

// 使用
Page<AuditTask> page = new Page<>(1, 10);  // 第 1 页，每页 10 条
Page<AuditTask> result = taskMapper.selectPage(page,
    new LambdaQueryWrapper<AuditTask>()
        .eq(AuditTask::getStatus, "COMPLETED")
        .orderByDesc(AuditTask::getCreatedAt));

result.getRecords();  // 当前页数据
result.getTotal();    // 总条数
result.getPages();    // 总页数
```

#### 乐观锁 @Version

```java
@Data
@TableName("audit_task")
public class AuditTask {
    @Version
    private Integer version;  // 每次 update 自动 +1，WHERE version = oldVersion
}

// SQL 自动变为：
// UPDATE audit_task SET ... , version = version + 1
// WHERE id = ? AND version = ?  ← 如果 version 不对（被其他线程改了），更新 0 行 → BizException
```

项目审计任务的生命周期变更依赖乐观锁——Worker 认领任务时用 `@Version` 防止两个 Worker 同时认领同一个任务。

#### TypeHandler — JSON 列映射

```java
// 项目中：audit_task.enabled_checks 是 JSON 列 → List<String>
// 用 TypeHandler 自动转换

@TableName(value = "audit_task", autoResultMap = true)
public class AuditTask {
    @TableField(typeHandler = StringListJsonTypeHandler.class)
    private List<String> enabledChecks;  // ["qualification", "pricing", "safety"]
}

// StringListJsonTypeHandler extends BaseTypeHandler<List<String>>
// 写入：List<String> → FastJSON → JSON string → DB
// 读取：DB JSON string → FastJSON → List<String>
```

---

### 4. 项目编码约定

```java
// Entity：全部 Lombok
@Data
@Builder
@NoArgsConstructor
@AllArgsConstructor
@TableName("audit_task")
public class AuditTask {
    @TableId(type = IdType.AUTO)
    private Long id;
    private Long tenantId;
    private String status;
    @Version
    private Integer version;
    private LocalDateTime createdAt;
}

// Mapper：继承 BaseMapper，复杂查询手写注解 SQL
@Mapper
public interface AuditTaskMapper extends BaseMapper<AuditTask> {
    @Select("SELECT * FROM audit_task WHERE tenant_id = #{tenantId} AND status = #{status}")
    List<AuditTask> findByTenantAndStatus(@Param("tenantId") Long tenantId,
                                           @Param("status") String status);
}

// Service：接口 + 实现分离（项目强制规范）
public interface AuditTaskService {
    AuditTask createTask(CreateTaskDTO dto);
    AuditTaskVO getTask(Long taskId);
}
@Service
public class AuditTaskServiceImpl implements AuditTaskService { ... }
```

---

## 动手

### 任务 1：IoC + @Transactional 陷阱

复现 `this.transactionalMethod()` 不生效→用 `@Autowired self` 修复→理解代理 vs 原始对象。

### 任务 2：MyBatis-Plus CRUD + 分页

创建 `AuditTask` 实体→实现 Mapper→用 LambdaQueryWrapper 按状态+时间过滤→分页查询→验证分页 SQL（`LIMIT ?,?`）。

### 任务 3：TypeHandler 实现

实现 `StringListJsonTypeHandler`（继承 `BaseTypeHandler<List<String>>`）→自动将 `["a","b"]` 序列化为 JSON 写入 DB，读取时反序列化回 List。

---

## 验收标准

- [ ] IoC + AOP 实验：复现失败 + 修复
- [ ] MyBatis-Plus 分页 SQL 正确
- [ ] TypeHandler 正确序列化/反序列化 JSON 列

---

## 思考题

1. MyBatis-Plus 的 `LambdaQueryWrapper` 用 `AuditTask::getStatus` 作为字段引用——这是 Java 的什么特性？如果 getter 改名了会发生什么？
2. JPA 的自动 flush 在长事务中可能导致什么性能问题？MyBatis-Plus 怎么避免？
3. `@Version` 乐观锁在并发冲突时应该重试还是直接抛异常？

---

## 与标书审核项目的关系

你今天写的 `AuditTaskMapper` 对标项目 `mapper/AuditTaskMapper.java`。LambdaQueryWrapper 在项目中广泛使用——Service 层的所有动态查询都用它。TypeHandler 已经在项目 `common/typehandler/` 中实现——你今天的实现就是理解它的原理。
