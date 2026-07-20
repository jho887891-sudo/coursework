use crate::MeetingRequest;
use std::path::Path;

/// Workflow 校验错误
#[derive(Debug, PartialEq, Eq)]
pub enum WorkflowError {
    MissingTopic,
    MissingDate,
    MissingParticipants,
    MaterialNotFound(String),
}

/// Workflow 助手：固定步骤，逐步校验。
///
/// ## 核心特征
/// - 严格按顺序校验每一个字段
/// - 任何缺失 / 无效 → **立即终止并报错**
/// - 最稳定：相同输入永远得到相同输出
/// - 不灵活：不能处理预期之外的偏差
pub struct MeetingWorkflow;

impl MeetingWorkflow {
    /// 逐步校验 MeetingRequest，全部通过后才生成摘要。
    /// 任何一步失败立即返回 Err。
    pub fn execute(request: &MeetingRequest) -> Result<String, WorkflowError> {
        // 步骤 1：主题不能为空
        if request.topic.trim().is_empty() {
            return Err(WorkflowError::MissingTopic);
        }

        // 步骤 2：日期必须明确提供
        let date = request.date.as_ref().ok_or(WorkflowError::MissingDate)?;

        // 步骤 3：必须有参与人
        if request.participants.is_empty() {
            return Err(WorkflowError::MissingParticipants);
        }

        // 步骤 4：如果指定了材料路径，文件必须存在
        if let Some(path) = &request.material_path {
            if !Path::new(path).exists() {
                return Err(WorkflowError::MaterialNotFound(path.clone()));
            }
        }

        // 全部校验通过 → 生成摘要
        let material_note = match &request.material_path {
            Some(path) => format!("材料「{}」已确认可用。", path),
            None => "无补充材料。".into(),
        };

        Ok(format!(
            "📋 会议摘要（Workflow 生成）\n\
             ────────────\n\
             主题：{}\n\
             日期：{}\n\
             参与人：{}\n\
             材料：{}\n\
             ────────────\n\
             流程已完成，所有字段校验通过。",
            request.topic,
            date,
            request.participants.join("、"),
            material_note,
        ))
    }
}
