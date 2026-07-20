//! 确定性中文分词器 —— 字符 Bigram + Unigram 回退。
//!
//! 本模块不依赖任何外部分词库，也不使用随机性或统计模型。
//! 同样输入永远产生同样输出，保证三种检索策略在同一条件下可比。
//!
//! ## 设计选择
//!
//! - **Bigram 为主**：将相邻两个字符合并为一个 token，例如 "材料迟交" → ["材料","料迟","迟交"]
//! - **Unigram 回退**：单字 query 无法形成 bigram 时保留单字，例如 "我" → ["我"]
//! - **中英混合**：英文单词按空格切分后保留完整单词，中文部分继续 bigram
//! - **标点与空白**：分词前先归一化（全角转半角、移除多余空白），但保留在 token 中以便匹配
//!
//! ## 为什么不是 jieba / 词典分词？
//!
//! 本课目标是建立检索→评测→证据闭环，不是宣称分词已生产可用。
//! Bigram 在中文检索中是一个经典强 baseline（参考
//! Nie et al., "On the use of words and n-grams for Chinese information retrieval"）。
//! 学生理解其原理后可以替换为任意分词器，评测框架不变。

/// 对中文文本执行确定性分词，返回 token 列表。
///
/// - Bigram：每个相邻字符对形成一个 token
/// - Unigram：单字符文本或最后无法配对的字符
/// - 英文/数字连续序列保留为一个 token
/// - 全角标点转为半角，多余空白折叠为单个空格
///
/// # Examples
///
/// ```
/// use lesson_04_evidence::tokenize;
/// assert!(!tokenize("材料迟交").is_empty());
/// assert!(tokenize("材料").contains(&"材料".to_string()));
/// ```
pub fn tokenize(text: &str) -> Vec<String> {
    // 归一化：全角转半角，折叠空白
    let normalized = normalize(text);

    if normalized.is_empty() {
        return vec![];
    }

    let mut tokens = Vec::new();

    // 按空白和标点分割为片段，每个片段内保留连续性
    let segments = split_segments(&normalized);

    for segment in segments {
        if segment.is_empty() {
            continue;
        }

        // 将片段进一步按文字类型拆分为子片段（ASCII vs CJK）
        for sub in split_by_script(&segment) {
            if sub.is_empty() {
                continue;
            }

            if sub.chars().all(|c| c.is_ascii_alphanumeric()) {
                // 英文/数字子片段：整体保留
                tokens.push(sub.to_lowercase());
            } else {
                // 含中文子片段：做 bigram 切分
                let chars: Vec<char> = sub.chars().collect();
                if chars.len() == 1 {
                    // 单字符：保留 unigram
                    tokens.push(chars[0].to_string());
                } else {
                    // 多字符：生成 bigram + 每个 unigram 作为补充
                    for i in 0..chars.len() - 1 {
                        let bigram: String = chars[i..i + 2].iter().collect();
                        tokens.push(bigram);
                    }
                    // 同时保留 unigram，提升单字匹配召回
                    for &ch in &chars {
                        tokens.push(ch.to_string());
                    }
                }
            }
        }
    }

    // 去重但保持顺序（保留首次出现位置，对排序计分有微弱影响但不改变语义）
    let mut seen = std::collections::HashSet::new();
    let mut deduped = Vec::new();
    for token in tokens {
        if seen.insert(token.clone()) {
            deduped.push(token);
        }
    }

    deduped
}

/// 文本归一化：全角转半角，去除首尾空白，折叠中间连续空白。
fn normalize(text: &str) -> String {
    let text = text.trim();
    let mut result = String::with_capacity(text.len());

    for ch in text.chars() {
        match ch {
            // 全角空格 → 半角空格
            '\u{3000}' => result.push(' '),
            // 全角标点符号区域 (FF01–FF5E) → 半角 (0020–007E)
            // 包括 FF01–FF5E 范围内的所有全角字符（含括号、逗号等）
            c @ '\u{FF01}'..='\u{FF5E}' => {
                let half = char::from_u32(c as u32 - 0xFEE0).unwrap_or(c);
                result.push(half);
            }
            // 中文书名号、引号等不在 FF01-FF5E 范围内的标点
            '\u{300A}' => result.push('<'),  // 《
            '\u{300B}' => result.push('>'),  // 》
            '\u{300C}' => result.push('['),  // 「
            '\u{300D}' => result.push(']'),  // 」
            '\u{300E}' => result.push('['),  // 『
            '\u{300F}' => result.push(']'),  // 』
            // 中文逗号、句号保留（对中文检索有意义）
            other => result.push(other),
        }
    }

    // 折叠连续空白为单个空格
    let folded: String = result
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ");

    folded
}

/// 按文字类型将片段拆分为连续的同类型子片段。
///
/// 例如 "hello世界" → ["hello", "世界"]，"保证金2%" → ["保证金", "2", "%"]
fn split_by_script(text: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut current_is_ascii: Option<bool> = None;

    for ch in text.chars() {
        let ch_is_ascii = ch.is_ascii_alphanumeric();

        match current_is_ascii {
            None => {
                // 第一个字符
                current.push(ch);
                current_is_ascii = Some(ch_is_ascii);
            }
            Some(is_ascii) if is_ascii == ch_is_ascii => {
                // 同类型，继续累积
                current.push(ch);
            }
            Some(_) => {
                // 类型切换，保存当前并开始新的
                if !current.is_empty() {
                    result.push(std::mem::take(&mut current));
                }
                current.push(ch);
                current_is_ascii = Some(ch_is_ascii);
            }
        }
    }
    if !current.is_empty() {
        result.push(current);
    }

    result
}

/// 按空白/标点将文本切分为连续片段。
/// 中文文本中，标点符号之间的连续中文字符构成一个片段。
fn split_segments(text: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();

    for ch in text.chars() {
        // 空白和 ASCII 标点视为分隔符
        if ch.is_whitespace() || is_punctuation(ch) {
            if !current.is_empty() {
                segments.push(std::mem::take(&mut current));
            }
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        segments.push(current);
    }

    segments
}

/// 判断是否为标点符号（ASCII 标点 + 常见中文标点）。
fn is_punctuation(ch: char) -> bool {
    // ASCII 标点
    if ch.is_ascii_punctuation() {
        return true;
    }
    // CJK 标点符号与全角标点 Unicode 区间
    matches!(
        ch,
        '\u{3000}'..='\u{303F}'   // CJK 符号与标点（、。「」『』…等）
        | '\u{FF01}'..='\u{FF5E}' // 全角标点（！＂＃＄％…等）
        | '\u{FF61}'..='\u{FF65}' // 半角片假名标点
        | '\u{FE10}'..='\u{FE1F}' // 竖直排版标点
    )
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_string_returns_empty() {
        assert!(tokenize("").is_empty());
    }

    #[test]
    fn whitespace_only_returns_empty() {
        assert!(tokenize("   ").is_empty());
    }

    #[test]
    fn single_chinese_character_returns_unigram() {
        let tokens = tokenize("我");
        assert!(tokens.contains(&"我".to_string()));
    }

    #[test]
    fn two_chinese_characters_produce_bigram_and_unigrams() {
        let tokens = tokenize("材料");
        assert!(tokens.contains(&"材料".to_string())); // bigram
        assert!(tokens.contains(&"材".to_string())); // unigram
        assert!(tokens.contains(&"料".to_string())); // unigram
    }

    #[test]
    fn three_character_query_produces_two_bigrams() {
        let tokens = tokenize("迟交材料");
        assert!(tokens.contains(&"迟交".to_string()));
        assert!(tokens.contains(&"交材".to_string()));
        assert!(tokens.contains(&"材料".to_string()));
    }

    #[test]
    fn determinism_same_input_same_output() {
        let a = tokenize("投标保证金上限");
        let b = tokenize("投标保证金上限");
        assert_eq!(a, b);
    }

    #[test]
    fn mixed_chinese_and_english_handles_both() {
        let tokens = tokenize("hello世界");
        assert!(tokens.contains(&"hello".to_string()));
        // 中文字符部分也应产生 tokens
        // bigram 跨中英边界不会产生 "世"+"界" 的 bigram，但 unigram 应存在
        let _has_chinese_tokens = tokens.iter().any(|t| {
            t == "世" || t == "界" || t == "世界"
        });
        // 至少应包含中文 unigram
        assert!(tokens.contains(&"世".to_string()));
        assert!(tokens.contains(&"界".to_string()));
    }

    #[test]
    fn fullwidth_punctuation_is_normalized() {
        // 全角逗号、句号等转为半角，不影响中文字符提取
        let tokens = tokenize("材料迟交，需要登记。");
        assert!(tokens.contains(&"材料".to_string()));
        assert!(tokens.contains(&"迟交".to_string()));
    }

    #[test]
    fn numbers_are_preserved() {
        let tokens = tokenize("保证金不超过2%");
        assert!(tokens.contains(&"2%".to_string()) || tokens.contains(&"2".to_string()));
    }
}
