# Day 5：解析 Pipeline 独立机制实验

> 前 4 天你理解了 PDF 内核、Rust↔Python 子进程、三引擎融合和质量评估。今天用独立 Demo 模拟 Worker 的关键状态变化；不要求接入真实 Redis、MinIO，也不修改项目代码。

---

## 目标

搭建一个最小解析 Worker 模拟器：输入本地 PDF 或伪造任务→计算哈希→选择解析策略→输出简化 `ParsedDocument` JSON→记录成功、部分失败或超时状态。

小组必做只需验证以下两个机制：

1. 相同文件重复提交时能够识别并复用结果；
2. 一个解析引擎超时或返回非法 JSON 时，能够记录失败并选择降级路径。

Redis Streams、MinIO、分片并行和真实回调作为骨干选做。

---

## 架构

```
POST /api/v1/parse { file }
  → Controller: 存文件到 MinIO → SHA256 哈希
  → 检查去重缓存: 已解析过? → 直接返回 document_id
  → XADD redis:parse:tasks * { file_key, file_hash, callback_url }
  → 返回 task_id (HTTP 202)

Worker (独立线程):
  XREADGROUP GROUP parse-workers worker-1 BLOCK 5000 STREAMS parse:tasks >
  → 收到消息
  → 下载文件 (MinIO presigned URL)
  → 检查页数:
      ≤ 50 页 → 单进程解析
      > 50 页 → 分片并行 (4 片)
  → 调 Python ML (Docling/MinerU/PaddleOCR) → ParsedDocument
  → 写入 MinIO: parsed/{document_id}.json
  → 更新 DB: parse_task(status=COMPLETED)
  → 回调 G8: POST {callback_url} { status: "COMPLETED", document_id }
  → XACK
```

---

## 核心实现

### Redis Streams 消费

```rust
use redis::streams::{StreamReadOptions, StreamReadReply};

async fn consume_loop(redis: &redis::aio::ConnectionManager) -> Result<()> {
    loop {
        let reply: StreamReadReply = redis
            .xread_options(
                &["parse:tasks"],
                &[">"],  // 只读新消息
                &StreamReadOptions::default()
                    .group("parse-workers", "worker-1")
                    .block(5000)
                    .count(1),
            )
            .await?;

        for stream_key in reply.keys {
            for entry in stream_key.ids {
                let file_key = entry.get("file_key").unwrap();
                let file_hash = entry.get("file_hash").unwrap();
                let callback_url = entry.get("callback_url").unwrap();

                match process_task(file_key, file_hash).await {
                    Ok(doc_id) => {
                        callback_g8(&callback_url, doc_id).await?;
                        redis.xack("parse:tasks", "parse-workers", &[&entry.id]).await?;
                    }
                    Err(e) => {
                        log::error!("Parse failed: {e}");
                        callback_g8_error(&callback_url, &e).await?;
                        redis.xack("parse:tasks", "parse-workers", &[&entry.id]).await?;
                    }
                }
            }
        }
    }
}
```

### 文件哈希去重

```rust
async fn process_task(file_key: &str, file_hash: &str) -> Result<String> {
    // 检查缓存
    if let Some(doc_id) = cache_get(file_hash).await? {
        log::info!("Cache hit: {file_hash} → {doc_id}");
        return Ok(doc_id);
    }

    // 下载 + 解析
    let data = download_from_minio(file_key).await?;
    let document = parse_document(&data).await?;

    // 写入 MinIO + 缓存 + 返回
    let doc_id = &document.document_id;
    let json = serde_json::to_string(&document)?;
    upload_to_minio(&format!("parsed/{doc_id}.json"), json.as_bytes()).await?;
    cache_set(file_hash, doc_id).await?;

    Ok(doc_id.clone())
}
```

### 分片并行

```rust
async fn parse_document(data: &[u8]) -> Result<ParsedDocument> {
    let page_count = get_page_count(data)?;

    if page_count <= 50 {
        // 单进程
        parse_single(data).await
    } else {
        // 分片：拆为 4 片，每片独立 Python 进程
        let shard_size = page_count / 4;
        let shards = vec![
            (1..shard_size),
            (shard_size+1..shard_size*2),
            (shard_size*2+1..shard_size*3),
            (shard_size*3+1..=page_count),
        ];

        let handles: Vec<_> = shards.into_iter()
            .map(|range| {
                let data = data.to_vec();
                tokio::task::spawn_blocking(move || parse_page_range(&data, range))
            })
            .collect();

        let results = futures::future::join_all(handles).await;
        // 按页码合并结果
        merge_shards(results)
    }
}
```

---

## 验收标准

| 验收项 | 权重 | 怎么测 |
|--------|------|--------|
| 完整链路：上传→解析→MinIO→回调 | 25% | e2e 测试 |
| 文件哈希去重 | 15% | 上传同一文件两次→第二次秒返回 document_id |
| 坏页容错（partial_errors） | 15% | 损坏 PDF→ParsedDocument 中标记失败页 |
| 进程池复用 | 15% | 连续解析 5 份→Python 进程不重启 |
| 分片并行（> 50 页文档） | 15% | 100 页文档→4 分片→耗时 < 2×单进程耗时 |
| 设计决策文档 | 15% | 去重策略/分片策略/PCB协议 |

---

## 设计决策文档

1. **为什么 Redis Streams 而不是直接 HTTP 同步** — 100 页 PDF 解析需 30s，HTTP 同步不可接受。Streams + Consumer Group + at-least-once 语义
2. **为什么进程池而不是每次 spawn** — Docling 模型加载 3-5s，预加载省 70% 时间
3. **为什么分片阈值是 50 页** — 经验值。单进程 50 页 ~15s，100 页 ~30s。分 4 片→4 进程各 ~8s→合并 ~2s→总 ~10s

---

## 与标书审核项目的关系

这个实验用于理解项目解析链路的状态、协议和失败边界。Demo 不直接作为项目 scaffold；若后续需要迁移，必须重新核对现有接口、依赖、资源限制和测试要求。
