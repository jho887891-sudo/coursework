# Day 4：质量评估 + bbox 坐标链路

> "你的表格识别率 92%——怎么算出来的？"今天你亲手标注 20 份招标文件的表格、章节、bbox 点。然后写自动化 Benchmark——每次升级 Docling 版本、改分块策略、调融合参数，跑一遍就知道变好了还是变坏了。

---

## 学习目标

1. 实现确定性 Block ID 生成——同一份 PDF 总是产生相同的 ID
2. 理解 bbox 三层坐标变换——PDF→Canvas→DOM→视口
3. 搭建表格识别率 + 章节分割准确率 + bbox 偏移误差的自动化 Benchmark
4. 标注 20 份招标文件——构建 G1 组的 Golden Standard

---

## 核心概念

### 1. 确定性 Block ID

```rust
fn generate_block_id(
    document_id: &str,
    page: usize,
    paragraph_index: usize,
    text_prefix: &str,       // 前 100 字符
) -> String {
    let input = format!("{}:{}:{}:{}", document_id, page, paragraph_index, text_prefix);
    let hash = sha256::digest(input.as_bytes());
    format!("b_{}", &hash[..8])  // "b_a3f2b1c8"
}

fn generate_clause_id(
    document_id: &str,
    page: usize,
    clause_number: &str,     // 归一化后的条款编号，如 "22"
) -> String {
    let input = format!("{}:{}:clause:{}", document_id, page, clause_number);
    let hash = sha256::digest(input.as_bytes());
    format!("cl_{}", &hash[..8])
}
```

**为什么 text_prefix 参与 hash**：同一页面同一位置的段落，如果内容变了（如 PDF 版本不同），应该产生不同的 Block ID。text_prefix 保证：轻微排版变化（空格/换行）不改变 ID，但实质内容变化会改变 ID。

### 2. bbox 坐标变换链

G1 输出的 bbox 是 PDF 坐标（左下角原点，point 单位）。G9 前端需要的是 CSS pixel（左上角原点，含滚动偏移）。三层变换：

```
Layer 1: PDF → Canvas Viewport
  page.getViewport({ scale: 1.5 })
  vpX = pdfX * (viewport.width / page.width)   // PDF point → viewport pixel
  vpY = viewport.height - pdfY * (viewport.height / page.height)  // 左下→左上

Layer 2: Canvas Viewport → CSS DOM
  canvas.style.width = viewport.width / devicePixelRatio
  cssX = vpX / devicePixelRatio
  cssY = vpY / devicePixelRatio

Layer 3: CSS DOM → 视口内可见像素
  scrollTop = container.scrollTop
  visibleY = cssY - scrollTop + container.getBoundingClientRect().top
  // 如果 visibleY < 0 或 > container.clientHeight → 不在视口中
```

**G1 只输出 PDF 坐标 + scale_factor + page_dimensions**。G9 前端自己算后两层变换——因为 `scrollTop` 是前端运行时才知道的。

```rust
/// G1 输出的 bbox 附带信息
pub struct BBoxWithContext {
    pub bbox: BBox,                        // PDF 坐标
    pub page_width_pt: f64,               // PDF 页面宽度（point）
    pub page_height_pt: f64,              // PDF 页面高度（point）
    pub recommended_scale: f64,           // 建议的 viewport scale（如 1.5）
}
```

### 3. 质量 Benchmark

#### 表格识别率

```rust
struct TableEval {
    page: usize,
    expected: TableGroundTruth,   // 人工标注
    actual: Table,                // 解析结果
}

struct TableGroundTruth {
    row_count: usize,
    col_count: usize,
    header: Vec<String>,
}

fn table_accuracy(evals: &[TableEval]) -> (f64, f64, f64) {
    let row_correct = evals.iter().filter(|e| e.expected.row_count == e.actual.rows.len()).count();
    let col_correct = evals.iter().filter(|e| e.expected.col_count == e.actual.rows.first().map_or(0, |r| r.len())).count();
    let header_correct = evals.iter().filter(|e| e.expected.header == e.actual.header).count();
    let n = evals.len() as f64;
    (row_correct as f64 / n, col_correct as f64 / n, header_correct as f64 / n)
}
```

#### bbox 偏移 RMSE

```rust
fn bbox_rmse(points: &[(BBox, BBox)]) -> f64 {
    let n = points.len() as f64;
    let sum_sq = points.iter()
        .map(|(expected, actual)| {
            let dx = expected.x - actual.x;
            let dy = expected.y - actual.y;
            dx * dx + dy * dy
        })
        .sum::<f64>();
    (sum_sq / n).sqrt()
}
```

---

## 动手

### 任务 1：Golden Standard 标注

手动标注 20 页招标文件 PDF——在每页上标记：
- 每个表格的行数、列数、表头文字
- 每个章节的起始页码和标题
- 10 个随机 bbox 点的精确坐标（用 PDF 阅读器的坐标尺）

### 任务 2：自动化 Benchmark

用 Day 3 的解析 Pipeline 跑这 20 页→输出 `ParsedDocument`→和 Golden Standard 对比→生成质量报告：表格识别率(行/列/表头)、章节分割准确率、bbox RMSE。

### 任务 3：坐标变换验证

取一个已知 bbox 的条款（如"第 12 条"）→从 PDF 坐标开始，手动计算三层变换→对比前端高亮的实际像素位置→验证变换链正确。

---

## 验收标准

- [ ] 20 页 Golden Standard 标注完成
- [ ] 表格识别率 > 80%（行数+列数）作为基线
- [ ] bbox RMSE < 10pt
- [ ] 坐标变换验证：手动计算与实际高亮位置偏差 < 5px

---

## 思考题

1. Block ID 用 text_prefix 参与 hash——如果 PDF 版本差异导致"第22条"的条文内容完全不变但前后多了空格，text_prefix 会变吗？Block ID 会变吗？
2. bbox RMSE < 10pt——10pt ≈ 3.5mm。这是"可接受"的误差吗？如果 G9 前端在这个 bbox 上画高亮，用户能看出偏移吗？
3. 质量 Benchmark 需要人工标注——但 G1 组只有 3 个人，不可能标注 1000 页。你怎么用 LLM 辅助标注来扩大样本量？

---

## 与标书审核项目的关系

G1 组的交付物有两件：① `ParsedDocument` JSON（给下游消费）和 ② 质量 Benchmark 报告（证明解析质量达标）。你今天标的数据 + 写的自动化 Benchmark = 项目的质量保障基础设施——每次升级 Docling 版本、调整 Chunking 策略、修改融合算法，跑一遍就成了。
