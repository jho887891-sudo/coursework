# Day 2：Redux Toolkit + TanStack Query + antd-style 组件架构

> Day 1 你理解了 React 19 怎么运行。今天你搭建项目的实际状态管理：Redux Toolkit 管 auth，TanStack Query 管数据，antd-style 管样式。三种范式各有领地，互不越界。

---

## 学习目标

1. 搭建 Redux Toolkit auth slice + typed hooks
2. 配置 TanStack Query 缓存策略（staleTime / gcTime / invalidateQueries）
3. 用 antd-style `createStyles` 编写类型安全的 CSS-in-JS
4. 理解 feature-based 目录结构并迁移一个 feature

---

## 核心概念

### 1. 为什么是 Redux + react-query 混合

```
┌─────────────────────────────────────────────┐
│              Application State              │
├──────────────┬──────────────┬───────────────┤
│  Client      │  Server      │  UI / Form    │
│  (Redux)     │  (react-qry) │  (useState)   │
├──────────────┼──────────────┼───────────────┤
│ auth/user    │ 项目列表     │ 输入框        │
│ token        │ 审核任务     │ 筛选条件      │
│ isAuth       │ 审核报告     │ 面板展开状态  │
│              │ 标书库       │ echarts 配置  │
│              │ 审核历史     │               │
└──────────────┴──────────────┴───────────────┘
```

**红线**：不要把服务端数据放进 Redux。react-query 自动管理缓存/失效/重新获取。Redux 只放跨组件且不依赖服务端的全局状态。

### 2. Redux Toolkit — 项目真实的 authSlice

```typescript
// store/slices/authSlice.ts（对标项目实际代码）
import { createSlice, createAsyncThunk } from '@reduxjs/toolkit';

interface AuthState {
  user: User | null;
  token: string | null;
  isAuthenticated: boolean;
}

const initialState: AuthState = {
  user: null,
  token: null,
  isAuthenticated: false,
};

export const login = createAsyncThunk(
  'auth/login',
  async (credentials: LoginDTO, { rejectWithValue }) => {
    try {
      const response = await authApi.login(credentials);
      return response.data;  // { user, token }
    } catch (err) {
      return rejectWithValue('登录失败');
    }
  }
);

const authSlice = createSlice({
  name: 'auth',
  initialState,
  reducers: {
    logout: (state) => {
      state.user = null;
      state.token = null;
      state.isAuthenticated = false;
    },
    setToken: (state, action: PayloadAction<string>) => {
      state.token = action.payload;
    },
  },
  extraReducers: (builder) => {
    builder
      .addCase(login.fulfilled, (state, action) => {
        state.user = action.payload.user;
        state.token = action.payload.token;
        state.isAuthenticated = true;
      });
  },
});
```

#### Typed Hooks — 整个项目的入口

```typescript
// store/hooks.ts
import { TypedUseSelectorHook, useDispatch, useSelector } from 'react-redux';
import type { RootState, AppDispatch } from './index';

export const useAppDispatch = () => useDispatch<AppDispatch>();
export const useAppSelector: TypedUseSelectorHook<RootState> = useSelector;

// 之后所有组件都用 useAppSelector 替代 useSelector
// → TS 自动推导 state.auth.user?.name 的类型
```

---

### 3. TanStack React Query — 服务端缓存

```typescript
// lib/queryClient.ts
import { QueryClient } from '@tanstack/react-query';

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 5 * 60 * 1000,      // 5 分钟内不重新请求
      gcTime: 30 * 60 * 1000,         // 旧 cacheTime，30 分钟后回收
      retry: 2,
      refetchOnWindowFocus: false,    // 审核工作台不自动重取
    },
    mutations: {
      retry: 1,
    },
  },
});
```

#### 审核任务的查询和变更

```typescript
// features/bidAudit/api/audit.ts
export function useAuditTasks(projectId: string) {
  return useQuery({
    queryKey: ['audit-tasks', projectId],
    queryFn: () => auditApi.listTasks(projectId),
    staleTime: 10 * 1000,  // 审核状态变化快，10 秒过期
  });
}

export function useCreateAuditTask() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (dto: CreateTaskDTO) => auditApi.createTask(dto),
    onSuccess: (_, variables) => {
      // 创建成功后立即刷新项目下的任务列表
      queryClient.invalidateQueries({
        queryKey: ['audit-tasks', variables.projectId],
      });
    },
  });
}
```

#### optimistic update 模式

```typescript
// React 19 useOptimistic + react-query onMutate
export function useOptimisticTaskCreate() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: auditApi.createTask,
    onMutate: async (newTask) => {
      // 乐观：立即取消正在进行的查询，避免覆盖
      await queryClient.cancelQueries({ queryKey: ['audit-tasks'] });
      // 保存旧数据用于回滚
      const previous = queryClient.getQueryData(['audit-tasks']);
      // 乐观更新
      queryClient.setQueryData(['audit-tasks'], (old) =>
        [...(old || []), { ...newTask, id: 'temp', status: 'PENDING' }]
      );
      return { previous };
    },
    onError: (_, __, context) => {
      // 回滚
      queryClient.setQueryData(['audit-tasks'], context?.previous);
    },
    onSettled: () => {
      // 无论成功失败，最终重新获取
      queryClient.invalidateQueries({ queryKey: ['audit-tasks'] });
    },
  });
}
```

---

### 4. antd-style — 项目真实的 CSS-in-JS

```typescript
// 用 createStyles 定义带类型的样式
import { createStyles } from 'antd-style';

const useStyles = createStyles(({ token, css }) => ({
  card: css`
    border: 1px solid ${token.colorBorderSecondary};
    border-radius: ${token.borderRadiusLG}px;
    padding: ${token.padding}px;
    &:hover {
      border-color: ${token.colorPrimary};
      box-shadow: 0 2px 8px ${token.colorPrimaryBg};
    }
  `,
  critical: css`
    background: ${token.colorErrorBg};
    border-left: 4px solid ${token.colorError};
  `,
}));

function IssueCard({ severity }: { severity: string }) {
  const { styles, cx } = useStyles();
  return (
    <div className={cx(styles.card, severity === 'critical' && styles.critical)}>
      ...
    </div>
  );
}
```

`createStyles` 的优势：类型安全的 token 访问（`token.colorPrimary` 是 TypeScript 类型，不会拼错）、动态主题（切换暗色模式→所有引用自动更新）、零运行时 CSS 注入（生产环境提取为静态 CSS）。

---

### 5. Feature-based 目录结构

```
features/bidAudit/               ← 一个 feature = 一个自包含模块
├── api/
│   └── audit.ts                 ← react-query hooks
├── components/
│   ├── DocumentViewer.tsx       ← PDF 查看器
│   ├── IssuePanel.tsx           ← 问题列表
│   ├── IssueCard.tsx            ← 单条问题卡片
│   └── ProgressBar.tsx          ← 审核进度
├── hooks/
│   ├── useAuditSSE.ts           ← eventsource SSE
│   └── useScrollSync.ts         ← 双栏联动
└── routes.tsx                   ← 懒加载路由: lazy(() => import('./AuditPage'))
```

每个 feature 自包含——删除文件夹即可移除整个功能模块。

---

## 动手

### 任务 1：Redux Toolkit auth slice

实现完整的 auth slice（login/logout + typed hooks）。验证：用 Redux DevTools 查看 action 流。

### 任务 2：react-query 缓存实验

创建两个页面（Dashboard + 审核工作台）。Dashboard 用 `staleTime=5min` 读项目列表。审核工作台创建新任务后→`invalidateQueries`→Dashboard 数据自动刷新。

### 任务 3：antd-style 重写 IssueCard

用 `createStyles` 重写 IssueCard 的样式（critical=红色左边框 / warning=金色 / info=蓝色）。对比 CSS Modules 方式的代码量。

---

## 验收标准

- [ ] Redux DevTools 中能看到 login/logout action
- [ ] react-query 缓存：创建任务→Dashboard 自动刷新
- [ ] IssueCard 三种 severity 的样式正确，token 引用无拼写错误

---

## 思考题

1. 为什么项目用 Redux 管 auth 而不是用 react-query 管 auth？（提示：auth 是"客户端状态"不是"服务端状态"）
2. react-query 的 `staleTime` 设太大可能读到过期数据，设太小又频繁请求。审核任务列表（状态快速变化）应该设多大？
3. feature-based 目录的边界在哪——`IssueCard` 如果被 `Dashboard` feature 复用了，放在哪？

---

## 与标书审核项目的关系

你今天在独立 Demo 中编写 `authSlice.ts` 和 `queryClient.ts`，再只读对照项目中的同名职责。Day 3-5 的实验代码继续放在 Demo 的 `features/bidAudit/` 下，不写入现有项目目录。
