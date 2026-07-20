# React 企业级审核工作台实战

> 5 天深度原理课。从 React 19 的 Fiber 协调机制到审核工作台的数据流，通过独立前端 Demo 研究状态管理、缓存、SSE、PDF 坐标和渲染性能。

本课遵循[专项课程统一学习规范](../../专项课程统一学习规范.md)。实验新建独立 Vite 工程或使用课程专属 Demo，不在课程中修改现有 `frontend/` 业务代码。

---

## 你需要准备什么

| 要求 | 说明 |
|------|------|
| React 基础 | 会用 useState / useEffect / 组件传参 |
| TypeScript 基础 | 会写基本类型标注 |
| CSS 基础 | Flexbox / Grid |
| Node.js | ≥ 22 |
| pnpm | `npm i -g pnpm` |

**不需要**：Redux 经验、react-query 经验、Ant Design 经验。

### 验证环境

```bash
node --version   # ≥ 22
pnpm --version

# 项目已有前端代码
cd frontend
pnpm install
pnpm dev          # http://localhost:5173
```

---

## 真实技术栈（对标项目）

| 层 | 实际使用 |
|----|---------|
| 框架 | **React 19** + TypeScript 5.9 |
| 构建 | **Vite 7** |
| UI | **Ant Design 5.27** + **antd-style** (CSS-in-JS) |
| 状态管理 | **Redux Toolkit** (auth) + **TanStack React Query** (服务端) |
| 路由 | **react-router-dom v7** (`createBrowserRouter`) |
| 图表 | **echarts 6** |
| PDF | **react-pdf** + pdfjs-dist |
| 其他 | **react-markdown**, **eventsource**, **file-saver**, **ahooks**, **lucide-react** |

---

## 你会学到什么

| 天次 | 主题 | 你会理解或验证什么 |
|------|------|------------|
| Day 1 | **React 19 内核深讲** | Fiber 协调 + Hooks 链表 + 闭包陷阱 + React 19 新特性 |
| Day 2 | **状态管理 + 组件架构** | Redux Toolkit auth slice + TanStack Query 缓存 + antd-style 主题 + feature-based 目录 |
| Day 3 | **双栏审核工作台** | react-pdf 渲染 + bbox 高亮 + echarts 图表 + eventsource SSE + react-markdown 报告 |
| Day 4 | **性能优化 + 测试** | react-query 缓存策略 + ahooks + Bundle 分析 + Vitest 测试 |
| Day 5 | **🎓 工作台机制 Demo** | Mock 上传→进度→问题定位→报告的最小闭环 |

---

## 怎么学

```
Day 1  Fiber 协调 → Hooks 链表 → 闭包陷阱 → React 19 新特性
Day 2  Redux Toolkit auth → react-query → antd-style token → feature-based
Day 3  react-pdf → echarts → eventsource SSE → react-markdown
Day 4  react-query cache → ahooks → Bundle → Vitest
Day 5  file-saver 导出 + lucide-react + react-draggable + 大作业
```

---

## 独立 Demo 目录结构（参考项目的 feature-based 思想）

```
src/
├── app/
│   ├── router.tsx          # createBrowserRouter 集中路由
│   └── RouteGuard.tsx       # 认证守卫
├── store/
│   └── slices/
│       └── authSlice.ts     # Redux Toolkit (auth only)
├── lib/
│   └── queryClient.ts       # TanStack Query 配置
├── features/
│   ├── dashboard/           # 工作台
│   ├── bidUpload/           # 文件上传
│   ├── bidAudit/            # 审核工作台（核心）
│   │   ├── components/      # DocumentViewer, IssuePanel, ProgressBar
│   │   ├── hooks/           # useSSE, useScrollSync
│   │   ├── api/             # audit API 调用
│   │   └── routes.tsx
│   ├── bidLibrary/          # 标书库
│   └── history/             # 审核历史
├── components/              # 通用组件
│   └── layout/              # Header, Sidebar
└── types/                   # 全局类型
```

---

## 与标书审核项目的关系

```
本课程 → G9 前端体验组
  ├─ Day 2 Redux+react-query → 项目的状态管理方案
  ├─ Day 3 审核工作台 → 项目 features/bidAudit/ 核心页面
  ├─ Day 4 性能优化 → 上线前性能审计
  └─ Day 5 实验 → 验证数据流和交互机制，为后续项目开发提供设计依据
```

---

## 参考资源

- [React 19 发布博客](https://react.dev/blog/2024/12/05/react-19)
- [Redux Toolkit 文档](https://redux-toolkit.js.org/)
- [TanStack Query v5](https://tanstack.com/query/latest)
- [antd-style](https://ant-design.github.io/antd-style/)
- [react-pdf 文档](https://github.com/wojtekmaj/react-pdf)
- [echarts 文档](https://echarts.apache.org/)
