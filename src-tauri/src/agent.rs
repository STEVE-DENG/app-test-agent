use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use chrono::Utc;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter};

use crate::device;
use crate::knowledge::KnowledgeStore;
use crate::models::{
    AgentEvent, AgentRunResult, ModelDecision, ModelTraceRecord, Observation, RawModelDecision,
    SkillHint, StoredSettings, ToolRecord, UiNode,
};
use crate::storage::AppStore;

const HIGH_RISK_TERMS: &[&str] = &[
    "支付",
    "付款",
    "下单",
    "购买",
    "转账",
    "删除",
    "发送",
    "安装",
    "卸载",
    "充值",
    "提交订单",
    "注销",
    "修改密码",
];

const ALLOWED_TOOLS: &[&str] = &[
    "observe_screen",
    "tap",
    "tap_text",
    "tap_id",
    "long_press",
    "swipe",
    "input_text",
    "press_key",
    "start_app",
    "wait",
    "ask_user",
    "finish",
];

const MODEL_NODE_LIMIT: usize = 56;
const HISTORY_LIMIT: usize = 6;

const CONNECTION_TEST_PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";

pub struct AgentDeps {
    pub app: Option<AppHandle>,
    pub store: Arc<AppStore>,
    pub knowledge: Arc<KnowledgeStore>,
    pub settings: StoredSettings,
    pub artifacts_dir: PathBuf,
    pub cancellation: Arc<AtomicBool>,
}

struct ModelCallResult {
    decision: ModelDecision,
    trace: ModelTraceRecord,
}

pub async fn run_agent(
    deps: AgentDeps,
    run_id: String,
    instruction: String,
    device_serial: String,
    allow_high_risk: bool,
) -> Result<AgentRunResult, String> {
    let instruction = instruction.trim().to_string();
    if instruction.is_empty() {
        return Err("任务描述不能为空".to_string());
    }
    if deps.settings.api_key.trim().is_empty() {
        return Err("请先在设置中配置多模态模型 API Key".to_string());
    }
    if deps.settings.model.trim().is_empty() {
        return Err("请先在设置中配置模型名称".to_string());
    }
    if !allow_high_risk && contains_high_risk(&instruction) {
        return Err(
            "任务包含支付、发送、删除、安装等高风险动作。请确认允许高风险操作后再执行。"
                .to_string(),
        );
    }

    device::ensure_device_ready(&device_serial).await?;

    deps.store
        .begin_run(&run_id, &instruction, &device_serial, &deps.settings.model)?;
    emit(
        deps.app.as_ref(),
        &run_id,
        "run_started",
        "开始执行",
        &instruction,
        0,
        None,
    );

    match execute_run(&deps, &run_id, &instruction, &device_serial).await {
        Ok(result) => Ok(result),
        Err(error) => {
            let _ = deps.store.finish_run(&run_id, "failed", &error, 0, None);
            emit(
                deps.app.as_ref(),
                &run_id,
                "run_failed",
                "任务失败",
                &error,
                0,
                None,
            );
            Err(error)
        }
    }
}

pub async fn test_model_connection(settings: &StoredSettings) -> Result<String, String> {
    if settings.api_key.trim().is_empty() {
        return Err("请先填写模型 API Key".to_string());
    }
    if settings.model.trim().is_empty() {
        return Err("请先填写模型名称".to_string());
    }

    let content = if settings.use_vision {
        json!([
            {
                "type": "text",
                "text": "这是一次连接检查。如果你能读取这张极小图片，请只回复 OK。"
            },
            {
                "type": "image_url",
                "image_url": { "url": CONNECTION_TEST_PNG }
            }
        ])
    } else {
        Value::String("这是一次连接检查，请只回复 OK。".to_string())
    };
    let body = json!({
        "model": settings.model,
        "temperature": 0,
        "max_tokens": 32,
        "messages": [
            {
                "role": "system",
                "content": "你是模型连通性检查器。只回复 OK。"
            },
            {
                "role": "user",
                "content": content
            }
        ]
    });
    let started = Instant::now();
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| format!("创建模型 HTTP 客户端失败: {error}"))?
        .post(chat_endpoint(&settings.base_url))
        .bearer_auth(&settings.api_key)
        .json(&body)
        .send()
        .await
        .map_err(|error| format!("连接模型服务失败: {error}"))?;
    let status = response.status();
    let raw = response
        .text()
        .await
        .map_err(|error| format!("读取模型响应失败: {error}"))?;
    if !status.is_success() {
        return Err(format!(
            "模型服务返回 HTTP {}: {}",
            status.as_u16(),
            truncate(&raw, 500)
        ));
    }
    let parsed: Value =
        serde_json::from_str(&raw).map_err(|error| format!("解析模型响应失败: {error}"))?;
    let reply = extract_message_content(&parsed)?;
    let mode = if settings.use_vision {
        "视觉请求已接受"
    } else {
        "文本请求已接受"
    };
    Ok(format!(
        "连接成功 · {} · {} ms · {} · 回复：{}",
        settings.model,
        started.elapsed().as_millis(),
        mode,
        truncate(reply.trim(), 40)
    ))
}

async fn execute_run(
    deps: &AgentDeps,
    run_id: &str,
    instruction: &str,
    device_serial: &str,
) -> Result<AgentRunResult, String> {
    let observation_started = Instant::now();
    let mut observation = device::capture_observation(
        device_serial,
        &deps.artifacts_dir,
        run_id,
        "step-00-initial",
    )
    .await?;
    let _initial_observation_ms = observation_started.elapsed().as_millis() as u64;
    if let Err(error) = deps.knowledge.register_observation_mut(&mut observation) {
        emit(
            deps.app.as_ref(),
            run_id,
            "knowledge_warning",
            "页面知识写入失败",
            &error,
            0,
            None,
        );
    }
    emit_observation(deps, run_id, 0, &observation);

    let skills = deps
        .store
        .retrieve_skills(instruction, &observation.package, 2)?;
    if !skills.is_empty() {
        let skill_ids = skills
            .iter()
            .map(|skill| skill.id.clone())
            .collect::<Vec<_>>();
        deps.store.mark_skills_used(&skill_ids)?;
        emit(
            deps.app.as_ref(),
            run_id,
            "skill_recall",
            "召回历史技能",
            &skills
                .iter()
                .map(|skill| skill.name.clone())
                .collect::<Vec<_>>()
                .join(" / "),
            0,
            Some(json!({ "skills": skills })),
        );
    }

    let mut records = Vec::<ToolRecord>::new();
    let mut history = Vec::<String>::new();
    let mut last_error: Option<String> = None;
    let mut next_step_index = 1_u32;

    if let Some(skill) = skills.first() {
        let plan = skill.plan.iter().take(8).collect::<Vec<_>>();
        if !plan.is_empty() {
            emit(
                deps.app.as_ref(),
                run_id,
                "skill_fast_path",
                "优先回放已验证 Skill",
                &format!("{} · {} steps", skill.name, plan.len()),
                next_step_index - 1,
                Some(json!({ "skillId": skill.id, "steps": plan.len() })),
            );
            for step in plan {
                if deps.cancellation.load(Ordering::Relaxed) {
                    break;
                }
                let step_index = next_step_index;
                let decision = ModelDecision {
                    thought: format!("复用已验证 Skill：{}", step.note),
                    action: step.tool.clone(),
                    args: step.args.clone(),
                    summary: None,
                };
                let before = observation.clone();
                let tool_started = Instant::now();
                let result = match execute_tool(device_serial, &decision, &before).await {
                    Ok(result) => result,
                    Err(error) => {
                        let tool_ms = tool_started.elapsed().as_millis() as u64;
                        let record = ToolRecord {
                            step_index,
                            thought: decision.thought.clone(),
                            tool: decision.action.clone(),
                            args: decision.args.clone(),
                            result: json!({ "error": error.clone() }),
                            page_signature_before: before.page_signature.clone(),
                            page_signature_after: before.page_signature.clone(),
                            screenshot_path: before.screenshot_path.clone(),
                            screenshot_after_path: before.screenshot_path.clone(),
                            page_id_before: before.page_id.clone(),
                            page_id_after: before.page_id.clone(),
                            created_at: Utc::now().to_rfc3339(),
                        };
                        deps.store.save_step_for_run(run_id, &record)?;
                        deps.store
                            .save_step_metric(run_id, step_index, 0, tool_ms, 0, 0, tool_ms)?;
                        records.push(record);
                        history.push(format!(
                            "#{step_index} skill {} {} -> ERROR: {}",
                            decision.action, decision.args, error
                        ));
                        last_error = Some(format!("Skill 回放失败，改用模型恢复：{error}"));
                        emit(
                            deps.app.as_ref(),
                            run_id,
                            "skill_fast_path_failed",
                            "Skill 回放失败",
                            &error,
                            step_index,
                            Some(json!({ "skillId": skill.id, "tool": decision.action })),
                        );
                        next_step_index = step_index + 1;
                        break;
                    }
                };
                let tool_ms = tool_started.elapsed().as_millis() as u64;
                let observation_started = Instant::now();
                let mut after = device::capture_observation(
                    device_serial,
                    &deps.artifacts_dir,
                    run_id,
                    &format!("skill-{step_index:02}-after"),
                )
                .await
                .unwrap_or_else(|_| before.clone());
                let _ = deps.knowledge.register_observation_mut(&mut after);
                let observation_ms = observation_started.elapsed().as_millis() as u64;
                let knowledge_started = Instant::now();
                let _ = deps.knowledge.record_action(
                    run_id,
                    step_index,
                    &before,
                    &after,
                    &decision.action,
                    &decision.args,
                    &result,
                    true,
                );
                let knowledge_ms = knowledge_started.elapsed().as_millis() as u64;
                let record = ToolRecord {
                    step_index,
                    thought: decision.thought.clone(),
                    tool: decision.action.clone(),
                    args: decision.args.clone(),
                    result: result.clone(),
                    page_signature_before: before.page_signature.clone(),
                    page_signature_after: after.page_signature.clone(),
                    screenshot_path: before.screenshot_path.clone(),
                    screenshot_after_path: after.screenshot_path.clone(),
                    page_id_before: before.page_id.clone(),
                    page_id_after: after.page_id.clone(),
                    created_at: Utc::now().to_rfc3339(),
                };
                deps.store.save_step_for_run(run_id, &record)?;
                deps.store.save_step_metric(
                    run_id,
                    step_index,
                    0,
                    tool_ms,
                    observation_ms,
                    knowledge_ms,
                    tool_ms + observation_ms + knowledge_ms,
                )?;
                records.push(record);
                history.push(format!(
                    "#{step_index} skill {} {} -> {}",
                    decision.action, decision.args, result
                ));
                next_step_index = step_index + 1;
                observation = after;
            }
        }
    }

    for step_index in next_step_index..=deps.settings.max_steps {
        let step_started = Instant::now();
        if deps.cancellation.load(Ordering::Relaxed) {
            let summary = "任务已由用户停止";
            let skill_ids = skills
                .iter()
                .map(|skill| skill.id.clone())
                .collect::<Vec<_>>();
            deps.store.record_skill_outcome(&skill_ids, false)?;
            deps.store
                .finish_run(run_id, "cancelled", summary, step_index - 1, None)?;
            emit(
                deps.app.as_ref(),
                run_id,
                "run_cancelled",
                "任务已停止",
                summary,
                step_index - 1,
                None,
            );
            return Ok(AgentRunResult {
                run_id: run_id.to_string(),
                status: "cancelled".to_string(),
                summary: summary.to_string(),
                steps: step_index - 1,
                skill_candidate_id: None,
            });
        }

        let model_started = Instant::now();
        let model_call = match ask_model(
            &deps.settings,
            run_id,
            instruction,
            device_serial,
            step_index,
            &observation,
            &history,
            &skills,
            last_error.as_deref(),
        )
        .await
        {
            Ok(decision) => decision,
            Err(error) => {
                let skill_ids = skills
                    .iter()
                    .map(|skill| skill.id.clone())
                    .collect::<Vec<_>>();
                deps.store.record_skill_outcome(&skill_ids, false)?;
                deps.store
                    .finish_run(run_id, "failed", &error, step_index - 1, None)?;
                emit(
                    deps.app.as_ref(),
                    run_id,
                    "run_failed",
                    "模型决策失败",
                    &error,
                    step_index - 1,
                    None,
                );
                return Err(error);
            }
        };
        let model_ms = model_started.elapsed().as_millis() as u64;
        let decision = model_call.decision;
        let mut trace = model_call.trace;
        trace.duration_ms = model_ms;
        if let Err(error) = deps.store.save_model_trace(&trace) {
            emit(
                deps.app.as_ref(),
                run_id,
                "model_trace_warning",
                "模型轨迹写入失败",
                &error,
                step_index,
                None,
            );
        }
        last_error = None;

        emit(
            deps.app.as_ref(),
            run_id,
            "thought",
            "Agent 判断",
            &decision.thought,
            step_index,
            Some(json!({
                "action": decision.action,
                "args": decision.args,
                "modelTrace": {
                    "attemptCount": trace.attempt_count,
                    "promptChars": trace.prompt_chars,
                    "nodeCount": trace.node_count,
                    "useVision": trace.use_vision,
                    "durationMs": trace.duration_ms,
                    "usage": trace.usage
                }
            })),
        );

        if decision.action == "finish" {
            let success = decision
                .args
                .get("success")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let summary = decision
                .summary
                .or_else(|| {
                    decision
                        .args
                        .get("summary")
                        .and_then(Value::as_str)
                        .map(ToString::to_string)
                })
                .unwrap_or_else(|| {
                    if success {
                        "任务已完成".to_string()
                    } else {
                        "任务未能完成".to_string()
                    }
                });
            let finish_record = ToolRecord {
                step_index,
                thought: decision.thought.clone(),
                tool: "finish".to_string(),
                args: decision.args.clone(),
                result: json!({ "success": success, "message": summary }),
                page_signature_before: observation.page_signature.clone(),
                page_signature_after: observation.page_signature.clone(),
                screenshot_path: observation.screenshot_path.clone(),
                screenshot_after_path: observation.screenshot_path.clone(),
                page_id_before: observation.page_id.clone(),
                page_id_after: observation.page_id.clone(),
                created_at: Utc::now().to_rfc3339(),
            };
            deps.store.save_step_for_run(run_id, &finish_record)?;
            records.push(finish_record);
            deps.store.save_step_metric(
                run_id,
                step_index,
                model_ms,
                0,
                0,
                0,
                step_started.elapsed().as_millis() as u64,
            )?;

            let skill_ids = skills
                .iter()
                .map(|skill| skill.id.clone())
                .collect::<Vec<_>>();
            deps.store.record_skill_outcome(&skill_ids, success)?;

            let skill_candidate_id = if success && !deps.store.skill_exists(run_id)? {
                deps.store
                    .create_skill_candidate(run_id, instruction, &observation.package, &records)?
                    .map(|skill| skill.id)
            } else {
                None
            };

            let status = if success { "completed" } else { "failed" };
            deps.store.finish_run(
                run_id,
                status,
                &summary,
                step_index,
                skill_candidate_id.as_deref(),
            )?;
            emit(
                deps.app.as_ref(),
                run_id,
                if success {
                    "run_completed"
                } else {
                    "run_failed"
                },
                if success {
                    "任务完成"
                } else {
                    "任务未完成"
                },
                &summary,
                step_index,
                Some(json!({ "skillCandidateId": skill_candidate_id })),
            );
            return Ok(AgentRunResult {
                run_id: run_id.to_string(),
                status: status.to_string(),
                summary,
                steps: step_index,
                skill_candidate_id,
            });
        }

        if decision.action == "ask_user" {
            let question = decision
                .args
                .get("question")
                .and_then(Value::as_str)
                .or(decision.summary.as_deref())
                .unwrap_or("请补充任务所需信息");
            let ask_record = ToolRecord {
                step_index,
                thought: decision.thought.clone(),
                tool: "ask_user".to_string(),
                args: decision.args.clone(),
                result: json!({ "message": question }),
                page_signature_before: observation.page_signature.clone(),
                page_signature_after: observation.page_signature.clone(),
                screenshot_path: observation.screenshot_path.clone(),
                screenshot_after_path: observation.screenshot_path.clone(),
                page_id_before: observation.page_id.clone(),
                page_id_after: observation.page_id.clone(),
                created_at: Utc::now().to_rfc3339(),
            };
            deps.store.save_step_for_run(run_id, &ask_record)?;
            records.push(ask_record);
            deps.store.save_step_metric(
                run_id,
                step_index,
                model_ms,
                0,
                0,
                0,
                step_started.elapsed().as_millis() as u64,
            )?;
            let skill_ids = skills
                .iter()
                .map(|skill| skill.id.clone())
                .collect::<Vec<_>>();
            deps.store.record_skill_outcome(&skill_ids, false)?;
            deps.store
                .finish_run(run_id, "needs_input", question, step_index, None)?;
            emit(
                deps.app.as_ref(),
                run_id,
                "needs_input",
                "需要你的补充",
                question,
                step_index,
                None,
            );
            return Ok(AgentRunResult {
                run_id: run_id.to_string(),
                status: "needs_input".to_string(),
                summary: question.to_string(),
                steps: step_index,
                skill_candidate_id: None,
            });
        }

        let before_signature = observation.page_signature.clone();
        let screenshot_path = observation.screenshot_path.clone();
        let tool_started = Instant::now();
        let tool_result = match execute_tool(device_serial, &decision, &observation).await {
            Ok(result) => result,
            Err(error) => {
                let tool_ms = tool_started.elapsed().as_millis() as u64;
                let result = json!({ "error": error.clone() });
                let record = ToolRecord {
                    step_index,
                    thought: decision.thought.clone(),
                    tool: decision.action.clone(),
                    args: decision.args.clone(),
                    result: result.clone(),
                    page_signature_before: before_signature.clone(),
                    page_signature_after: before_signature.clone(),
                    screenshot_path: screenshot_path.clone(),
                    screenshot_after_path: screenshot_path.clone(),
                    page_id_before: observation.page_id.clone(),
                    page_id_after: observation.page_id.clone(),
                    created_at: Utc::now().to_rfc3339(),
                };
                deps.store.save_step_for_run(run_id, &record)?;
                deps.store.save_step_metric(
                    run_id,
                    step_index,
                    model_ms,
                    tool_ms,
                    0,
                    0,
                    step_started.elapsed().as_millis() as u64,
                )?;
                records.push(record);
                history.push(format!(
                    "#{step_index} {} {} -> ERROR: {}",
                    decision.action, decision.args, error
                ));
                last_error = Some(error.clone());
                emit(
                    deps.app.as_ref(),
                    run_id,
                    "tool_error",
                    &format!("工具失败 · {}", decision.action),
                    &error,
                    step_index,
                    Some(json!({
                        "tool": decision.action,
                        "args": decision.args,
                        "error": error
                    })),
                );
                continue;
            }
        };
        let tool_ms = tool_started.elapsed().as_millis() as u64;

        let observation_started = Instant::now();
        let mut after_observation = device::capture_observation(
            device_serial,
            &deps.artifacts_dir,
            run_id,
            &format!("step-{step_index:02}-after"),
        )
        .await
        .unwrap_or_else(|_| observation.clone());
        if let Err(error) = deps
            .knowledge
            .register_observation_mut(&mut after_observation)
        {
            emit(
                deps.app.as_ref(),
                run_id,
                "knowledge_warning",
                "页面知识写入失败",
                &error,
                step_index,
                None,
            );
        }
        let observation_ms = observation_started.elapsed().as_millis() as u64;
        let knowledge_started = Instant::now();
        if let Err(error) = deps.knowledge.record_action(
            run_id,
            step_index,
            &observation,
            &after_observation,
            &decision.action,
            &decision.args,
            &tool_result,
            true,
        ) {
            emit(
                deps.app.as_ref(),
                run_id,
                "knowledge_warning",
                "页面关系写入失败",
                &error,
                step_index,
                None,
            );
        }
        let knowledge_ms = knowledge_started.elapsed().as_millis() as u64;

        let record = ToolRecord {
            step_index,
            thought: decision.thought.clone(),
            tool: decision.action.clone(),
            args: decision.args.clone(),
            result: tool_result.clone(),
            page_signature_before: before_signature,
            page_signature_after: after_observation.page_signature.clone(),
            screenshot_path,
            screenshot_after_path: after_observation.screenshot_path.clone(),
            page_id_before: observation.page_id.clone(),
            page_id_after: after_observation.page_id.clone(),
            created_at: Utc::now().to_rfc3339(),
        };
        deps.store.save_step_for_run(run_id, &record)?;
        deps.store.save_step_metric(
            run_id,
            step_index,
            model_ms,
            tool_ms,
            observation_ms,
            knowledge_ms,
            step_started.elapsed().as_millis() as u64,
        )?;
        records.push(record);
        history.push(format!(
            "#{step_index} {} {} -> {}",
            decision.action, decision.args, tool_result
        ));
        emit_observation(deps, run_id, step_index, &after_observation);
        emit(
            deps.app.as_ref(),
            run_id,
            "tool_result",
            &format!("执行 · {}", decision.action),
            &result_summary(&tool_result),
            step_index,
            Some(json!({
                "tool": decision.action,
                "args": decision.args,
                "result": tool_result,
                "pageSignature": after_observation.page_signature
            })),
        );
        observation = after_observation;
    }

    let summary = format!(
        "达到最大步数 {}，任务未完成。Agent 未能给出可靠结束条件。",
        deps.settings.max_steps
    );
    let skill_ids = skills
        .iter()
        .map(|skill| skill.id.clone())
        .collect::<Vec<_>>();
    deps.store.record_skill_outcome(&skill_ids, false)?;
    deps.store
        .finish_run(run_id, "failed", &summary, deps.settings.max_steps, None)?;
    emit(
        deps.app.as_ref(),
        run_id,
        "run_failed",
        "达到最大步数",
        &summary,
        deps.settings.max_steps,
        None,
    );
    Ok(AgentRunResult {
        run_id: run_id.to_string(),
        status: "failed".to_string(),
        summary,
        steps: deps.settings.max_steps,
        skill_candidate_id: None,
    })
}

async fn execute_tool(
    serial: &str,
    decision: &ModelDecision,
    observation: &Observation,
) -> Result<Value, String> {
    let args = &decision.args;
    match decision.action.as_str() {
        "observe_screen" => Ok(json!({ "message": "已重新观察当前屏幕" })),
        "tap" => {
            let x = required_i32(args, "x")?;
            let y = required_i32(args, "y")?;
            let message = device::tap(serial, x, y).await?;
            Ok(json!({ "message": message, "x": x, "y": y }))
        }
        "tap_text" => {
            let text = required_str(args, "text")?;
            let (x, y, matched_text, resource_id, bounds) =
                if let Some(node) = device::find_node_by_text(&observation.nodes, text) {
                    (
                        node.bounds.center().0,
                        node.bounds.center().1,
                        node.text.clone(),
                        node.resource_id.clone(),
                        node.bounds,
                    )
                } else {
                    let nodes = device::dump_ui_nodes(serial).await?;
                    let node = device::find_node_by_text(&nodes, text)
                        .ok_or_else(|| format!("当前页面找不到可点击文本: {text}"))?;
                    (
                        node.bounds.center().0,
                        node.bounds.center().1,
                        node.text.clone(),
                        node.resource_id.clone(),
                        node.bounds,
                    )
                };
            let message = device::tap(serial, x, y).await?;
            Ok(json!({
                "message": message,
                "matchedText": matched_text,
                "resourceId": resource_id,
                "bounds": bounds
            }))
        }
        "tap_id" => {
            let resource_id = required_str(args, "resource_id")?;
            let (x, y, matched_text, matched_id, bounds) =
                if let Some(node) = device::find_node_by_id(&observation.nodes, resource_id) {
                    (
                        node.bounds.center().0,
                        node.bounds.center().1,
                        node.text.clone(),
                        node.resource_id.clone(),
                        node.bounds,
                    )
                } else {
                    let nodes = device::dump_ui_nodes(serial).await?;
                    let node = device::find_node_by_id(&nodes, resource_id)
                        .ok_or_else(|| format!("当前页面找不到元素 ID: {resource_id}"))?;
                    (
                        node.bounds.center().0,
                        node.bounds.center().1,
                        node.text.clone(),
                        node.resource_id.clone(),
                        node.bounds,
                    )
                };
            let message = device::tap(serial, x, y).await?;
            Ok(json!({
                "message": message,
                "matchedText": matched_text,
                "resourceId": matched_id,
                "bounds": bounds
            }))
        }
        "long_press" => {
            let duration = args
                .get("duration_ms")
                .and_then(Value::as_u64)
                .unwrap_or(800);
            let (x, y) = resolve_target(serial, args, &observation.nodes).await?;
            let message = device::long_press(serial, x, y, duration).await?;
            Ok(json!({ "message": message, "x": x, "y": y }))
        }
        "swipe" => {
            let direction = required_str(args, "direction")?.to_lowercase();
            let distance = args
                .get("distance")
                .and_then(Value::as_f64)
                .unwrap_or(0.55)
                .clamp(0.2, 0.8);
            let duration = args
                .get("duration_ms")
                .and_then(Value::as_u64)
                .unwrap_or(320);
            let (width, height) = if observation.width > 0 && observation.height > 0 {
                (observation.width, observation.height)
            } else {
                let nodes = device::dump_ui_nodes(serial).await?;
                device::content_viewport(&nodes)
            };
            if width <= 0 || height <= 0 {
                return Err("无法获取屏幕尺寸，滑动失败".to_string());
            }
            let (start, end) = swipe_points(&direction, width, height, distance)?;
            let message = device::swipe(serial, start, end, duration).await?;
            Ok(json!({ "message": message, "direction": direction }))
        }
        "input_text" => {
            let text = required_str(args, "text")?;
            let clear = args.get("clear").and_then(Value::as_bool).unwrap_or(false);
            let message = device::input_text(serial, text, clear).await?;
            Ok(json!({ "message": message, "text": text }))
        }
        "press_key" => {
            let key = required_str(args, "key")?;
            let message = device::press_key(serial, key).await?;
            Ok(json!({ "message": message, "key": key }))
        }
        "start_app" => {
            let package = required_str(args, "package")?;
            let message = device::start_app(serial, package).await?;
            Ok(json!({ "message": message, "package": package }))
        }
        "wait" => {
            let milliseconds = args.get("ms").and_then(Value::as_u64).unwrap_or(800);
            let message = device::wait_ms(milliseconds).await;
            Ok(json!({ "message": message }))
        }
        other => Err(format!("不允许或未知的工具: {other}")),
    }
}

async fn resolve_target(
    serial: &str,
    args: &Value,
    observation_nodes: &[UiNode],
) -> Result<(i32, i32), String> {
    if let (Some(x), Some(y)) = (
        args.get("x").and_then(Value::as_i64),
        args.get("y").and_then(Value::as_i64),
    ) {
        return Ok((x as i32, y as i32));
    }
    let bounds = if let Some(node) = find_target_node(observation_nodes, args) {
        node.bounds
    } else {
        let nodes = device::dump_ui_nodes(serial).await?;
        find_target_node(&nodes, args)
            .ok_or_else(|| "长按目标既没有坐标，也无法通过文本或 ID 定位".to_string())?
            .bounds
    };
    Ok(bounds.center())
}

fn find_target_node<'a>(nodes: &'a [UiNode], args: &Value) -> Option<&'a UiNode> {
    if let Some(text) = args.get("text").and_then(Value::as_str) {
        device::find_node_by_text(nodes, text)
    } else {
        args.get("resource_id")
            .and_then(Value::as_str)
            .and_then(|resource_id| device::find_node_by_id(nodes, resource_id))
    }
}

async fn ask_model(
    settings: &StoredSettings,
    run_id: &str,
    instruction: &str,
    device_serial: &str,
    step_index: u32,
    observation: &Observation,
    history: &[String],
    skills: &[SkillHint],
    last_error: Option<&str>,
) -> Result<ModelCallResult, String> {
    let started = Instant::now();
    let endpoint = chat_endpoint(&settings.base_url);
    let system_prompt = system_prompt();
    let user_prompt = user_prompt(
        instruction,
        device_serial,
        step_index,
        settings.max_steps,
        observation,
        history,
        skills,
        last_error,
    );
    let prompt_chars = user_prompt.chars().count();
    let node_count = compact_nodes(&observation.nodes, MODEL_NODE_LIMIT).len();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|error| format!("创建模型 HTTP 客户端失败: {error}"))?;

    let mut last_parse_error = String::new();
    let mut raw_responses = Vec::new();
    let mut usage = Value::Null;
    for attempt in 0..2 {
        let mut prompt = user_prompt.clone();
        if attempt > 0 {
            prompt.push_str(&format!(
                "\n\n上一次响应无法解析为合法工具 JSON：{last_parse_error}\n只返回一个 JSON 对象，不要 Markdown。"
            ));
        }
        let content = if settings.use_vision {
            json!([
                { "type": "text", "text": prompt },
                {
                    "type": "image_url",
                    "image_url": { "url": observation.screenshot_data_url }
                }
            ])
        } else {
            Value::String(prompt)
        };
        let body = json!({
            "model": settings.model,
            "temperature": settings.temperature,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": content }
            ]
        });

        let response = client
            .post(&endpoint)
            .bearer_auth(&settings.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|error| format!("请求多模态模型失败: {error}"))?;
        let status = response.status();
        let raw = response
            .text()
            .await
            .map_err(|error| format!("读取模型响应失败: {error}"))?;
        if !status.is_success() {
            return Err(format!(
                "模型服务返回 HTTP {}: {}",
                status.as_u16(),
                truncate(&raw, 500)
            ));
        }
        let parsed: Value = serde_json::from_str(&raw)
            .map_err(|error| format!("解析模型 API JSON 失败: {error}"))?;
        if let Some(value) = parsed.get("usage") {
            usage = value.clone();
        }
        let content = extract_message_content(&parsed)?;
        raw_responses.push(format!("attempt {}:\n{}", attempt + 1, content));
        match parse_decision(&content) {
            Ok(decision) => {
                if !ALLOWED_TOOLS.contains(&decision.action.as_str()) {
                    last_parse_error = format!("工具 {} 不在允许列表中", decision.action);
                    continue;
                }
                return Ok(ModelCallResult {
                    decision,
                    trace: ModelTraceRecord {
                        run_id: run_id.to_string(),
                        step_index,
                        model: settings.model.clone(),
                        prompt_text: user_prompt.clone(),
                        endpoint: endpoint.clone(),
                        prompt_chars,
                        system_chars: system_prompt.chars().count(),
                        node_count,
                        use_vision: settings.use_vision,
                        attempt_count: attempt as u32 + 1,
                        raw_response: raw_responses.join("\n\n"),
                        usage,
                        duration_ms: started.elapsed().as_millis() as u64,
                        created_at: Utc::now().to_rfc3339(),
                    },
                });
            }
            Err(error) => last_parse_error = error,
        }
    }
    Err(format!(
        "模型连续两次未返回合法工具调用: {last_parse_error}"
    ))
}

fn system_prompt() -> &'static str {
    r#"你是运行在 Android 设备上的 AI App 自动化 Agent。你必须同时利用 accessibility/UI 树和截图理解界面，并一次只执行一个可验证动作。

只输出一个严格 JSON 对象，不要 Markdown、代码块或额外解释：
{
  "thought": "简短说明当前判断和下一动作的目的",
  "action": {
    "name": "工具名",
    "args": {}
  },
  "summary": null
}

可用工具：
1. observe_screen {}：重新观察，不执行动作。
2. tap {"x":整数,"y":整数}：坐标点击，仅在无法用语义定位时使用。
3. tap_text {"text":"可见文本或content-desc"}：优先使用。
4. tap_id {"resource_id":"完整或结尾 resource-id"}：优先使用稳定 ID。
5. long_press {"text":"..."} 或 {"resource_id":"..."} 或 {"x":..,"y":..,"duration_ms":800}。
6. swipe {"direction":"up|down|left|right","distance":0.55,"duration_ms":320}。
7. input_text {"text":"内容","clear":true}：先点击输入框，再输入。
8. press_key {"key":"back|home|enter|recents|delete|search"}。
9. start_app {"package":"com.example.app"}。
10. wait {"ms":800}。
11. ask_user {"question":"需要用户补充的问题"}：仅在缺关键信息、登录/验证码或无法判断时使用。
12. finish {"success":true,"summary":"包含可见证据的结果说明"}：任务完成或确认无法完成时调用。

规则：
- 点击元素优先 tap_text/tap_id，坐标点击只作为兜底。
- 每次动作前基于当前截图和 UI 树重新确认目标，禁止复用历史坐标。
- 输入前先点击输入框；输入后应在后续观察中检查文本。
- 遇到登录、验证码、支付密码或生物识别时调用 ask_user，要求人工处理。
- 不要执行与用户目标无关的发送、支付、删除、安装、权限修改等副作用动作。
- 不要声称完成，除非当前观察中有明确证据；finish.summary 必须说明验证依据。
- 不要单独调用 finish 与另一个工具；一次只能一个动作。
- 相似历史 Skill 只作为已验证路线提示，不盲信旧坐标，必须根据当前页面校验。"#
}

fn user_prompt(
    instruction: &str,
    device_serial: &str,
    step_index: u32,
    max_steps: u32,
    observation: &Observation,
    history: &[String],
    skills: &[SkillHint],
    last_error: Option<&str>,
) -> String {
    let nodes = compact_nodes(&observation.nodes, MODEL_NODE_LIMIT);
    let recent_history = history
        .iter()
        .rev()
        .take(HISTORY_LIMIT)
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    let skill_text = if skills.is_empty() {
        "无".to_string()
    } else {
        skills
            .iter()
            .map(|skill| {
                let steps = skill
                    .plan
                    .iter()
                    .take(10)
                    .enumerate()
                    .map(|(index, step)| {
                        format!("{}. {} {} ({})", index + 1, step.tool, step.args, step.note)
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                format!(
                    "Skill: {}\n说明: {}\n历史步骤:\n{}",
                    skill.name, skill.description, steps
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    };

    format!(
        "用户任务：{instruction}\n设备：{device_serial}\n当前步骤：{step_index}/{max_steps}\n前台：{} / {}\n页面签名：{}\n视口：{}x{}\n\n可操作 UI 节点 JSON：\n{}\n\n最近执行记录：\n{}\n\n相似已验证 Skill：\n{}\n\n上一次工具错误：\n{}\n\n请基于当前屏幕截图和 UI 节点决定下一步。只返回一个 JSON 工具调用。",
        observation.package,
        observation.activity,
        &observation.page_signature[..observation.page_signature.len().min(16)],
        observation.width,
        observation.height,
        serde_json::to_string_pretty(&nodes).unwrap_or_else(|_| "[]".to_string()),
        if recent_history.is_empty() {
            "无"
        } else {
            &recent_history
        },
        skill_text,
        last_error.unwrap_or("无")
    )
}

fn compact_nodes(nodes: &[UiNode], limit: usize) -> Vec<Value> {
    let mut candidates = nodes
        .iter()
        .filter(|node| node.is_actionable())
        .filter(|node| {
            node.clickable
                || node.scrollable
                || !node.text.trim().is_empty()
                || !node.content_description.trim().is_empty()
                || !node.resource_id.trim().is_empty()
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|node| {
        (
            if node.clickable { 0 } else { 1 },
            if !node.text.is_empty() || !node.content_description.is_empty() {
                0
            } else {
                1
            },
            -node.bounds.area(),
        )
    });
    candidates
        .into_iter()
        .take(limit)
        .map(|node| {
            json!({
                "i": node.index,
                "text": node.text,
                "desc": node.content_description,
                "id": node.resource_id,
                "class": node.class_name,
                "bounds": [node.bounds.left, node.bounds.top, node.bounds.right, node.bounds.bottom],
                "click": node.clickable,
                "scroll": node.scrollable,
                "focus": node.focusable
            })
        })
        .collect()
}

fn parse_decision(content: &str) -> Result<ModelDecision, String> {
    let cleaned = content
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let start = cleaned
        .find('{')
        .ok_or_else(|| "响应中没有 JSON 对象".to_string())?;
    let end = cleaned
        .rfind('}')
        .ok_or_else(|| "响应中没有 JSON 结束符".to_string())?;
    let json_text = &cleaned[start..=end];
    let raw: RawModelDecision =
        serde_json::from_str(json_text).map_err(|error| format!("工具 JSON 解析失败: {error}"))?;
    Ok(raw.into_decision())
}

fn extract_message_content(response: &Value) -> Result<String, String> {
    let content = response
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .ok_or_else(|| "模型响应缺少 choices[0].message.content".to_string())?;

    if let Some(text) = content.as_str() {
        return Ok(text.to_string());
    }
    if let Some(parts) = content.as_array() {
        let text = parts
            .iter()
            .filter_map(|part| {
                part.get("text")
                    .and_then(Value::as_str)
                    .or_else(|| part.as_str())
            })
            .collect::<Vec<_>>()
            .join("\n");
        if !text.is_empty() {
            return Ok(text);
        }
    }
    Err("模型响应 content 不是可读取文本".to_string())
}

fn chat_endpoint(base_url: &str) -> String {
    let normalized = base_url.trim().trim_end_matches('/');
    if normalized.ends_with("/chat/completions") {
        normalized.to_string()
    } else {
        format!("{normalized}/chat/completions")
    }
}

fn emit_observation(deps: &AgentDeps, run_id: &str, step_index: u32, observation: &Observation) {
    emit(
        deps.app.as_ref(),
        run_id,
        "observation",
        "屏幕观察",
        &format!(
            "{} / {} · {} 个 UI 节点",
            observation.package,
            observation.activity,
            observation.nodes.len()
        ),
        step_index,
        Some(json!({ "observation": observation })),
    );
}

fn emit(
    app: Option<&AppHandle>,
    run_id: &str,
    event_type: &str,
    title: &str,
    detail: &str,
    step_index: u32,
    data: Option<Value>,
) {
    let Some(app) = app else {
        return;
    };
    let _ = app.emit(
        "agent-event",
        AgentEvent {
            run_id: run_id.to_string(),
            event_type: event_type.to_string(),
            title: title.to_string(),
            detail: detail.to_string(),
            step_index,
            timestamp: Utc::now().to_rfc3339(),
            data,
        },
    );
}

fn required_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("工具参数缺少 {key}"))
}

fn required_i32(args: &Value, key: &str) -> Result<i32, String> {
    args.get(key)
        .and_then(Value::as_i64)
        .map(|value| value as i32)
        .ok_or_else(|| format!("工具参数缺少整数 {key}"))
}

fn swipe_points(
    direction: &str,
    width: i32,
    height: i32,
    distance: f64,
) -> Result<((i32, i32), (i32, i32)), String> {
    let half = distance / 2.0;
    match direction {
        "up" => Ok((
            (width / 2, ((0.5 + half) * height as f64) as i32),
            (width / 2, ((0.5 - half) * height as f64) as i32),
        )),
        "down" => Ok((
            (width / 2, ((0.5 - half) * height as f64) as i32),
            (width / 2, ((0.5 + half) * height as f64) as i32),
        )),
        "left" => Ok((
            (((0.5 + half) * width as f64) as i32, height / 2),
            (((0.5 - half) * width as f64) as i32, height / 2),
        )),
        "right" => Ok((
            (((0.5 - half) * width as f64) as i32, height / 2),
            (((0.5 + half) * width as f64) as i32, height / 2),
        )),
        other => Err(format!("不支持的滑动方向: {other}")),
    }
}

pub(crate) fn contains_high_risk(text: &str) -> bool {
    HIGH_RISK_TERMS.iter().any(|keyword| text.contains(keyword))
}

fn result_summary(result: &Value) -> String {
    result
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| result.get("error").and_then(Value::as_str))
        .unwrap_or("工具执行完成")
        .to_string()
}

fn truncate(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wrapped_tool_json() {
        let content = r#"```json
        {"thought":"tap search","action":{"name":"tap_text","args":{"text":"搜索"}},"summary":null}
        ```"#;
        let decision = parse_decision(content).unwrap();
        assert_eq!(decision.action, "tap_text");
        assert_eq!(decision.args["text"], "搜索");
    }

    #[test]
    fn detects_high_risk_instruction() {
        assert!(contains_high_risk("打开应用并完成付款"));
        assert!(!contains_high_risk("打开应用并查看商品详情"));
    }

    #[test]
    fn builds_openai_compatible_endpoint() {
        assert_eq!(
            chat_endpoint("https://api.example.com/v1/"),
            "https://api.example.com/v1/chat/completions"
        );
    }
}
