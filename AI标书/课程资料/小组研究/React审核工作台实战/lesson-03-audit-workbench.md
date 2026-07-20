# Day 3：双栏审核工作台 — react-pdf + echarts + eventsource + react-markdown

> 今天你搭建审核工作台的核心——react-pdf 渲染 PDF + bbox 高亮、echarts 多维统计图、eventsource 实时 SSE、react-markdown 审核报告。四个工具，一个页面，零重复代码。

---

## 学习目标

1. 用 react-pdf 渲染 PDF + 自定义 bbox 高亮覆盖层
2. 用 echarts 绘制 Dashboard 统计图表（环形图 + 柱状图）
3. 用 eventsource 集成 SSE 实时审核进度（含断线重连）
4. 用 react-markdown + remark-gfm + rehype-raw 渲染审核报告

---

## 核心概念

### 1. react-pdf — React 组件化的 PDF 渲染

#### 与裸 pdfjs-dist 的区别

```typescript
// ❌ 裸 pdfjs-dist：自己管理 Canvas 生命周期
const canvas = useRef<HTMLCanvasElement>(null);
useEffect(() => {
  const page = await pdf.getPage(1);
  page.render({ canvasContext: canvas.current.getContext('2d')!, viewport });
}, []);

// ✅ react-pdf：声明式组件，React 管理渲染和卸载
import { Document, Page, pdfjs } from 'react-pdf';
pdfjs.GlobalWorkerOptions.workerSrc = '/pdf.worker.min.mjs';

<Document file="/bid.pdf" onLoadSuccess={onDocLoad}>
  <Page pageNumber={currentPage} width={containerWidth} />
</Document>
```

react-pdf 是 pdfjs-dist 的 React 封装——它将 Canvas 的渲染/重渲染/销毁生命周期与 React 的组件生命周期绑定。在项目 `package.json` 中 `"react-pdf": "^10.4.1"`。

#### bbox 高亮覆盖层

react-pdf 的 `Page` 组件渲染 Canvas——但它不提供"高亮某个段落"的功能。你需要自己加一个覆盖层：

```
┌──────────────────────────────┐
│ Page Container (relative)    │
│ ┌──────────────────────────┐ │
│ │ <Page> (Canvas)           │ │  ← react-pdf 渲染
│ └──────────────────────────┘ │
│ ┌──────────────────────────┐ │
│ │ <div> (Overlay 绝对定位)   │ │  ← 你的高亮层
│ │  ┌──────────┐             │ │
│ │  │ 高亮矩形  │  ← bbox    │ │
│ │  └──────────┘             │ │
│ └──────────────────────────┘ │
└──────────────────────────────┘
```

```typescript
function HighlightLayer({ bboxes, pageNumber, scale }: Props) {
  return (
    <div style={{ position: 'absolute', top: 0, left: 0, pointerEvents: 'none' }}>
      {bboxes
        .filter(b => b.page === pageNumber)
        .map(b => (
          <div
            key={b.id}
            style={{
              position: 'absolute',
              left: b.x * scale,
              top: b.y * scale,
              width: b.w * scale,
              height: b.h * scale,
              background: 'rgba(255, 77, 79, 0.15)',
              border: '1px solid #ff4d4f',
            }}
          />
        ))}
    </div>
  );
}
```

---

### 2. echarts — 数据可视化

项目用 echarts 6（`"echarts": "^6.0.0"`），不是 echarts-for-react 封装——直接使用原生 API：

```typescript
import * as echarts from 'echarts';
import { useEffect, useRef } from 'react';
import { useDebounceFn } from 'ahooks';

function AuditStatsChart({ data }: { data: AuditStats }) {
  const chartRef = useRef<HTMLDivElement>(null);
  const chartInstance = useRef<echarts.ECharts>();

  useEffect(() => {
    chartInstance.current = echarts.init(chartRef.current!);
    return () => chartInstance.current?.dispose();  // 清理！
  }, []);

  useEffect(() => {
    chartInstance.current?.setOption({
      tooltip: { trigger: 'item' },
      series: [{
        type: 'pie',
        radius: ['40%', '70%'],  // 环形图
        data: [
          { value: data.critical, name: '严重', itemStyle: { color: '#cf1322' } },
          { value: data.warning, name: '警告', itemStyle: { color: '#d48806' } },
          { value: data.info, name: '信息', itemStyle: { color: '#1677ff' } },
        ],
      }],
    });
  }, [data]);

  // resize 防抖（ahooks useDebounceFn）
  const { run: handleResize } = useDebounceFn(
    () => chartInstance.current?.resize(),
    { wait: 200 }
  );

  useEffect(() => {
    window.addEventListener('resize', handleResize);
    return () => window.removeEventListener('resize', handleResize);
  }, []);

  return <div ref={chartRef} style={{ width: '100%', height: 300 }} />;
}
```

关键点：
- `echarts.init(dom)` 返回的实例必须在组件卸载时 `dispose()`
- `setOption` 接受新的 data 时自动 diff——不会全量重建 Canvas
- resize 必须用防抖——窗口缩放事件每秒可能触发几十次

---

### 3. eventsource — SSE 实时流

项目用 `eventsource` package（`"eventsource": "^4.1.0"`）——比浏览器原生 EventSource 多了自定义 Header、自动重连配置、Node.js 同构。

```typescript
import { EventSource } from 'eventsource';

function useAuditSSE(taskId: string | null) {
  const [messages, setMessages] = useState<SSEMessage[]>([]);
  const [state, setState] = useState<'connecting' | 'open' | 'closed'>('closed');
  const esRef = useRef<EventSource>();

  useEffect(() => {
    if (!taskId) return;

    setState('connecting');
    const es = new EventSource(`/api/audit/tasks/${taskId}/stream`);
    esRef.current = es;

    es.onopen = () => setState('open');

    es.addEventListener('progress', (e: MessageEvent) => {
      const data = JSON.parse(e.data);  // { progress: 65, currentStage: '资质审查' }
      setMessages(prev => [...prev, { type: 'progress', ...data }]);
    });

    es.addEventListener('finding', (e: MessageEvent) => {
      const finding = JSON.parse(e.data);  // 完整的 AuditFinding
      setMessages(prev => [...prev, { type: 'finding', finding }]);
    });

    es.addEventListener('complete', () => {
      setState('closed');
      es.close();
    });

    es.onerror = () => {
      setState('closed');
      es.close();
      // eventsource package 自带指数退避重连
      // 3 次内自动重新连接
    };

    return () => {
      es.close();
      setState('closed');
    };
  }, [taskId]);

  return { messages, state };
}
```

SSE vs WebSocket vs 轮询：标书审核需要从服务器到客户端的单向推送（进度+发现+完成），SSE 是最简单的方案。WebSocket 是全双工——本场景不需要客户端推送消息到服务器。

---

### 4. react-markdown + remark-gfm + rehype-raw

审核报告用 Markdown 存储（Agent 输出 Markdown 格式→前端渲染）。项目用了三个包：

```typescript
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';       // 表格、任务列表、删除线
import rehypeRaw from 'rehype-raw';       // 允许嵌入原始 HTML（如 <span class="highlight">）

function AuditReport({ markdown }: { markdown: string }) {
  return (
    <ReactMarkdown
      remarkPlugins={[remarkGfm]}
      rehypePlugins={[rehypeRaw]}
      components={{
        // 自定义渲染：给表格加 antd Table 样式
        table: ({ children }) => (
          <table style={{ borderCollapse: 'collapse', width: '100%' }}>{children}</table>
        ),
        // 法规引用自动高亮
        code: ({ className, children }) => {
          const text = String(children);
          if (text.startsWith('《') && text.endsWith('》')) {
            return <span className="law-ref">{text}</span>;
          }
          return <code className={className}>{children}</code>;
        },
      }}
    >
      {markdown}
    </ReactMarkdown>
  );
}
```

---

## 动手

### 任务 1：react-pdf + bbox 高亮

渲染 3 页招标文件 PDF。在指定 bbox 位置覆盖高亮矩形（讲师提供 bbox 坐标 JSON）。点击高亮矩形→打印 clause_id。

### 任务 2：echarts Dashboard

用 echarts 绘制环形图（critical/warning/info 比例）+ 柱状图（按 Agent 维度的发现数量）。resize 防抖验证。

### 任务 3：eventsource SSE + react-markdown

用 eventsource 连接 mock SSE→接收进度+新 finding→追加到列表。审核完成后切换到 react-markdown 报告渲染（含表格+法规引用）。

---

## 验收标准

- [ ] react-pdf 渲染 + bbox 高亮位置准确
- [ ] echarts 环形图 + 柱状图可交互（hover tooltip）
- [ ] eventsource 自动重连 + 三种消息类型分发正确
- [ ] react-markdown 正确渲染表格和法规引用高亮

---

## 思考题

1. react-pdf 的 `Page` 组件 `onRenderSuccess` 回调在 Canvas 绘制完成后触发——这是不是获取 bbox 实际像素位置的最佳时机？
2. echarts `setOption` 的 `notMerge: true` 与 `notMerge: false` 有什么区别？在数据完全变化和部分变化时分别应该用什么？
3. SSE 和 WebSocket 的选择——什么场景下 SSE 不够用？

---

## 与标书审核项目的关系

独立 Demo 中的 DocumentViewer、IssuePanel、ProgressBar 用于理解项目同类组件的职责。课程只读对照项目结构；Demo 使用 Mock SSE 和 Mock 报告，不直接修改项目审核页面。
