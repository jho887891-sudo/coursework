# Day 5：审核工作台独立机制 Demo

> 前 4 天你理解了 React 19 内核、Redux+react-query、react-pdf+echarts+SSE、缓存与性能。今天你从零搭建 feature-based 审核工作台——可拖拽双栏面板、实时 SSE 进度、echarts 统计、file-saver 导出。

---

## 目标

搭建 `features/bidAudit/` 完整审核工作台——文件上传→react-pdf 预览（bbox 高亮）→eventsource SSE 进度→echarts 统计→react-markdown 审核报告→file-saver 导出。TypeScript strict 零 any。

---

## 功能清单

### P0：完整审核工作台（及格线）

| 功能 | 技术 |
|------|------|
| 文件上传 | antd Upload + UploadDragger |
| PDF 预览 + 高亮 | react-pdf + 自定义 bbox overlay |
| 可拖拽面板分隔 | react-resizable 分隔器 |
| SSE 实时进度 | eventsource → progress + finding 消息 |
| Dashboard 统计图 | echarts 环形图 + 柱状图 |
| 审核报告渲染 | react-markdown + remark-gfm + rehype-raw |
| 报告导出 | file-saver 触发 Blob 下载 |
| 图标 | lucide-react |

### P1：Redux + react-query 缓存（加分项）

- authSlice 正确管理 token/user/isAuthenticated
- 审核任务列表 useQuery + createTask useMutation + invalidateQueries
- staleTime 分场景配置

### P2：响应式 + 拖拽（加分项）

- `< 768px`：双栏→单栏栈式布局
- react-resizable 可拖拽面板分隔器
- antd-style createStyles 类型安全的样式

---

## Demo 路由结构（参考项目 `app/router.tsx` 的组织思想）

```typescript
// createBrowserRouter 集中路由
const router = createBrowserRouter([
  {
    path: '/',
    element: <AppLayout />,
    children: [
      { index: true, element: <Navigate to="/dashboard" replace /> },
      {
        path: 'dashboard',
        lazy: () => import('@/features/dashboard/DashboardPage'),
      },
      {
        path: 'projects/:id/audit/:tid',
        lazy: () => import('@/features/bidAudit/AuditPage'),  // 审核工作台
      },
      {
        path: 'projects/:id/report/:rid',
        lazy: () => import('@/features/bidAudit/ReportPage'),  // 审核报告
      },
    ],
  },
]);
```

---

## 审核工作台页面结构

```
┌─────────────────────────────────────────────────────────┐
│ Header: ← 返回  │ 招标文件.pdf  │ [导出报告]              │
├──────────────────────┬──────────────────────────────────┤
│ react-pdf Document   │  echarts 统计环形图               │
│ Page + bbox overlay  │  ┌──────┐  ┌──────┐             │
│                      │  │crit  │  │warn  │              │
│ ┌────────────────┐   │  └──────┘  └──────┘             │
│ │                │   │                                  │
│ │  招标文件 PDF   │   │  IssuePanel:                     │
│ │                │   │  ┌──────────────────────────┐   │
│ │  ┌──高亮──┐    │   │  │ ⚠ 排斥性条款               │   │
│ │  │ bbox  │    │   │  │ 《政府采购法》第22条         │   │
│ │  └───────┘    │   │  │ 修改建议：删除地域限制       │   │
│ │                │   │  └──────────────────────────┘   │
│ └────────────────┘   │  ┌──────────────────────────┐   │
│                      │  │ ℹ 格式问题                │   │
│ ← 可拖拽分隔器 →     │  │ ...                       │   │
└──────────────────────┴──────────────────────────────────┘
```

---

## 导出模块（file-saver）

```typescript
import { saveAs } from 'file-saver';

async function exportReport(reportId: string, format: 'pdf' | 'docx') {
  const blob = await fetch(`/api/audit/reports/${reportId}/export?format=${format}`)
    .then(r => r.blob());
  saveAs(blob, `审核报告_${reportId}.${format}`);
}

// 或纯前端生成 PDF（jsPDF）
// 或纯前端生成 Word（html-docx-js-typescript 项目已引入）
```

---

## 大作业验收

| 验收项 | 权重 | 怎么测 |
|--------|------|--------|
| TypeScript strict 零 any | 10% | `tsc --noEmit` |
| react-pdf + bbox 高亮 | 15% | 上传 PDF→预览→高亮位置准确 |
| eventsource SSE | 10% | mock SSE→三种消息正确处理 |
| echarts 图表 | 10% | 环形图+柱状图可交互 |
| react-markdown 报告 | 10% | 表格+法规引用正确渲染 |
| file-saver 导出 | 10% | 下载的文件非空 |
| redux + react-query | 10% | auth slice + 数据缓存 |
| 可拖拽面板分隔 | 10% | react-resizable 可拖拽 |
| antd-style + lucide-react | 5% | 类型安全样式+统一图标 |
| 设计决策文档 | 10% | 状态管理/SSE重连/缓存策略/导出方案 |

---

## 设计决策文档（必写）

1. **为什么 auth 用 Redux，数据用 react-query，而不是全用 React Context？** — Context 推送导致无关组件渲染，Redux selector + react-query cache 各有精确的"什么时候重渲染"控制
2. **为什么用 react-pdf 而不是裸 pdfjs-dist？** — react-pdf 将 Canvas 生命周期绑定 React 生命周期，省去了手动管理渲染/销毁/重渲染的代码
3. **SSE 为什么用 eventsource package 而不是原生 EventSource？** — 原生 EventSource 不支持自定义 Header（如 Authorization），项目 JWT 认证模式下必须用 eventsource
4. **echarts 为什么直接用原生 API 而不是 echarts-for-react？** — 项目已经有 `ahooks` 的防抖能力和严格的 useEffect 规范，不需要额外的 React 封装层

---

## 与标书审核项目的关系

这个 Demo 用 Mock API 和模拟 SSE 完成“任务进度→问题列表→PDF 定位→报告展示”的最小闭环。它用于理解状态边界、缓存和坐标变换，不直接合并到 `frontend/src/features/bidAudit/`。后续项目开发需要按真实 API、设计规范和测试要求重新实现或迁移。
