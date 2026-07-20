const PHASE_ORDER = ["state", "predict", "action", "observation", "termination"];

const baseState = {
  topic: "Q3 项目评审",
  date: "2026-08-01",
  participants: "张三、李四",
  materialPath: "test_material.txt",
  materialText: null,
  userDeclined: false,
  steps: 0,
  completed: false,
};

const actionOptions = [
  "AskForTopic",
  "AskForDate",
  "AskForParticipants",
  "AskForMaterialPath",
  "ReadMaterial",
  "ProduceChecklist",
  "Stop",
];

const scenarioDefinitions = {
  complete: {
    kicker: "SCENARIO 01 · ACTION ≠ FACT",
    title: "信息完整，但材料内容尚未读取",
    objective: "区分“拥有路径”“提出读取动作”和“环境证明读取成功”。",
    steps: [
      stateStep(1, 0, baseState, ["materialText"], "S0", {
        question: "当前哪个字段阻止系统直接完成任务？",
        explanation:
          "所有会议字段已经存在，但 material_text 仍为 None。路径只是定位信息，不是材料内容。",
        invariant: "没有 MaterialLoaded Observation，就不能声称已经读到材料。",
      }),
      predictStep(
        1,
        1,
        baseState,
        "现在应该直接 ProduceChecklist，还是先 ReadMaterial？",
        "ReadMaterial",
        {
          explanation:
            "让学生先选 ProduceChecklist 或 ReadMaterial，再追问：我们真的读到文件内容了吗？",
          invariant: "State 中不存在的事实，Policy 不能靠猜测补齐。",
        },
      ),
      actionStep(1, 2, baseState, "ReadMaterial", 'path: "test_material.txt"', {
        reason: "当前只有材料路径，material_text 仍为空。",
        explanation:
          "强调 Action 只是系统提出的意图。此刻 State 仍没有材料内容。",
        invariant: "Action 不直接修改事实状态。",
      }),
      observationStep(
        1,
        3,
        { ...baseState, steps: 1 },
        "MaterialLoaded",
        'path: "test_material.txt"\ncontent: "项目背景：需要决定是否进入下一阶段。"',
        {
          explanation:
            "只有工具真实返回 MaterialLoaded，Reducer 才能写入 material_text。",
          invariant: "Observation 必须来自 Environment，而不是来自模型自述。",
        },
      ),
      stateStep(
        2,
        4,
        {
          ...baseState,
          materialText: "项目背景：需要决定是否进入下一阶段。",
          steps: 1,
        },
        ["materialText"],
        "S1",
        {
          question: "现在还缺少完成任务所需的信息吗？",
          explanation:
            "material_text 从 None 变成真实内容。绿色字段就是本轮 Observation 造成的状态差异。",
          invariant: "Reducer 只根据已验证 Observation 更新 State。",
        },
      ),
      predictStep(
        2,
        5,
        {
          ...baseState,
          materialText: "项目背景：需要决定是否进入下一阶段。",
          steps: 1,
        },
        "所有必需信息已经齐全，下一动作是什么？",
        "ProduceChecklist",
        {
          explanation:
            "此时再选择 ReadMaterial 会产生无效重复；继续询问也没有必要。",
          invariant: "目标已经满足时，Agent 不得继续索取信息。",
        },
      ),
      actionStep(
        2,
        6,
        {
          ...baseState,
          materialText: "项目背景：需要决定是否进入下一阶段。",
          steps: 1,
        },
        "ProduceChecklist",
        "使用已验证的 State 生成会议清单",
        {
          reason: "主题、日期、参与人和真实材料内容均已具备。",
          explanation: "这不是模型说“完成了”，而是 Policy 根据状态判断目标满足。",
          invariant: "Finish 前必须验证完成条件。",
        },
      ),
      terminationStep(
        2,
        7,
        {
          ...baseState,
          materialText: "项目背景：需要决定是否进入下一阶段。",
          steps: 1,
          completed: true,
        },
        "Completed",
        "任务所需信息已经完整",
        {
          explanation: "完成后状态被锁定，不能继续接收新的环境事实。",
          invariant: "Terminal State 不接受后续状态修改。",
        },
      ),
    ],
  },

  missingDate: {
    kicker: "SCENARIO 02 · RUNTIME DECISION",
    title: "缺少日期：下一动作由当前 State 决定",
    objective: "比较自动猜测、固定失败与运行时获取信息三种控制方式。",
    steps: buildMissingDateSteps(),
  },

  readFailure: {
    kicker: "SCENARIO 03 · FAILURE AS OBSERVATION",
    title: "读取失败：错误反馈必须改变下一动作",
    objective: "观察 MaterialReadFailed 如何清除无效路径并触发恢复路径。",
    steps: buildReadFailureSteps(),
  },

  userDeclined: {
    kicker: "SCENARIO 04 · EXPLICIT TERMINATION",
    title: "用户拒绝：停止可以是正确结果",
    objective: "区分任务完成、用户取消与预算耗尽等不同终止语义。",
    steps: buildUserDeclinedSteps(),
  },
};

function buildMissingDateSteps() {
  const s0 = { ...baseState, date: null };
  const s1 = { ...baseState, date: "2026-08-01", steps: 1 };
  const s2 = {
    ...s1,
    materialText: "项目背景与会议议题。",
    steps: 2,
  };

  return [
    stateStep(1, 0, s0, ["date"], "S0", {
      question: "date = None 时，系统还能可靠地产生会议清单吗？",
      explanation:
        "此处让学生比较三种行为：Chatbot 猜测、Workflow 报错、Agent 询问。",
      invariant: "缺失的业务事实不能由模型自行伪造。",
    }),
    predictStep(
      1,
      1,
      s0,
      "缺少日期时，Agent 应该选择哪个动作？",
      "AskForDate",
      {
        explanation:
          "Agent 的区别不是“更会说”，而是能在运行时根据缺失字段选择信息动作。",
        invariant: "动作选择必须由当前 State 决定。",
      },
    ),
    actionStep(1, 2, s0, "AskForDate", "向用户请求明确日期", {
      reason: "date 缺失，且输入仍然开放。",
      explanation:
        "Workflow 会在这里返回 MissingDate；Agent 则保留任务并请求补充。",
      invariant: "询问是合法 Action，不是系统失败。",
    }),
    observationStep(
      1,
      3,
      { ...s0, steps: 1 },
      "UserProvidedDate",
      '"2026-08-01"',
      {
        explanation: "日期来自用户回复，因此可以作为可信 Observation 写入 State。",
        invariant: "用户提供事实与模型猜测必须区分。",
      },
    ),
    stateStep(2, 4, s1, ["date"], "S1", {
      question: "日期已补齐，当前还缺什么？",
      explanation:
        "date 已变绿，但 material_text 仍为空。Agent 会重新基于完整 State 决策。",
      invariant: "每次 Observation 后都应重新计算下一动作。",
    }),
    predictStep(
      2,
      5,
      s1,
      "日期已经补齐，但材料内容仍缺失，下一动作是什么？",
      "ReadMaterial",
      {
        explanation: "让学生意识到补齐一个字段不代表任务已经完成。",
        invariant: "Step success 不等于 Goal completion。",
      },
    ),
    actionStep(2, 6, s1, "ReadMaterial", 'path: "test_material.txt"', {
      reason: "材料路径存在，但材料内容尚未进入 State。",
      explanation: "这是第二轮决策，顶部流程会重新从 State 开始高亮。",
      invariant: "拥有路径不等于拥有内容。",
    }),
    observationStep(
      2,
      7,
      { ...s1, steps: 2 },
      "MaterialLoaded",
      'content: "项目背景与会议议题。"',
      {
        explanation: "工具返回真实内容后，系统才具备输出清单的条件。",
        invariant: "工具结果必须进入 Observation，再由 Reducer 更新 State。",
      },
    ),
    stateStep(3, 8, s2, ["materialText"], "S2", {
      question: "所有字段齐全后，还需要继续询问或读取吗？",
      explanation: "如果继续行动，就是多余调用。此时应完成目标。",
      invariant: "足够信息出现后必须停止探索。",
    }),
    predictStep(3, 9, s2, "下一动作是什么？", "ProduceChecklist", {
      explanation: "用这一问收束运行时决策：不同 State 对应不同 Action。",
      invariant: "Policy 必须区分信息获取与最终输出。",
    }),
    actionStep(3, 10, s2, "ProduceChecklist", "生成会议准备清单", {
      reason: "所有完成条件已满足。",
      explanation:
        "对比结果：naive Chatbot 自动猜日期；Workflow 终止；Agent 获取日期后继续。",
      invariant: "最终输出只使用 State 中已验证事实。",
    }),
    terminationStep(
      3,
      11,
      { ...s2, completed: true },
      "Completed",
      "日期由用户提供，材料由工具返回，任务完成",
      {
        explanation: "强调 Agent 的收益来自闭环获取信息，而不是语言更像人。",
        invariant: "完成原因必须可从 Trace 还原。",
      },
    ),
  ];
}

function buildReadFailureSteps() {
  const s0 = { ...baseState, materialPath: "missing.txt" };
  const s1 = {
    ...s0,
    materialPath: null,
    steps: 1,
  };
  const s2 = {
    ...s1,
    materialPath: "replacement.txt",
    steps: 2,
  };
  const s3 = {
    ...s2,
    materialText: "替代材料读取成功。",
    steps: 3,
  };

  return [
    stateStep(1, 0, s0, ["materialText"], "S0", {
      question: "有路径但没有内容，是否可以直接输出？",
      explanation: "路径 missing.txt 只是用户输入，尚未经过环境验证。",
      invariant: "未经执行的路径不能产生材料事实。",
    }),
    predictStep(
      1,
      1,
      s0,
      "当前有材料路径但没有内容，下一动作是什么？",
      "ReadMaterial",
      {
        explanation: "首先必须尝试真实读取，而不是假设文件有效。",
        invariant: "外部事实必须通过 Environment 获取。",
      },
    ),
    actionStep(1, 2, s0, "ReadMaterial", 'path: "missing.txt"', {
      reason: "State 需要材料内容。",
      explanation: "现在仍不能断言读取是否成功。",
      invariant: "Action 是请求，不是结果。",
    }),
    observationStep(
      1,
      3,
      { ...s0, steps: 1 },
      "MaterialReadFailed",
      'path: "missing.txt"',
      {
        explanation:
          "错误也是 Observation。它不能被吞掉，也不能被转换成假成功。",
        invariant: "失败必须作为结构化事实进入状态更新。",
      },
    ),
    stateStep(2, 4, s1, ["materialPath"], "S1", {
      question: "读取失败后，为什么 material_path 被清空？",
      explanation:
        "无效路径不应继续留在可执行状态，否则 Policy 会反复读取同一个错误路径。",
      invariant: "失败 Observation 必须消除导致重复失败的旧假设。",
    }),
    predictStep(
      2,
      5,
      s1,
      "无效路径已清除，下一动作是什么？",
      "AskForMaterialPath",
      {
        explanation: "系统没有权限凭空发明新路径，只能向用户请求。",
        invariant: "恢复策略也必须服从信息与权限边界。",
      },
    ),
    actionStep(2, 6, s1, "AskForMaterialPath", "请求新的材料路径", {
      reason: "material_path = None。",
      explanation: "这展示了失败反馈如何改变下一动作。",
      invariant: "同样的 Policy 在不同 State 上应产生不同动作。",
    }),
    observationStep(
      2,
      7,
      { ...s1, steps: 2 },
      "UserProvidedMaterialPath",
      '"replacement.txt"',
      {
        explanation: "新路径来自用户，但仍未证明文件内容可读取。",
        invariant: "路径更新与内容读取是两个不同状态转换。",
      },
    ),
    stateStep(3, 8, s2, ["materialPath"], "S2", {
      question: "拿到新路径后，可以直接完成吗？",
      explanation: "不能。material_text 仍为 None，需要再次执行读取动作。",
      invariant: "修复输入不等于动作已经成功。",
    }),
    predictStep(3, 9, s2, "下一动作是什么？", "ReadMaterial", {
      explanation: "让学生预测第二次 ReadMaterial，并比较两次调用参数。",
      invariant: "恢复后的动作必须基于新 State。",
    }),
    actionStep(3, 10, s2, "ReadMaterial", 'path: "replacement.txt"', {
      reason: "新路径存在，但内容仍未获得。",
      explanation: "注意这次读取使用 replacement.txt，而不是旧路径。",
      invariant: "失败后的重试不能重复使用已判定无效的参数。",
    }),
    observationStep(
      3,
      11,
      { ...s2, steps: 3 },
      "MaterialLoaded",
      'path: "replacement.txt"\ncontent: "替代材料读取成功。"',
      {
        explanation: "工具结果路径必须与当前请求路径匹配，否则应拒绝。",
        invariant: "Observation 必须与上一 Action 对应。",
      },
    ),
    stateStep(4, 12, s3, ["materialText"], "S3", {
      question: "恢复是否已经完成？",
      explanation: "材料内容现在存在，可以进入最终输出。",
      invariant: "恢复成功也要由 State 证明。",
    }),
    predictStep(4, 13, s3, "下一动作是什么？", "ProduceChecklist", {
      explanation: "此时继续读取会造成不必要调用。",
      invariant: "目标满足后应停止工具探索。",
    }),
    actionStep(4, 14, s3, "ProduceChecklist", "使用替代材料生成清单", {
      reason: "失败路径已恢复，所有完成条件满足。",
      explanation:
        "完整闭环是 ReadFailed → 清除旧路径 → 获取新路径 → 再读 → 完成。",
      invariant: "成功输出必须建立在恢复后的真实 Observation 上。",
    }),
    terminationStep(
      4,
      15,
      { ...s3, completed: true },
      "Completed",
      "读取失败已经恢复，任务完成",
      {
        explanation: "让学生沿底部 Trace 复述整条恢复路径。",
        invariant: "失败恢复过程必须可追踪、可解释。",
      },
    ),
  ];
}

function buildUserDeclinedSteps() {
  const s0 = { ...baseState, date: null };
  const s1 = {
    ...s0,
    userDeclined: true,
    steps: 1,
  };

  return [
    stateStep(1, 0, s0, ["date"], "S0", {
      question: "缺少日期时，系统应猜测还是询问？",
      explanation: "Agent 首先选择 AskForDate。",
      invariant: "缺失事实不能由 Policy 自行制造。",
    }),
    predictStep(1, 1, s0, "缺少日期时，下一动作是什么？", "AskForDate", {
      explanation: "先让学生预测正常信息获取动作。",
      invariant: "当前 State 决定当前问题。",
    }),
    actionStep(1, 2, s0, "AskForDate", "请求用户提供会议日期", {
      reason: "date = None。",
      explanation: "此时系统仍有继续任务的可能。",
      invariant: "询问动作不能被记录成已经获得日期。",
    }),
    observationStep(
      1,
      3,
      { ...s0, steps: 1 },
      "UserDeclined",
      "用户明确表示不再补充",
      {
        explanation:
          "UserDeclined 是环境事实。它不是异常，也不是“模型回答失败”。",
        invariant: "用户取消必须被尊重。",
      },
    ),
    stateStep(2, 4, s1, ["userDeclined"], "S1", {
      question: "用户拒绝后，继续追问、猜日期，还是停止？",
      explanation: "user_declined 变为 true，这是一个明确的终止触发条件。",
      invariant: "人工边界优先于继续自动化。",
    }),
    predictStep(2, 5, s1, "此时 Agent 应选择什么？", "Stop", {
      explanation: "让学生区分“任务未完成”和“系统行为错误”。",
      invariant: "无法继续不等于无限重试。",
    }),
    actionStep(2, 6, s1, "Stop", 'reason: "user_declined"', {
      reason: "用户明确取消信息补充。",
      explanation: "终止原因需要结构化记录，而不是默默退出循环。",
      invariant: "每次终止都必须有可解释原因。",
    }),
    terminationStep(
      2,
      7,
      s1,
      "Stopped",
      "user_declined",
      {
        explanation:
          "完成、用户取消和预算耗尽是不同终止语义。这里是正确停止，不是 Completed。",
        invariant: "Stop 与 Finish 不能混为一谈。",
      },
      "stopped",
    ),
  ];
}

function stateStep(cycle, index, state, changed, label, notes) {
  return {
    phase: "state",
    cycle,
    index,
    state,
    changed,
    label,
    title: "观察当前 State",
    timeline: { type: "state", label },
    ...notes,
  };
}

function predictStep(cycle, index, state, question, correct, notes) {
  return {
    phase: "predict",
    cycle,
    index,
    state,
    changed: [],
    title: "请预测下一动作",
    question,
    options: actionOptions,
    correct,
    timeline: { type: "predict", label: "学生预测" },
    ...notes,
  };
}

function actionStep(cycle, index, state, action, detail, notes) {
  return {
    phase: "action",
    cycle,
    index,
    state,
    changed: [],
    title: "Policy 选择 Action",
    action,
    detail,
    timeline: { type: "action", label: action },
    ...notes,
  };
}

function observationStep(
  cycle,
  index,
  state,
  observation,
  detail,
  notes,
) {
  return {
    phase: "observation",
    cycle,
    index,
    state,
    changed: [],
    title: "Environment 返回 Observation",
    observation,
    detail,
    timeline: { type: "observation", label: observation },
    ...notes,
  };
}

function terminationStep(
  cycle,
  index,
  state,
  termination,
  reason,
  notes,
  status = "completed",
) {
  return {
    phase: "termination",
    cycle,
    index,
    state,
    changed: status === "completed" ? ["completed"] : [],
    title: "运行终止",
    termination,
    reason,
    status,
    timeline: { type: "termination", label: termination },
    ...notes,
  };
}

const dom = {
  scenarioButtons: [...document.querySelectorAll(".scenario-button")],
  modeButtons: [...document.querySelectorAll(".mode-button")],
  processStages: [...document.querySelectorAll(".process-stage")],
  scenarioKicker: document.querySelector("#scenario-kicker"),
  scenarioTitle: document.querySelector("#scenario-title"),
  scenarioObjective: document.querySelector("#scenario-objective"),
  cycleLabel: document.querySelector("#cycle-label"),
  stepCounter: document.querySelector("#step-counter"),
  stateVersion: document.querySelector("#state-version"),
  stateTable: document.querySelector("#state-table"),
  stageKicker: document.querySelector("#stage-kicker"),
  stageTitle: document.querySelector("#stage-title"),
  phaseBadge: document.querySelector("#phase-badge"),
  stageContent: document.querySelector("#stage-content"),
  predictionFeedback: document.querySelector("#prediction-feedback"),
  teachingQuestion: document.querySelector("#teaching-question"),
  teachingExplanation: document.querySelector("#teaching-explanation"),
  invariantText: document.querySelector("#invariant-text"),
  codeMappingText: document.querySelector("#code-mapping-text"),
  timeline: document.querySelector("#timeline"),
  previousButton: document.querySelector("#previous-button"),
  nextButton: document.querySelector("#next-button"),
  resetButton: document.querySelector("#reset-button"),
  fullscreenButton: document.querySelector("#fullscreen-button"),
};

const appState = {
  scenarioId: "complete",
  stepIndex: 0,
  mode: "teacher",
  selectedChoice: null,
  predictionSubmitted: false,
};

function render() {
  const scenario = scenarioDefinitions[appState.scenarioId];
  const step = scenario.steps[appState.stepIndex];

  dom.scenarioButtons.forEach((button) => {
    button.classList.toggle(
      "is-active",
      button.dataset.scenario === appState.scenarioId,
    );
  });

  dom.modeButtons.forEach((button) => {
    button.classList.toggle("is-active", button.dataset.mode === appState.mode);
  });

  dom.scenarioKicker.textContent = scenario.kicker;
  dom.scenarioTitle.textContent = scenario.title;
  dom.scenarioObjective.textContent = scenario.objective;
  dom.cycleLabel.textContent = `第 ${step.cycle} 轮决策`;
  dom.stepCounter.textContent = `${appState.stepIndex + 1} / ${scenario.steps.length}`;

  renderProcess(step, scenario.steps);
  renderState(step);
  renderStage(step);
  renderExplanation(step);
  renderTimeline(scenario.steps);
  renderControls(step, scenario.steps.length);
}

function renderProcess(step, allSteps) {
  const cycleSteps = allSteps.filter((item) => item.cycle === step.cycle);
  const currentCyclePosition = cycleSteps.findIndex(
    (item) => item.index === step.index,
  );
  const completedPhases = new Set(
    cycleSteps
      .slice(0, currentCyclePosition)
      .map((item) => item.phase),
  );

  dom.processStages.forEach((stage) => {
    const phase = stage.dataset.phase;
    stage.classList.toggle("is-active", phase === step.phase);
    stage.classList.toggle(
      "is-complete",
      completedPhases.has(phase) && phase !== step.phase,
    );
  });
}

function renderState(step) {
  dom.stateVersion.textContent =
    step.label || `S${Math.max(0, step.cycle - 1)}`;
  dom.stateTable.replaceChildren();

  const fields = [
    ["topic", step.state.topic],
    ["date", step.state.date],
    ["participants", step.state.participants],
    ["material_path", step.state.materialPath],
    ["material_text", step.state.materialText],
    ["user_declined", step.state.userDeclined],
    ["steps", step.state.steps],
    ["completed", step.state.completed],
  ];

  fields.forEach(([key, value]) => {
    const row = document.createElement("dl");
    row.className = "state-row";
    if (step.changed?.includes(fieldAlias(key))) {
      row.classList.add("is-changed");
    } else if (
      value === null ||
      value === "" ||
      (key === "participants" && !value)
    ) {
      row.classList.add("is-focus");
    }
    if (step.observation === "MaterialReadFailed" && key === "material_path") {
      row.classList.add("is-error");
    }

    const term = document.createElement("dt");
    term.textContent = key;
    const description = document.createElement("dd");
    description.textContent = formatStateValue(value);
    description.title = description.textContent;
    row.append(term, description);
    dom.stateTable.append(row);
  });
}

function renderStage(step) {
  const phaseNames = {
    state: "STATE",
    predict: "PREDICT",
    action: "ACTION",
    observation: "OBSERVATION",
    termination: "TERMINATION",
  };
  dom.stageKicker.textContent = phaseNames[step.phase];
  dom.stageTitle.textContent = step.title;
  dom.phaseBadge.textContent = `Cycle ${step.cycle}`;
  dom.stageContent.replaceChildren();
  dom.predictionFeedback.hidden = true;
  dom.predictionFeedback.className = "prediction-feedback";

  if (step.phase === "state") {
    dom.stageContent.append(
      paragraph(step.question, "lead-question"),
      paragraph(
        "先只观察左侧 State。不要根据场景标题猜答案，而要指出具体缺失或刚变化的字段。",
        "supporting-copy",
      ),
    );
    return;
  }

  if (step.phase === "predict") {
    dom.stageContent.append(paragraph(step.question, "lead-question"));
    const list = document.createElement("div");
    list.className = "choice-list";

    step.options.forEach((option) => {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "choice-button";
      button.textContent = option;
      button.dataset.choice = option;
      button.classList.toggle("is-selected", appState.selectedChoice === option);

      if (appState.predictionSubmitted) {
        button.disabled = true;
        button.classList.toggle("is-correct", option === step.correct);
        button.classList.toggle(
          "is-wrong",
          option === appState.selectedChoice && option !== step.correct,
        );
      }

      button.addEventListener("click", () => {
        if (appState.predictionSubmitted) return;
        appState.selectedChoice = option;
        render();
      });
      list.append(button);
    });
    dom.stageContent.append(list);

    if (appState.predictionSubmitted) {
      const correct = appState.selectedChoice === step.correct;
      dom.predictionFeedback.hidden = false;
      dom.predictionFeedback.classList.add(
        correct ? "is-correct" : "is-wrong",
      );
      dom.predictionFeedback.textContent = correct
        ? `预测正确：${step.correct}`
        : `当前选择是 ${appState.selectedChoice}；正确动作是 ${step.correct}。请结合 State 解释差异。`;
    }
    return;
  }

  if (step.phase === "action") {
    const card = document.createElement("div");
    card.className = "action-card";
    card.append(
      span("POLICY 选择", "card-label"),
      paragraph(
        `${step.action}${step.detail ? ` {\n  ${step.detail}\n}` : ""}`,
        "code-value",
      ),
    );
    dom.stageContent.append(card, reasonBox(step.reason));
    return;
  }

  if (step.phase === "observation") {
    const card = document.createElement("div");
    card.className = "observation-card";
    card.append(
      span("ENVIRONMENT 返回", "card-label"),
      paragraph(
        `${step.observation}${step.detail ? ` {\n  ${step.detail.replaceAll("\n", "\n  ")}\n}` : ""}`,
        "code-value",
      ),
    );
    dom.stageContent.append(
      card,
      reasonBox("下一步不是直接输出，而是先把 Observation 交给 Reducer 更新 State。"),
    );
    return;
  }

  if (step.phase === "termination") {
    const card = document.createElement("div");
    card.className = `termination-card is-${step.status}`;
    card.append(
      span("RUN STATUS", "card-label"),
      paragraph(step.termination, "code-value"),
      reasonBox(`reason: ${step.reason}`),
    );
    dom.stageContent.append(card);
  }
}

function renderExplanation(step) {
  dom.teachingQuestion.textContent =
    step.question || explanationQuestionFor(step);
  dom.teachingExplanation.textContent = step.explanation;
  dom.invariantText.textContent = step.invariant;
  dom.codeMappingText.textContent = codeMappingFor(step.phase);
}

function renderTimeline(allSteps) {
  dom.timeline.replaceChildren();

  allSteps.slice(0, appState.stepIndex + 1).forEach((step, index) => {
    const item = document.createElement("div");
    item.className = `timeline-item is-${step.timeline.type}`;
    item.classList.toggle("is-current", index === appState.stepIndex);
    item.append(
      span(`C${step.cycle} · ${step.timeline.type}`, "timeline-type"),
      strong(step.timeline.label),
    );
    dom.timeline.append(item);
  });

  requestAnimationFrame(() => {
    dom.timeline.scrollLeft = dom.timeline.scrollWidth;
  });
}

function renderControls(step, totalSteps) {
  dom.previousButton.disabled = appState.stepIndex === 0;
  const last = appState.stepIndex === totalSteps - 1;

  if (
    step.phase === "predict" &&
    appState.mode === "student" &&
    !appState.predictionSubmitted
  ) {
    dom.nextButton.textContent = "提交预测";
    dom.nextButton.disabled = appState.selectedChoice === null;
  } else if (last) {
    dom.nextButton.textContent = "重新开始";
    dom.nextButton.disabled = false;
  } else {
    dom.nextButton.textContent =
      step.phase === "predict" ? "揭晓 Policy 选择" : "下一步";
    dom.nextButton.disabled = false;
  }
}

function nextStep() {
  const scenario = scenarioDefinitions[appState.scenarioId];
  const step = scenario.steps[appState.stepIndex];

  if (
    step.phase === "predict" &&
    appState.mode === "student" &&
    !appState.predictionSubmitted
  ) {
    if (!appState.selectedChoice) return;
    appState.predictionSubmitted = true;
    render();
    return;
  }

  if (appState.stepIndex === scenario.steps.length - 1) {
    resetScenario();
    return;
  }

  appState.stepIndex += 1;
  clearPrediction();
  render();
}

function previousStep() {
  if (appState.stepIndex === 0) return;
  appState.stepIndex -= 1;
  clearPrediction();
  render();
}

function selectScenario(id) {
  appState.scenarioId = id;
  appState.stepIndex = 0;
  clearPrediction();
  render();
}

function resetScenario() {
  appState.stepIndex = 0;
  clearPrediction();
  render();
}

function clearPrediction() {
  appState.selectedChoice = null;
  appState.predictionSubmitted = false;
}

function setMode(mode) {
  appState.mode = mode;
  clearPrediction();
  render();
}

function explanationQuestionFor(step) {
  if (step.phase === "action") return `为什么选择 ${step.action}？`;
  if (step.phase === "observation")
    return `${step.observation} 能证明什么，不能证明什么？`;
  if (step.phase === "termination")
    return `为什么这里是 ${step.termination}？`;
  return "当前最值得学生注意的状态变化是什么？";
}

function codeMappingFor(phase) {
  const mappings = {
    state: "MeetingState 负责保存当前事实；它不主动执行外部动作。",
    predict: "MeetingState::decide(&self, max_steps) 是确定性 Policy。",
    action: "MeetingAction 表达系统意图；此时环境事实尚未改变。",
    observation:
      "MeetingObservation 由用户或工具返回；MeetingState::apply 负责状态更新。",
    termination:
      "ProduceChecklist 与 Stop 是不同终止语义；completed 只用于正常完成。",
  };
  return mappings[phase];
}

function fieldAlias(key) {
  const aliases = {
    material_path: "materialPath",
    material_text: "materialText",
    user_declined: "userDeclined",
  };
  return aliases[key] || key;
}

function formatStateValue(value) {
  if (value === null || value === "") return "None";
  if (typeof value === "boolean") return String(value);
  if (typeof value === "number") return String(value);
  return `Some(${value})`;
}

function paragraph(text, className) {
  const element = document.createElement("p");
  element.className = className;
  element.textContent = text;
  return element;
}

function span(text, className) {
  const element = document.createElement("span");
  element.className = className;
  element.textContent = text;
  return element;
}

function strong(text) {
  const element = document.createElement("strong");
  element.textContent = text;
  return element;
}

function reasonBox(text) {
  return paragraph(text, "reason-box");
}

dom.scenarioButtons.forEach((button) => {
  button.addEventListener("click", () => selectScenario(button.dataset.scenario));
});

dom.modeButtons.forEach((button) => {
  button.addEventListener("click", () => setMode(button.dataset.mode));
});

dom.nextButton.addEventListener("click", nextStep);
dom.previousButton.addEventListener("click", previousStep);
dom.resetButton.addEventListener("click", resetScenario);
dom.fullscreenButton.addEventListener("click", async () => {
  if (!document.fullscreenElement) {
    await document.documentElement.requestFullscreen?.();
  } else {
    await document.exitFullscreen?.();
  }
});

document.addEventListener("keydown", (event) => {
  const target = event.target;
  if (target instanceof HTMLButtonElement && (event.key === " " || event.key === "Enter")) {
    return;
  }

  if (event.key === "ArrowRight" || event.key === " ") {
    event.preventDefault();
    nextStep();
  } else if (event.key === "ArrowLeft") {
    previousStep();
  } else if (event.key.toLowerCase() === "r") {
    resetScenario();
  } else if (["1", "2", "3", "4"].includes(event.key)) {
    const ids = ["complete", "missingDate", "readFailure", "userDeclined"];
    selectScenario(ids[Number(event.key) - 1]);
  }
});

render();
