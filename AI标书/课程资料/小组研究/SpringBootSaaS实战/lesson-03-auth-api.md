# Day 3：JWT 认证 + SSE 实时推送

> 项目没用 Spring Security——用了 400 行自定义 JWT 拦截器。今天你理解为什么这样选，然后实现 SSE Hub（ConcurrentHashMap<SseEmitter>）——审核进度实时推送到前端，断线了还能补推。

---

## 学习目标

1. 理解为什么项目用 `HandlerInterceptor` 而不是 Spring Security
2. 实现 JWT 签发/验证/解析 + 自定义拦截器
3. 实现 SseHub 实时推送 + SSE 事件持久化 + 断线 replay

---

## 核心概念

### 1. 为什么不用 Spring Security

Spring Security 的架构是"我帮你做一切"——Filter Chain、AuthenticationManager、UserDetailsService、GrantedAuthority。它适合标准 Web 应用（表单登录、Session、CSRF）。

项目的情况：
- 纯 API（无 Session、无 CSRF、无 form login）
- JWT 无状态认证（不需要 `SecurityContextPersistenceFilter`）
- 自定义权限（直接从 JWT claims 读 userId/tenantId，不需要 `GrantedAuthority`）
- MyBatis-Plus 的 ThreadLocal 上下文（不需要 `SecurityContextHolder`）

用 Spring Security 需要大量"禁用"操作——`csrf().disable()`、`sessionManagement().stateless()`、`formLogin().disable()`...最终是一堆否定配置。不如从头写一个 `HandlerInterceptor`——400 行代码，意图清晰，零否定配置。

### 2. JwtTokenAdminInterceptor — 项目真实的认证拦截器

```java
@Component
public class JwtTokenAdminInterceptor implements HandlerInterceptor {
    
    private final JwtUtil jwtUtil;
    
    @Override
    public boolean preHandle(HttpServletRequest request, 
                             HttpServletResponse response, 
                             Object handler) {
        // 1. 从 Header 提取 Token
        String token = extractToken(request);
        if (token == null) {
            response.setStatus(401);
            response.getWriter().write("{\"code\":401,\"message\":\"未登录\"}");
            return false;
        }
        
        // 2. 验证 Token（签名 + 过期）
        if (!jwtUtil.validateToken(token)) {
            response.setStatus(401);
            response.getWriter().write("{\"code\":401,\"message\":\"Token无效或已过期\"}");
            return false;
        }
        
        // 3. 提取 userId 写入 BaseContext
        Long userId = jwtUtil.getUserIdFromToken(token);
        Long tenantId = jwtUtil.getTenantIdFromToken(token);
        BaseContext.setCurrentId(userId);
        BaseContext.setCurrentTenantId(tenantId);
        
        return true;  // 放行
    }
    
    @Override
    public void afterCompletion(...) {
        BaseContext.removeCurrentId();       // 必须清理！
        BaseContext.removeCurrentTenantId();
    }
    
    private String extractToken(HttpServletRequest request) {
        String header = request.getHeader("Authorization");
        if (header != null && header.startsWith("Bearer ")) {
            return header.substring(7);
        }
        return null;
    }
}

// 注册拦截器
@Configuration
public class WebMvcConfiguration implements WebMvcConfigurer {
    @Autowired
    private JwtTokenAdminInterceptor jwtTokenAdminInterceptor;
    
    @Override
    public void addInterceptors(InterceptorRegistry registry) {
        registry.addInterceptor(jwtTokenAdminInterceptor)
                .addPathPatterns("/api/**")
                .excludePathPatterns(
                    "/api/auth/login",
                    "/api/auth/register",
                    "/api/audit-tasks/callback"  // Rust 引擎回调不需要 JWT
                );
    }
}
```

### 3. JwtUtil — jjwt 0.9.1

项目用 `io.jsonwebtoken:jjwt:0.9.1`。注意：Java 11+ 需要额外引入 `javax.xml.bind:jaxb-api`（pom.xml 中已有）。

```java
public class JwtUtil {
    @Value("${jwt.secret}")
    private String secret;
    
    public String createToken(Long userId, Long tenantId) {
        return Jwts.builder()
            .setSubject(userId.toString())
            .claim("tenant_id", tenantId)
            .setIssuedAt(new Date())
            .setExpiration(new Date(System.currentTimeMillis() + 7 * 24 * 3600 * 1000L))
            .signWith(SignatureAlgorithm.HS256, secret)
            .compact();
    }
    
    public Long getUserIdFromToken(String token) {
        return Long.parseLong(
            Jwts.parser().setSigningKey(secret).parseClaimsJws(token)
                .getBody().getSubject()
        );
    }
    
    public boolean validateToken(String token) {
        try {
            Jwts.parser().setSigningKey(secret).parseClaimsJws(token);
            return true;
        } catch (JwtException | IllegalArgumentException e) {
            return false;
        }
    }
}
```

**为什么不把 token 存 Redis**：JWT 是无状态的——签名验证就可以判断是否有效，不需要查 Redis。退出登录时如果需要"立即失效"，把 token 加入 Redis 黑名单（TTL = token 剩余有效时间）。

---

### 4. SseHub — 项目真实的 SSE 推送

```java
@Component
public class SseHub {
    // taskId → SseEmitter（审核任务的 SSE 连接）
    private final ConcurrentHashMap<String, SseEmitter> emitters = new ConcurrentHashMap<>();
    
    public SseEmitter register(String taskId) {
        SseEmitter emitter = new SseEmitter(0L);  // 0 = 无超时
        emitters.put(taskId, emitter);
        
        emitter.onCompletion(() -> emitters.remove(taskId));
        emitter.onTimeout(() -> emitters.remove(taskId));
        emitter.onError(e -> emitters.remove(taskId));
        
        return emitter;
    }
    
    public void emit(String taskId, AuditTaskEvent event) {
        SseEmitter emitter = emitters.get(taskId);
        if (emitter != null) {
            try {
                emitter.send(
                    SseEmitter.event()
                        .id(event.getId())           // 事件 ID → 用于断线 replay
                        .name(event.getType())        // progress / finding / complete
                        .data(JSON.toJSONString(event))
                );
            } catch (IOException e) {
                emitters.remove(taskId);  // 发送失败→客户端断开→清理
            }
        }
    }
    
    public void remove(String taskId) {
        SseEmitter emitter = emitters.remove(taskId);
        if (emitter != null) emitter.complete();
    }
}
```

#### 断线重连 + Event Replay

客户端断开后重连→需要补推断开期间的事件：

```java
// 1. 客户端重连时带上 lastEventId
// 2. 从 Redis 或 DB 查询该 lastEventId 之后的事件
// 3. 先批量推送补推事件，再推送实时事件

@GetMapping("/api/audit-tasks/{taskId}/stream")
public SseEmitter stream(
    @PathVariable String taskId,
    @RequestHeader(value = "Last-Event-ID", required = false) String lastEventId
) {
    SseEmitter emitter = sseHub.register(taskId);
    
    // 如果有 lastEventId → 补推缺失事件
    if (lastEventId != null) {
        List<AuditTaskEvent> missedEvents = 
            auditTaskEventService.findEventsAfter(taskId, lastEventId);
        for (AuditTaskEvent event : missedEvents) {
            emitter.send(SseEmitter.event().id(event.getId()).name(event.getType()).data(...));
        }
    }
    
    return emitter;
}
```

---

## 动手

### 任务 1：JWT 完整链路

实现 JwtUtil（签发/验证/解析）→ JwtTokenAdminInterceptor→注册到 WebMvcConfigurer→测试：带 token 调受保护 API→不带 token 返回 401→token 过期返回 401→退出登录后 token 入黑名单→401。

### 任务 2：SseHub 实现

实现 SseHub + Controller。用 curl 连接 SSE 流→另一个终端创建审核任务→Worker 处理时通过 SseHub 推送进度事件→curl 终端实时收到。

### 任务 3：SSE 断线 replay

断开 curl 连接→重连时带 `Last-Event-ID` header→验证补推了断开期间的事件。

---

## 验收标准

- [ ] JWT 认证链路完整（登录→携带 token→受保护 API→过期→401）
- [ ] SSE 实时推送流畅（创建任务→进度事件实时到达前端）
- [ ] 断线 replay 正确（重连后补推缺失事件）

---

## 思考题

1. Spring Security 的 `SecurityContextHolder` 默认用 `ThreadLocal` 存认证信息——和你的 `BaseContext` 是同一个原理。为什么项目不用它？
2. SseHub 的 `ConcurrentHashMap` 在高并发下（1000 个审核任务同时推送）有性能瓶颈吗？瓶颈在哪？
3. JWT 的"无状态"是优势也是劣势——如果你需要"强制下线某个用户"，怎么做？（提示：Redis 黑名单 + 版本号）

---

## 与标书审核项目的关系

项目的 `JwtTokenAdminInterceptor` 在 `common/JwtTokenAdminInterceptor.java`。`SseHub` 在 `sse/SseHub.java`。SSE 事件持久化在 `AuditTaskEventService`（查询 `audit_task_event` 表）。你今天的实现就是这些文件的原理版。
