# Day 4：异步任务队列 + Rust AI 引擎代理

> Day 3 你实现了 JWT + SSE。今天你让审核任务异步执行——用户点击"开始审核"后立即返回，后台 Worker 消费队列，调用 Rust 引擎，结果通过 SSE 推送到前端。项目用了 Strategy 模式：3 种队列实现，通过一个配置切换。

---

## 学习目标

1. 理解审核任务生命周期的 4 个阶段
2. 实现 Strategy 模式的任务分发器（Async/RedisList/RedisStream）
3. 实现 RustApiClient（同步 HTTP）和 RustSseClient（SSE 事件转发）
4. 实现 @Transactional afterCommit 触发任务分发的模式

---

## 核心概念

### 1. 审核任务生命周期

```
POST /api/audit-tasks { projectId, fileKey }
  → Controller: taskService.createTask(dto)
  → Service: INSERT audit_task (status=PENDING)
  → @Transactional commit
  → afterCommit: taskDispatcher.dispatch(taskId)   ← ★ 事务提交后才分发！
  → Worker: status → PROCESSING → 调 Rust API → COMPLETED/FAILED

为什么 afterCommit？
  如果任务分发在事务提交前 → Worker 可能读到 PENDING 但事务回滚了 → 
  任务永远卡在 PENDING（数据库中没有这条记录）。
  afterCommit 保证：Worker 看到的任务一定已经持久化了。
```

```java
@Service
public class AuditTaskServiceImpl implements AuditTaskService {
    
    @Override
    @Transactional
    public AuditTask createTask(CreateTaskDTO dto) {
        // 1. 写数据库
        AuditTask task = AuditTask.builder()
            .projectId(dto.getProjectId())
            .status("PENDING")
            .tenantId(BaseContext.getCurrentTenantId())
            .build();
        taskMapper.insert(task);
        
        // 2. 事务提交后分发
        TransactionSynchronizationManager.registerSynchronization(
            new TransactionSynchronization() {
                @Override
                public void afterCommit() {
                    taskDispatcher.dispatch(task.getId());
                }
            }
        );
        
        return task;
    }
}
```

### 2. Strategy 模式 — 三种队列实现

```java
// 接口
public interface AuditTaskDispatcher {
    void dispatch(Long taskId);
}

// 实现 1：@Async 线程池（默认，最简单）
@Component
@ConditionalOnProperty(name = "audit.task.dispatcher", havingValue = "async", matchIfMissing = true)
public class AsyncAuditTaskDispatcher implements AuditTaskDispatcher {
    @Override
    @Async
    public void dispatch(Long taskId) {
        auditEngineService.start(taskId);  // 在新线程中执行审核
    }
}

// 实现 2：Redis List（BLPOP 轮询）
@Component
@ConditionalOnProperty(name = "audit.task.dispatcher", havingValue = "redis-list")
public class RedisListAuditTaskDispatcher implements AuditTaskDispatcher {
    @Autowired
    private StringRedisTemplate redis;
    
    @Override
    public void dispatch(Long taskId) {
        redis.opsForList().leftPush("audit:task:queue", taskId.toString());
        // Worker 线程在另一个类中 BLPOP 消费
    }
}

// 实现 3：Redis Streams（Consumer Group + DLQ）
@Component
@ConditionalOnProperty(name = "audit.task.dispatcher", havingValue = "redis-stream")
public class RedisStreamAuditTaskDispatcher implements AuditTaskDispatcher {
    @Override
    public void dispatch(Long taskId) {
        Map<String, String> msg = Map.of("taskId", taskId.toString());
        redis.opsForStream().add("audit:tasks", msg);
        // Worker 在 RedisStreamAuditTaskWorker 中 XREADGROUP 消费
    }
}
```

### Redis Streams Worker

```java
@Component
public class RedisStreamAuditTaskWorker {
    private volatile boolean running = true;
    
    @PostConstruct
    public void start() {
        new Thread(this::consumeLoop, "audit-stream-worker").start();
    }
    
    private void consumeLoop() {
        while (running) {
            List<MapRecord<String, Object, Object>> messages = redis.opsForStream()
                .read(Consumer.from("audit-workers", "worker-1"),
                      StreamReadOptions.empty().count(1).block(Duration.ofSeconds(5)),
                      StreamOffset.create("audit:tasks", ReadOffset.lastConsumed()));
            
            if (messages == null) continue;
            
            for (MapRecord<String, Object, Object> msg : messages) {
                Long taskId = Long.parseLong(msg.getValue().get("taskId").toString());
                try {
                    auditEngineService.start(taskId);
                    redis.opsForStream().acknowledge("audit:tasks", "audit-workers", msg.getId());
                } catch (Exception e) {
                    // 重试逻辑 → 超过 maxRetries → XADD audit:dead-letter
                    handleFailure(msg, e);
                }
            }
        }
    }
}
```

### 3. Rust 透明代理

项目 Java 层是**薄网关**——不承载 AI 逻辑。审核引擎全在 Rust（端口 3001）：

```java
@Service
public class RustApiClient {
    private final HttpClient httpClient = HttpClient.newHttpClient();
    private final String rustBaseUrl;
    
    // 同步调用 Rust API
    public String uploadDocument(Long taskId, byte[] fileContent) {
        HttpRequest request = HttpRequest.newBuilder()
            .uri(URI.create(rustBaseUrl + "/api/review/documents"))
            .POST(HttpRequest.BodyPublishers.ofByteArray(fileContent))
            .build();
        HttpResponse<String> response = httpClient.send(request, HttpResponse.BodyHandlers.ofString());
        return response.body();  // document_id
    }
}
```

```java
// RustSseClient：监听 Rust SSE 流 → 转发到 SseHub
public void connectAndRelay(Long taskId, String rustTaskId) {
    HttpRequest request = HttpRequest.newBuilder()
        .uri(URI.create(rustBaseUrl + "/api/review/" + rustTaskId + "/stream"))
        .GET()
        .build();
    
    httpClient.sendAsync(request, HttpResponse.BodyHandlers.ofLines())
        .thenAccept(response -> {
            response.body().forEach(line -> {
                if (line.startsWith("data:")) {
                    AuditTaskEvent event = parseRustEvent(line);
                    auditTaskEventService.save(taskId, event);  // 持久化
                    sseHub.emit(taskId.toString(), event);      // 实时推送
                    
                    if ("done".equals(event.getType())) {
                        fetchResultAndComplete(taskId, rustTaskId);
                    }
                }
            });
        });
}
```

### 4. 审核引擎的 4 个阶段

```java
@Service
public class AuditEngineServiceImpl {
    
    public void start(Long taskId) {
        // Stage 1: 上传文档到 Rust 引擎（幂等——同名文件跳过）
        String rustDocId = documentService.ensureUploaded(taskId);
        
        // Stage 2: 提交审核任务 → Rust 返回 202 Accepted
        rustApiClient.submitReview(rustDocId);
        
        // Stage 3: 监听 Rust SSE 流 → 转发事件到 Java SseHub
        rustSseClient.connectAndRelay(taskId, rustDocId);
        
        // Stage 4: Rust 发送 "done" → GET /api/review/{id}/result
        // → 解析结果 → 更新 DB status=COMPLETED → emit COMPLETE SSE
    }
}
```

---

## 动手

### 任务 1：@Async 任务分发

实现 `AsyncAuditTaskDispatcher`→`@Transactional` 写 PENDING→`afterCommit` 分发→Worker 中模拟审核处理（sleep 3s）→更新 COMPLETED→SSE 推送 complete 事件。

### 任务 2：Redis Streams Worker

创建 ConsumerGroup→XREADGROUP 消费→ACK→模拟 Worker 宕机→另一 Worker XCLAIM 认领超时消息→重试→超 maxRetries→进入 DLQ。

### 任务 3：Rust Mock 代理

创建 Mock Rust 服务（简单的 HTTP endpoint）→`RustApiClient` 调用→`RustSseClient` 转发 SSE 事件→验证事件同时出现在 SseHub 和 audit_task_event 表中。

---

## 验收标准

- [ ] @Async 任务分发 + afterCommit 正确
- [ ] Redis Streams ACK + XCLAIM + DLQ 链路完整
- [ ] Rust 代理正确转发事件到数据库 + SseHub

---

## 思考题

1. `afterCommit` 保证"Worker 看到的数据一定已持久化"。但如果 `afterCommit` 回调本身失败了（如 Redis 挂了）——任务永远卡在 PENDING。怎么解决？
2. Strategy 模式用 `@ConditionalOnProperty` 切换。如果三个实现中任意一个都满足 `@ConditionalOnProperty`——Spring 会选哪个？
3. RustSseClient 用 `HttpClient.sendAsync` 监听流——如果 Rust 挂了，流中断了，怎么重连？

---

## 与标书审核项目的关系

项目的 `service/engine/queue/` 下有三种 TaskDispatcher 实现。`service/engine/rust/` 下有 RustApiClient/RustSseClient/RustDocumentService。CLAUDE.md 的第 2 节"审核任务生命周期"详细描述了这 4 个阶段——你今天的实现就是它的原理版。
