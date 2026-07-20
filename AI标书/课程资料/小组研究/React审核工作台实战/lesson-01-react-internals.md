# Day 1：React 19 内核 — Fiber、Hooks 链表与闭包陷阱

> 项目用 React 19.2。今天你不学"怎么用 useState"——你理解 Fiber 协调器怎么调度渲染、Hooks 链表怎么存在 Fiber 节点上、闭包陷阱为什么发生、React 19 的 `use()` 和 `ref` as prop 带来了什么。

---

## 学习目标

1. 理解 Fiber reconciler 的节点结构和双缓冲机制
2. 从 Fiber.memoizedState 理解 Hooks 链表和闭包陷阱的根因
3. 掌握 React 19 新增 `use()` / ref as prop / `useOptimistic` / 异步清理
4. 了解 React 19 破坏性变化（`propTypes` 移除、`defaultProps` 废弃、`<Context>` 直接替代 `<Context.Provider>`）

---

## 核心概念

### 1. Fiber — React 的"虚拟 DOM"其实是一棵树链表

React 19 的 Fiber reconciler 与 React 18 在核心机制上一致——可中断的异步渲染、优先级调度、双缓冲。

```
Fiber 节点结构（关键字段）：

  child    → 第一个子节点
  sibling  → 下一个兄弟节点（树转链表）
  return   → 父节点
  alternate → current ↔ workInProgress 双缓冲指针

  memoizedState → Hooks 链表头
  memoizedProps → 上次渲染的 props
  stateNode     → DOM 节点引用

  lanes     → 31 位优先级位图
  childLanes → 子树中最高优先级
```

React 19 的变化：改进了并发渲染的调度稳定性、减少了未使用状态造成的额外渲染、`use()` 允许在渲染中直接读取异步数据而无需 Suspense 边界。

---

### 2. Hooks 链表与闭包陷阱

Hooks 在 Fiber.memoizedState 上是**单向链表**：

```
fiber.memoizedState → {
  memoizedState: 0,           // useState(0)
  queue: { dispatch: setState },
  next: {
    memoizedState: {          // useEffect(cb, deps)
      create: callback,
      destroy: cleanup,
      deps: [0]
    },
    next: null
  }
}
```

闭包陷阱的根因不变：effect 回调创建时捕获了当时的闭包变量。如果 deps 为空，回调永远用第一次的值。

```typescript
// ❌ 陷阱：count 永远是 0
useEffect(() => {
  const id = setInterval(() => setCount(count + 1), 1000);
  return () => clearInterval(id);
}, []);

// ✅ 解法 1：函数式更新
setCount(c => c + 1);

// ✅ 解法 2：正确 deps
useEffect(() => { ... }, [count]);
```

---

### 3. React 19 新特性（对标项目）

#### `use()` — 在渲染中读数据

```typescript
// React 19：直接在组件中用 use() 读取 Promise
function AuditTask({ taskId }: { taskId: string }) {
  const task = use(fetchTask(taskId));  // 不阻塞兄弟组件
  return <TaskDetail task={task} />;
}

// 替代了"useState + useEffect + if (loading)"的三段式
```

#### ref 作为 prop — 不再需要 forwardRef

```typescript
// React 18：需要 forwardRef 包装
const MyInput = forwardRef<HTMLInputElement, Props>((props, ref) => (
  <input ref={ref} {...props} />
));

// React 19：直接传 ref prop
function MyInput({ ref, ...props }: Props & { ref: Ref<HTMLInputElement> }) {
  return <input ref={ref} {...props} />;
}
```

本项目 antd 5.27 在 React 19 下已支持此用法。

#### useOptimistic — 乐观更新

```typescript
// 提交审核任务后立即显示"处理中"，不等服务器确认
const [optimisticTasks, addOptimistic] = useOptimistic(
  tasks,
  (state, newTask: AuditTask) => [...state, { ...newTask, status: 'PENDING' }]
);

function handleCreate(dto: CreateTaskDTO) {
  addOptimistic({ ...dto, id: tempId });
  // 实际请求在后台进行——失败时 useOptimistic 自动回滚
  createTaskMutation.mutate(dto);
}
```

#### 异步清理函数

```typescript
// React 19：useEffect 的 cleanup 可以返回 Promise
useEffect(() => {
  const es = new EventSource('/api/stream');
  es.onmessage = handleMessage;
  return async () => {
    es.close();
    await persistState();  // 关闭连接前保存状态
  };
}, []);
```

---

## 动手

### 任务 1：Fiber 树遍历

用 `__SECRET_INTERNALS_DO_NOT_USE_OR_YOU_WILL_BE_FIRED`（仅学习用）遍历 React 根节点的 Fiber 树，打印关键字段。对比 DevTools 验证。

### 任务 2：闭包陷阱实验室

复现 3 个经典陷阱——`setInterval` 读到旧值、`useEffect` deps 为空导致状态过期、`useCallback` 引用不稳定。每种提供修复方案。

### 任务 3：React 19 新特性对比

分别用 `use()` + `useOptimistic()` 重写一个"提交审核任务"的组件。对比与 React 18 写法的代码量差异。

---

## 验收标准

- [ ] Fiber 遍历器正确输出树结构
- [ ] 3 个闭包陷阱全部修复 + 根因分析
- [ ] React 19 新特性示例可运行

---

## 思考题

1. `use()` 和 `useEffect + useState` 的本质区别是什么？什么场景不适合用 `use()`？
2. React 19 的 `ref` as prop 消除了 `forwardRef`——但 Custom Component 的 `ref` 传递给哪个 DOM 节点是隐式的。怎么让使用者知道 ref 挂在了哪个元素上？
3. `useOptimistic` 在失败时自动回滚——但如果乐观更新时间窗口内用户做了其他操作（如删除了同一条任务），回滚逻辑怎么处理？

---

## 与标书审核项目的关系

审核工作台的 SSE 实时流（Day 3）可以在连接关闭时使用 React 19 的异步清理函数保存状态。乐观更新（`useOptimistic`）让创建审核任务时用户体验更流畅——点击"开始审核"后立即显示进度条，不等待 HTTP 响应。
