# Day 3：多引擎编排 + ParsedDocument Schema

> Day 2 你用 Rust 驾驭了 Docling。但一份招标文件里——正文是标准排版（Docling 擅长）、表格是三线表（MinerU 擅长）、资质证书页是扫描件（PaddleOCR 擅长）。今天你做三引擎调度——根据页面特征自动选择最优引擎，融合结果，输出统一的 ParsedDocument。

---

## 学习目标

1. 设计三引擎调度策略（Docling 主力→MinerU 表格增强→PaddleOCR 兜底）
2. 实现引擎结果融合算法（表格替换/文本去重/bbox 统一）
3. 定义 ParsedDocument Schema（serde）并对接项目接口契约

---

## 核心概念

### 1. 三引擎决策树

```
每个 PDF 页面 → 评估页面特征：

内嵌文本量 > 100 字符？
  ├─ 是 → 使用 Docling 主解析
  │       └─ 表格置信度 > 0.7？
  │           ├─ 是 → 保留 Docling 表格
  │           └─ 否 → MinerU 对该页表格区域重新提取 → 替换
  └─ 否 → 该页为图片型 PDF（扫描件）
          └─ 整体用 PaddleOCR
```

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
enum EngineChoice {
    Docling,             // 标准页面
    DoclingWithMinerU,   // 有表格但 Docling 置信度不够
    PaddleOCR,           // 扫描件页面
}

#[derive(Debug)]
struct PageFeatures {
    page_num: usize,
    embedded_text_len: usize,    // 内嵌文本字符数
    image_area_ratio: f64,       // 图片占页面比例
    has_tables: bool,            // Docling 初步检测到表格
    docling_table_confidence: Option<f64>,  // 表格置信度
}

fn select_engine(features: &PageFeatures) -> EngineChoice {
    if features.embedded_text_len < 50 {
        return EngineChoice::PaddleOCR;  // 扫描件
    }
    if features.has_tables
        && features.docling_table_confidence.unwrap_or(1.0) < 0.7
    {
        return EngineChoice::DoclingWithMinerU;
    }
    EngineChoice::Docling
}
```

**为什么不是"三引擎全跑取最优"**：成本——Docling 2s/页 + MinerU 5s/表格 + PaddleOCR 10s/页。100 页文档全跑三引擎 = 1700 秒 ≈ 28 分钟。决策树先筛选页面特征→最多每页跑两个引擎。

### 2. 结果融合算法

#### 表格替换

```rust
fn merge_tables(docling_page: &Page, mineru_page: &Page) -> Vec<Table> {
    let mut tables = vec![];

    for dl_table in &docling_page.tables {
        // 找 MinerU 中同一位置的表格
        let mineru_match = mineru_page.tables.iter()
            .find(|mt| bbox_iou(&dl_table.bbox, &mt.bbox) > 0.8);

        match mineru_match {
            Some(mt) if dl_table.confidence < 0.7 => {
                // Docling 置信度低 → 用 MinerU 的表格
                tables.push(Table {
                    bbox: dl_table.bbox,          // 位置保留 Docling 的
                    rows: mt.rows.clone(),        // 内容用 MinerU 的
                    source: "mineru".into(),
                    confidence: mt.confidence,
                });
            }
            _ => {
                // MinerU 没找到或 Docling 置信度够 → 保留 Docling
                tables.push(dl_table.clone());
            }
        }
    }

    // MinerU 发现但 Docling 漏掉的表格 → 补充
    for mt in &mineru_page.tables {
        if !tables.iter().any(|t| bbox_iou(&t.bbox, &mt.bbox) > 0.8) {
            tables.push(mt.clone());
        }
    }

    tables
}

/// bbox 的 IoU（Intersection over Union）
fn bbox_iou(a: &BBox, b: &BBox) -> f64 {
    let ix = f64::max(0.0, f64::min(a.x + a.w, b.x + b.w) - f64::max(a.x, b.x));
    let iy = f64::max(0.0, f64::min(a.y + a.h, b.y + b.h) - f64::max(a.y, b.y));
    let intersection = ix * iy;
    let union = a.w * a.h + b.w * b.h - intersection;
    if union == 0.0 { 0.0 } else { intersection / union }
}
```

#### 文本去重

MinerU 和 Docling 可能提取了同一段文本（如表格标题）。去重策略：编辑距离 < 5 + 两段文本的 embedding cosine similarity > 0.95。

### 3. ParsedDocument Schema

这是 G1 对下游 8 个组的**接口契约**。所有字段必须 `#[serde(rename = "camelCase")]` 以匹配 JSON 规范。

```rust
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedDocument {
    pub document_id: String,       // SHA256(file_content)[:16]
    pub file_name: String,
    pub page_count: usize,
    pub chapters: Vec<Chapter>,
    pub clauses: Vec<Clause>,
    pub tables: Vec<Table>,
    pub images: Vec<DocumentImage>,
    pub parse_meta: ParseMeta,
}

#[derive(Serialize, Deserialize)]
pub struct Chapter {
    pub id: String,                // "ch_001"
    pub title: String,
    pub level: u8,                 // 1=章, 2=节, 3=小节
    pub page_start: usize,
    pub page_end: usize,
    pub clause_ids: Vec<String>,   // ["cl_001", "cl_002"]
}

#[derive(Serialize, Deserialize)]
pub struct Clause {
    pub id: String,                // "cl_001" = SHA256(doc_id + page + index)[:8]
    pub chapter_id: String,
    pub text: String,
    pub clause_type: String,       // "requirement" | "definition" | "procedure"
    pub page: usize,
    pub bbox: BBox,
    pub block_ids: Vec<String>,    // 构成此 Clause 的原始 Block IDs
}

#[derive(Serialize, Deserialize)]
pub struct Table {
    pub id: String,
    pub caption: Option<String>,
    pub page: usize,
    pub header: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub bbox: BBox,
}

#[derive(Serialize, Deserialize)]
pub struct BBox {
    pub x: f64, pub y: f64,        // PDF 坐标系（左下角原点）
    pub w: f64, pub h: f64,
}

#[derive(Serialize, Deserialize)]
pub struct ParseMeta {
    pub parse_time_ms: u64,
    pub parser_version: String,    // "docling-2.0.1+mineru-1.2.0"
    pub partial_errors: Vec<ParseError>,
}

#[derive(Serialize, Deserialize)]
pub struct ParseError {
    pub page: usize,
    pub engine: String,            // "docling" | "mineru" | "paddleocr"
    pub error: String,
}

#[derive(Serialize, Deserialize)]
pub struct Chunk {
    pub id: String,
    pub text: String,
    pub clause_id: Option<String>,
    pub bbox_refs: BBox,           // Chunk 内所有 block 的聚合 bbox（预计算缓存）
}
```

**关键设计决策**：

- **Block ID 嵌入 Clause ID**：`cl_001` 由 `SHA256(doc_id + page + paragraph_index + content_prefix)` 生成。G4 Agent 引用 `clause_id` → G9 前端通过 `clause_id` 反查 `bbox` → 高亮定位
- **bbox 坐标体系统一为 PDF 坐标**：左下角原点。G9 前端自己做 Canvas→DOM 变换。不预计算 CSS 坐标——前端容器尺寸动态变化
- **partial_errors 不中断整文档**：单页解析失败→标记在 `partial_errors`→其他页正常输出。下游可以跳过损坏页继续审核

---

## 动手

### 任务 1：实现引擎选择器

写 `EngineSelector` struct——输入 Docling 初步解析结果→计算每页特征→输出每页的 `EngineChoice`。关键：特征计算要快（< 10ms/页），不能比解析本身还慢。

### 任务 2：表格融合

用 Docling + MinerU 解析同一份含表格的招标文件→实现 `merge_tables()`→对比融合前后表格的行列数是否正确。

### 任务 3：ParsedDocument 序列化

用 `serde` 将解析结果序列化为 `ParsedDocument` JSON→对照片段验证字段名 camelCase→存为 `.json` 文件。

---

## 验收标准

- [ ] Engine 选择器正确区分标准页/表格页/扫描件页
- [ ] 表格融合后行列数正确率 > 95%
- [ ] ParsedDocument JSON 通过 `serde` 序列化→反序列化→字段无损

---

## 思考题

1. 决策树的"内嵌文本量阈值 50 字符"——这个阈值怎么确定？太高会误判扫描件为标准页（OCR 反而被跳过），太低会误判标准页为扫描件（不必要的 OCR 开销）
2. bbox IoU 的阈值 0.8——两个表格的 bbox 有 80% 重叠算"同一表格"。如果 PDF 是一个跨页表格——在页面 A 和页面 B 各有一半，IoU=0。融合算法会怎么处理？
3. ParsedDocument 的 `clause_id` 依赖 Docling 的段落分割。如果 Docling 升级到 3.0→段落分割算法改变了→相同 PDF 产生不同的 `clause_id`→G4 Agent 引用的旧 clause_id 全部失效。你怎么处理版本迁移？

---

## 与标书审核项目的关系

你今天定义的 `ParsedDocument` struct 就是 G1 组的输出接口。项目 `docs/schemas/parsed-document.schema.json` 的 Rust 版本就是这份 `schema.rs`。G2/G3/G4/G9 四个组都依赖这份 Schema——你改一个字段名，四个组都要跟着改。所以今天也是"接口契约"的第一课——怎么设计稳定、可扩展、向后兼容的数据格式。
