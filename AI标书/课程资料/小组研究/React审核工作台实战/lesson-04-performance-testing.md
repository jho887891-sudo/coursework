# Day 4：react-query 缓存策略 + ahooks + Bundle 分析

> 审核工作台功能对了——但 Dashboard 打开 3 秒、滚动时 CPU 100%、窗口缩小后 echarts 挤成一团。今天你学会 react-query 的缓存分层、ahooks 的防抖节流、Bundle 分析和 Lighthouse 审计。

---

## 学习目标

1. 配置 react-query 的 staleTime / gcTime / invalidateQueries 形成分场景缓存策略
2. 用 ahooks 的 useRequest / useDebounce / useThrottleFn 简化异步和防抖
3. 用 Vite 的 rollup-plugin-visualizer 分析 Bundle
4. 用 React DevTools Profiler + Lighthouse 定位性能瓶颈

---

## 核心概念

### 1. react-query 缓存策略分层

不同的查询有不同的新鲜度需求——不能一刀切用同一个 staleTime：

```typescript
// lib/queryClient.ts
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 5 * 60 * 1000,   // 默认 5 分钟
      gcTime: 30 * 60 * 1000,
      retry: 2,
    },
  },
});

// 按查询场景精细化配置：

// 项目列表——不常变，5 分钟内缓存直接返回
useQuery({ queryKey: ['projects'], queryFn: ..., staleTime: 5 * 60 * 1000 });

// 审核任务状态——变化快，10 秒过期
useQuery({ queryKey: ['audit-task', id], queryFn: ..., staleTime: 10 * 1000 });

// 审核报告——完成后不再变，gcTime 设为 1 小时
useQuery({ queryKey: ['audit-report', id], queryFn: ..., gcTime: 60 * 60 * 1000 });

// 用户信息——几乎不变，staleTime 设为 30 分钟
useQuery({ queryKey: ['user'], queryFn: ..., staleTime: 30 * 60 * 1000 });
```

#### 预取（prefetch）减少等待

```typescript
// Dashboard 页面 hover 项目卡片时，预取审核任务列表
function ProjectCard({ projectId }: { projectId: string }) {
  const queryClient = useQueryClient();

  const handleMouseEnter = () => {
    queryClient.prefetchQuery({
      queryKey: ['audit-tasks', projectId],
      queryFn: () => auditApi.listTasks(projectId),
      staleTime: 10 * 1000,
    });
  };

  return <Card onMouseEnter={handleMouseEnter}>...</Card>;
}
```

鼠标悬停时预取→点击跳转时数据大概率已在缓存中→瞬间渲染。

---

### 2. ahooks — 项目中已引入的 Hooks 库

项目 `package.json` 中 `"ahooks": "^3.9.6"`。四个最常用的：

```typescript
// ① useRequest：自动管理 loading/error/data + 缓存 + 防抖
const { data, loading, error, run } = useRequest(
  (keyword: string) => auditApi.searchLaws(keyword),
  {
    debounceWait: 300,   // 输入停止 300ms 后才请求
    retryCount: 2,
    cacheKey: 'law-search',  // 跨组件缓存
  }
);

// ② useDebounce：函数防抖——echarts resize
const { run: debouncedResize } = useDebounceFn(
  () => chartRef.current?.resize(),
  { wait: 200 }
);

// ③ useThrottleFn：函数节流——滚动事件
const { run: throttledScroll } = useThrottleFn(
  (scrollTop: number) => updateActiveBBox(scrollTop),
  { wait: 100 }
);

// ④ useInfiniteScroll：审核历史无限滚动
const { data, loading, loadMore, noMore } = useInfiniteScroll(
  (d) => auditApi.listHistory({ cursor: d?.nextCursor }),
  { target: scrollContainerRef, isNoMore: (d) => !d?.hasMore }
);
```

---

### 3. Bundle 分析与代码分割

```bash
# 安装 rollup-plugin-visualizer
pnpm add -D rollup-plugin-visualizer

# vite.config.ts
import { visualizer } from 'rollup-plugin-visualizer';
export default defineConfig({
  plugins: [
    react(),
    visualizer({ open: true, gzipSize: true }),  // 构建后打开分析页面
  ],
});
```

分析重点关注：
- **echarts**（~1MB gzip ~300KB）→如果只在 Dashboard 用→`React.lazy` 动态导入
- **react-pdf**（~500KB gzip ~150KB）→只在审核工作台用→`React.lazy`
- **antd icons**（全量导入 ~200KB）→改用 `@ant-design/icons` 的 tree-shaking 版本

```typescript
// 路由级代码分割——已在项目中
const AuditWorkbench = lazy(() => import('@/features/bidAudit/AuditPage'));
const Dashboard = lazy(() => import('@/features/dashboard/DashboardPage'));

// 组件级代码分割——echarts 只在 Dashboard 用
const AuditStatsChart = lazy(() => import('./AuditStatsChart'));
```

#### Lighthouse Performance 审查

```
Chrome DevTools → Lighthouse → Performance 审计

目标：
  FCP (First Contentful Paint)  < 1.8s
  LCP (Largest Contentful Paint) < 2.5s
  TBT (Total Blocking Time)      < 200ms
  Performance Score              > 90

常见瓶颈：
  - 未压缩的 antd JS → gzip 由 Nginx/CDN 处理
  - PDF worker 阻塞主线程 → react-pdf 的 worker 在 Web Worker 中运行（默认）
  - echarts 初始化在主线程 → lazy + Suspense
```

---

## 动手

### 任务 1：react-query 缓存实验

监控 DevTools Network 面板。切换页面再切回来→观察哪些请求被跳过（缓存命中）。调整 staleTime→验证"过期=重新请求"。

### 任务 2：ahooks 集成

用 `useRequest` 替换三个手动 `useState + useEffect` 的 API 调用。用 `useDebounceFn` 优化 echarts resize。用 `useInfiniteScroll` 实现审核历史列表的无限滚动。

### 任务 3：Bundle 分析 + Lighthouse

运行 `pnpm build --mode analyze`→截图 Bundle 分析图→标注最大的 3 个包。对 echarts 做 `React.lazy` 拆分→对比前后的 LCP 变化。

---

## 验收标准

- [ ] react-query 缓存命中（DevTools 中无重复请求）
- [ ] ahooks 替代手动 loading/error 管理
- [ ] Bundle 分析报告 + Lighthouse > 85

---

## 思考题

1. react-query 的 `staleTime` 和 `gcTime` 的关系——"stale but cached"是什么意思？
2. `useRequest` 的 `cacheKey` 跨组件共享——这和 react-query 的 `queryKey` 有什么本质区别？
3. 代码分割后，用户点击"审核工作台"菜单→需要下载 react-pdf chunk（~150KB）。在慢网（3G）下这要 3 秒——你怎么优化？（提示：prefetch + Suspense fallback）

---

## 与标书审核项目的关系

项目的 `lib/queryClient.ts` 就是今天配置的 queryClient。ahooks 已在 `package.json` 中引入——审核工作台的滚动联动和防抖直接用它。Bundle 分析是上线前的性能审计——G9 组交付物的 Lighthouse 报告对标今天的实验数据。
