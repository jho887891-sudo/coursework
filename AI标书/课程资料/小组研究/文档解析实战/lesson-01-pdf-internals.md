# Day 1：PDF 内核 — 从二进制到可渲染文本

> 打开一份招标文件 PDF，你看到的是排版精美的表格和文字。hexdump 打开——全是二进制垃圾。今天你用 Rust 从这些垃圾中提取出文字，理解为什么中文 PDF 的 `extract_text()` 经常返回乱码。

---

## 学习目标

1. 理解 PDF 文件的物理结构：Header / Objects / xref / Trailer
2. 用 Rust 解析 xref 偏移表并定位任意 Object
3. 理解 CID 字体编码和 CMap 映射——中文乱码的根因
4. 手写从 Content Stream 中提取文本操作数（Tj/TJ）的解析器

---

## 核心概念

### 1. PDF 不是纯文本文件

用 `xxd` 或 Rust 的 `std::fs::read` 打开一份 PDF 的前 200 字节：

```
%PDF-1.7
%âãÏÓ
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
...
xref
0 6
0000000000 65535 f 
0000000015 00000 n 
0000000072 00000 n 
...
trailer
<< /Size 6 /Root 1 0 R >>
startxref
178
%%EOF
```

**四个区域**：
- **Header**：`%PDF-1.7` ——版本声明
- **Body**：`1 0 obj ... endobj` ——对象定义。一个 PDF 由几百到几千个 Object 组成
- **xref 表**：每个 Object 在文件中的字节偏移量。这是"目录"——有了它才能 O(1) 定位任意 Object
- **Trailer**：指向 Root Object（Catalog）+ xref 表的起始偏移

### 2. PDF 对象模型

```rust
// PDF 有 8 种对象类型
enum PdfObject {
    Boolean(bool),                    // true / false
    Integer(i64),                     // 42
    Real(f64),                        // 3.14
    String(Vec<u8>),                  // (Hello)
    Name(String),                     // /Type
    Array(Vec<PdfObject>),            // [1 0 R /XYZ 72 720 0]
    Dictionary(HashMap<String, PdfObject>), // << /Type /Page /Contents 4 0 R >>
    Stream { dict: Dictionary, data: Vec<u8> },  // 压缩数据（通常是页面内容）
    Null,
    Reference(u32, u32),              // 4 0 R ——指向 Object 4, Generation 0
}
```

**为什么需要 xref 表**：当 PDF 说 `/Contents 4 0 R` ——它不是在说"字符串 4 0 R"，而是"请跳到 xref[4] 的字节偏移量去读 Object 4"。xref 表就是 PDF 的"间接引用"（Indirect Reference）分辨率。

### 3. 页面内容流 — 文本在哪里

一个 Page Object 的 `/Contents` 指向一个 Stream。解压（通常是 FlateDecode = zlib）后得到**页面操作符序列**：

```
BT                              % Begin Text
/F1 12 Tf                       % 选择 F1 字体，12pt
72 720 Td                       % 移动到坐标 (72, 720)
(投标人须知) Tj                  % 绘制文本 "投标人须知"
ET                              % End Text
```

**文本提取 = 解析 Content Stream 中的 Tj/TJ 操作符**：

```rust
// TJ 操作符返回一个数组——混合字符串和间距调整
// [(投标人) -5 (须) -3 (具备)] TJ
// = "投标人" 后移 5 个单位 + "须" 后移 3 个单位 + "具备"

fn parse_text_operations(content: &[u8]) -> Vec<String> {
    let mut texts = vec![];
    let mut i = 0;
    while i < content.len() {
        match content[i..] {
            _ if content[i..].starts_with(b"BT") => { /* 进入文本块 */ }
            _ if content[i..].starts_with(b"ET") => { /* 退出文本块 */ }
            _ if content[i..].starts_with(b"Tj") => {
                // 提取前一操作数（括号内的字符串）
                let s = extract_string_before(&content[..i]);
                texts.push(s);
            }
            _ => {}
        }
        i += 1;
    }
    texts
}
```

### 4. 字体编码 — 中文乱码的根因

**英文 PDF**：`(Hello) Tj` → "H" = ASCII 72 → 直接映射到字符 "H"。简单。

**中文 PDF（CID 字体）**：文本实际上存的是 `<0032>` 这样的 CID（Character ID），不是可读字符。要通过 CMap 表映射 CID→Unicode：

```
<0032> Tj   → CID=0x0032 → CMap 查表 → U+6295 ("投")
<0033> Tj   → CID=0x0033 → CMap 查表 → U+6807 ("标")
```

**乱码的三种原因**：

1. **CMap 内嵌但非标准**：PDF 内嵌了 CMap，但使用了自定义映射→你看到的 `0x0032` 映射到字符不是"投"。Docling 的策略：不信任内嵌 CMap→渲染为图像→OCR
2. **CMap 引用外部字体**：`/FontDescriptor` 指向系统字体——Windows 有 SimSun，Linux 没有→映射失败→回退到 `.notdef`（方框）
3. **ToUnicode CMap 缺失**：PDF 里根本没有 ToUnicode 映射表→提取出来的全是 CID 数字。需要 OCR 补全

```rust
// Rust 端读取 Font 字典
// << /Type /Font /Subtype /Type0 /BaseFont /SimSun /Encoding /UniCNS-UTF16-H
//    /ToUnicode 12 0 R >>
// → ToUnicode 指向 Object 12 → 又是一个 Stream → 解压得到 CMap 映射表

fn extract_font_encoding(font_dict: &Dictionary) -> Result<Encoding> {
    if let Some(PdfObject::Reference(to_unicode_id, _)) = font_dict.get("ToUnicode") {
        let cmap_stream = resolve_object(to_unicode_id)?;
        let cmap_data = flate2::decode(&cmap_stream.data)?;
        Ok(Encoding::Cmap(parse_cmap(&cmap_data)?))
    } else if let Some(encoding_name) = font_dict.get("Encoding") {
        Ok(Encoding::Predefined(encoding_name.to_string()))
    } else {
        Ok(Encoding::Missing)  // → 需要 OCR
    }
}
```

---

## 动手

### 任务 1：Rust PDF 结构解析器

用 `nom` 写一个最小的 PDF 解析器：
- `parse_xref(input)` → 提取所有 Object 的字节偏移量
- `parse_object(input, offset)` → 定位到偏移，解析 Object（Dictionary/Stream/Reference）
- `resolve_reference(ref)` → 通过 xref 表跳转到被引用的 Object

### 任务 2：文本提取 + 编码诊断

解压 Content Stream → 提取 Tj/TJ 操作数 → 尝试用 CMap 映射为 Unicode → 如果映射失败→输出"该字符 CID=0x0032 无法映射，原因：ToUnicode CMap 缺失"

### 任务 3：对比实验

用 Rust 手写提取器 vs `pdfplumber` vs `pdf-extract` crate 在 5 份招标文件 PDF 上对比文本提取结果。标注差异位置，分析根因。

---

## 验收标准

- [ ] xref 解析器正确提取所有 Object 偏移量
- [ ] Tj/TJ 文本提取器正确输出英文文本
- [ ] 至少诊断出 1 个中文乱码的根因（ToUnicode 缺失/非标准编码/外部字体）
- [ ] 对比报告：手写提取 vs pdfplumber vs pdf-extract 的差异分析

---

## 思考题

1. PDF 的 xref 表是追加式更新的——修改 PDF 后新 xref 追加到文件末尾。如果你只读了第一个 xref 表，会漏掉修改过的 Object。怎么处理增量更新？
2. Content Stream 中的文本顺序 ≠ 人类阅读顺序。一个两列布局的 PDF 页面，文本可能在操作符序列中按"先左列再右列"或"交错"出现。怎么重建正确的阅读顺序？
3. CID 字体 + 内嵌 CMap 的 PDF，为什么 Docling 选择"渲染为图像再 OCR"而不是信任 CMap？这反映了什么工程哲学？

---

## 与标书审核项目的关系

你今天写的解析器不是用来替换 Docling 的——是用来**理解 Docling 为什么这样设计**。当你拿到一份解析失败的 PDF（Docling 输出 `partial_errors`），你能用 Rust 读取原始 PDF 诊断根因——CMap 缺失？非标准编码？图片型 PDF？这份诊断能力是 G1 组的核心竞争力。
