# 图数据库实战：从图论到 GraphRAG

> 5 天深度原理课。从图论、Neo4j 存储与查询到 GraphRAG，通过小图和独立 Demo 理解机制。完整论文算法复现作为骨干选做。

本课遵循[专项课程统一学习规范](../../专项课程统一学习规范.md)。课程使用独立 Neo4j 容器和小规模自建数据，不修改项目知识库。

---

## 你需要准备什么

| 要求 | 说明 |
|------|------|
| Rust 基础 | struct / enum / trait / async / Result / reqwest / serde。不需要上过 Rust 课程——自己学过就行 |
| Docker | 已安装，每天都要跑 Neo4j |
| Neo4j | `docker pull neo4j:5-community` |
| 图论 | **零基础可学**——Day 1 从头讲 |
| 好奇心 | 想知道"为什么图遍历比 JOIN 快 1000 倍" |

**不需要**：向量数据库经验、机器学习背景、RAG 课程——全部概念在本课程内闭环。

### 验证环境

```bash
# 确认 Rust 工具链
rustc --version  # ≥ 1.96
cargo --version

# 确认 Docker
docker --version

# 拉取 Neo4j 镜像
docker pull neo4j:5-community
```

### 前置准备（开课前完成）

```bash
# 创建数据目录
mkdir -p ~/neo4j/data ~/neo4j/logs ~/neo4j/import

# 启动 Neo4j（Day 1 用）
docker run -d --name neo4j-learn \
  -p 7474:7474 -p 7687:7687 \
  -v ~/neo4j/data:/data \
  -v ~/neo4j/logs:/logs \
  -v ~/neo4j/import:/var/lib/neo4j/import \
  -e NEO4J_AUTH=neo4j/password \
  neo4j:5-community

# 验证：浏览器打开 http://localhost:7474
```

---

## 你会学到什么

| 天次 | 主题 | 你会理解或验证什么 |
|------|------|------------|
| Day 1 | **图论基础 + 图数据模型** | 图论核心概念理解 + Property Graph 建模 + 20 条 Cypher 查询 |
| Day 2 | **Neo4j 内核深挖** | hexdump 读存储文件 + 手写查询计划解释器 + Page Cache 命中率实验报告 |
| Day 3 | **法律知识图谱建模** | 法律领域 Schema + 实体消歧 Pipeline + 500 法规 2000 条款批量导入 |
| Day 4 | **GraphRAG 深度** | 社区检测推演 + 简化 Demo + 图检索对比；完整 Leiden 选做 |
| Day 5 | **知识生命周期实验** | 小型知识图 + 去重/溯源/时效 + 图检索最小闭环 |

---

## 怎么学

```
Day 1  图论概念 → Property Graph → Cypher → 动手建图
Day 2  Neo4j 存储文件 → Page Cache → 查询编译 → 代价优化
Day 3  Schema 设计 → 实体消歧 → 关系抽取 → 批量导入
Day 4  GraphRAG 论文 → Leiden → 社区摘要 → 图增强检索
Day 5  Curator Pipeline 内核 → 图嵌入 → 大作业
```

---

## 代码怎么写

**Demo 以 Rust 为主。** Neo4j 通过 `neo4rs` 连接；必修部分亲手实现小图建模、查询和一个简化算法机制，完整 Leiden、Node2Vec 和生产 Pipeline 为骨干选做。

| 任务 | Rust 实现 |
|------|----------|
| Neo4j 连接 | `neo4rs` crate |
| hexdump 解析 | 手写二进制解析（`std::fs::read` + 字节操作） |
| 实体消歧/归一化 | 手写正则 + 查表 |
| 批量导入 | 生成 Cypher → neo4rs 批量执行 |
| Leiden 算法 | 手工推演 + 简化局部移动 Demo；三阶段完整实现选做 |
| Node2Vec | 随机游走 Demo；Word2Vec 训练选做 |

---

## 与标书审核项目的关系

```
本课程 → G3/G6 组
  ├─ Day 2 Neo4j 内核 → 调优 Neo4j 生产配置
  ├─ Day 3 Schema 设计 → 项目法律知识图谱 Schema
  ├─ Day 4 GraphRAG → 图增强检索模块
  └─ Day 5 Curator 内核 → Curator Pipeline 原型

G4/G5 Agent 消费图检索能力：
  └─ graph_traverse 工具 → 调用本课程 Day 5 搭建的图检索 API
```

---

## 参考资源

- [Neo4j 源码](https://github.com/neo4j/neo4j) — Java 写的图数据库内核
- [GraphRAG 论文](https://arxiv.org/abs/2404.16130) — Microsoft, 2024
- [Leiden 社区检测论文](https://www.nature.com/articles/s41598-019-41695-z) — Traag et al., 2019
- [Node2Vec 论文](https://arxiv.org/abs/1607.00653) — Grover & Leskovec, 2016
- [Neo4j Graph Data Science](https://neo4j.com/docs/graph-data-science/current/) — GDS 库文档
