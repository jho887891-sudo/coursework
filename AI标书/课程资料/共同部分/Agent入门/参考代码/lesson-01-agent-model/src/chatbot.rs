use crate::MeetingRequest;

/// Chatbot 助手：纯函数，无状态，无校验。
///
/// ## 核心特征
/// - 不管输入是否完整，**总是生成回答**
/// - 缺日期 → 自行补全（可能猜错）
/// - 材料路径不存在 → 可能声称"已读取"（幻觉风险）
/// - 用户拒绝补充 → 继续生成（不尊重边界）
pub struct ChatbotAssistant;

impl ChatbotAssistant {
    /// 接收 MeetingRequest，直接返回回答字符串。
    /// 不返回 Result —— 因为 Chatbot 永远不会"失败"。
    pub fn respond(request: &MeetingRequest) -> String {
        let date = request
            .date
            .clone()
            .unwrap_or_else(|| "本周五（自动推定）".into());

        let participants_str = if request.participants.is_empty() {
            "未指定".into()
        } else {
            request.participants.join("、")
        };

        let material_note = match &request.material_path {
            Some(path) => {
                // Chatbot 不检查文件是否真实存在 ——
                // 它可能声称读取了不存在的文件（幻觉）
                format!("已根据「{}」的内容整理要点。", path)
            }
            None => "无补充材料。".into(),
        };

        format!(
            "📋 会议摘要\n\
             ────────────\n\
             主题：{}\n\
             日期：{}\n\
             参与人：{}\n\
             材料：{}\n\
             ────────────\n\
             建议：请提前准备发言要点。",
            request.topic, date, participants_str, material_note
        )
    }
}
