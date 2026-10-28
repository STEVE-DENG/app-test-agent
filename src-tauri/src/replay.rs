use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter};

use crate::device;
use crate::knowledge::{KnowledgeStore, element_key};
use crate::models::{
    AgentEvent, AppKnowledgePlugin, Observation, ReplayLocatorEvidence, ReplayStepEvidence,
    TestCaseRunResult, TestFailureAnalysis, UiNode,
};

pub struct ReplayDeps {
    pub app: Option<AppHandle>,
    pub knowledge: Arc<KnowledgeStore>,
    pub artifacts_dir: PathBuf,
}

pub async fn run_test_case(
    deps: ReplayDeps,
    run_id: String,
    device_serial: String,
    package: String,
    test_id: String,
) -> Result<TestCaseRunResult, String> {
    device::ensure_device_ready(&device_serial).await?;
    let test_case = deps.knowledge.get_test_case(&package, &test_id)?;
    let total_steps = test_case.steps.len() as u32;
    deps.knowledge.set_test_result(
        &package,
        &test_id,
        "running",
        "正在重放",
        Some(&run_id),
        None,
    )?;
    emit(
        deps.app.as_ref(),
        &run_id,
        "test_started",
        "开始重放用例",
        &test_case.name,
        0,
        Some(json!({
            "testId": test_id,
            "package": package,
            "totalSteps": total_steps
        })),
    );

    device::restart_app_cleanly(&device_serial, &package).await?;
    let mut observation = capture(&deps, &device_serial, &run_id, "test-000-initial").await?;
    let mut plugin = deps.knowledge.get_plugin(&package)?;
    if !same_page(&plugin, &test_case.start_page_id, &observation.page_id) {
        let message = format!(
            "起始页面不匹配：期望 {}，实际 {}",
            page_label(&plugin, &test_case.start_page_id),
            page_label(&plugin, &observation.page_id)
        );
        let analysis = initial_page_failure(
            &plugin,
            &test_case,
            &observation,
            test_case.last_analysis.as_ref(),
        );
        return finish(
            &deps,
            &run_id,
            &package,
            &test_id,
            "failed",
            0,
            total_steps,
            message,
            Some(analysis),
        );
    }

    let mut passed_steps = 0_u32;
    for (index, step) in test_case.steps.iter().enumerate() {
        let before = observation.clone();
        let execution = execute_step(&device_serial, &before, step, &plugin).await;
        let error = execution.result.as_ref().err().cloned();
        device::wait_ms(if error.is_some() { 350 } else { 700 }).await;
        let after = capture(
            &deps,
            &device_serial,
            &run_id,
            &format!("test-{:03}-after", index + 1),
        )
        .await
        .unwrap_or_else(|_| before.clone());
        let result = match &execution.result {
            Ok(result) => result.clone(),
            Err(error) => json!({ "error": error }),
        };
        let success = error.is_none();
        let evidence = build_step_evidence(
            index as u32 + 1,
            step,
            &before,
            &after,
            execution.locator,
            result.clone(),
            error.clone(),
            success,
        );
        deps.knowledge.record_action(
            &run_id,
            index as u32 + 1,
            &before,
            &after,
            &step.action,
            &step.args,
            &result,
            success,
        )?;
        plugin = deps.knowledge.get_plugin(&package)?;

        if let Some(error) = error {
            let (category, confidence, suggestions) = classify_execution_error(&error);
            let analysis = build_failure_analysis(
                category,
                confidence,
                format!("第 {} 步执行失败：{error}", index + 1),
                index as u32 + 1,
                evidence,
                test_case.last_analysis.as_ref(),
                suggestions,
                false,
            );
            emit(
                deps.app.as_ref(),
                &run_id,
                "test_step_failed",
                "用例步骤执行失败",
                &error,
                index as u32 + 1,
                Some(json!({
                    "step": index + 1,
                    "label": step.label,
                    "error": error,
                    "failure": analysis
                })),
            );
            return finish(
                &deps,
                &run_id,
                &package,
                &test_id,
                "failed",
                passed_steps,
                total_steps,
                analysis.summary.clone(),
                Some(analysis),
            );
        }

        let matched = same_page(&plugin, &step.expected_page_id, &after.page_id);
        emit(
            deps.app.as_ref(),
            &run_id,
            if matched {
                "test_step_passed"
            } else {
                "test_step_failed"
            },
            if matched {
                "用例步骤通过"
            } else {
                "用例步骤未达预期页面"
            },
            &format!(
                "#{} {} · 期望 {} · 实际 {}",
                index + 1,
                step.label,
                page_label(&plugin, &step.expected_page_id),
                page_label(&plugin, &after.page_id)
            ),
            index as u32 + 1,
            Some(json!({
                "fromPageId": before.page_id,
                "expectedPageId": step.expected_page_id,
                "actualPageId": after.page_id,
                "action": step.action,
                "label": step.label,
                "result": result
            })),
        );
        if !matched {
            let (mut category, mut confidence, mut suggestions) =
                classify_page_mismatch(&plugin, &step.expected_page_id, &after.page_id);
            let mut can_be_regression = before.page_id != after.page_id;
            let mut summary = format!(
                "第 {} 步未到达预期页面：期望 {}，实际 {}",
                index + 1,
                page_label(&plugin, &step.expected_page_id),
                page_label(&plugin, &after.page_id)
            );
            if evidence
                .locator
                .as_ref()
                .is_some_and(|locator| !locator_matches_step(locator, step))
            {
                category = "element-not-found";
                confidence = 0.91;
                can_be_regression = false;
                summary.push_str("；实际点击控件与用例记录语义不一致，按定位失效处理");
                suggestions = vec![
                    "该控件可能是信息流回收位，已拒绝按旧位置盲点。".to_string(),
                    "使用当前截图/OCR 重新确认目标，并更新测试用例的稳定锚点。".to_string(),
                    "若控件已消失，保留失败证据并等待页面图合并或人工确认。".to_string(),
                ];
            }
            let analysis = build_failure_analysis(
                category,
                confidence,
                summary,
                index as u32 + 1,
                evidence,
                test_case.last_analysis.as_ref(),
                suggestions,
                can_be_regression,
            );
            return finish(
                &deps,
                &run_id,
                &package,
                &test_id,
                "failed",
                passed_steps,
                total_steps,
                analysis.summary.clone(),
                Some(analysis),
            );
        }
        passed_steps += 1;
        observation = after;
    }

    finish(
        &deps,
        &run_id,
        &package,
        &test_id,
        "verified",
        passed_steps,
        total_steps,
        format!("{passed_steps}/{total_steps} 步通过，页面跳转与预期一致"),
        None,
    )
}

fn finish(
    deps: &ReplayDeps,
    run_id: &str,
    package: &str,
    test_id: &str,
    status: &str,
    passed_steps: u32,
    total_steps: u32,
    message: String,
    analysis: Option<TestFailureAnalysis>,
) -> Result<TestCaseRunResult, String> {
    deps.knowledge.set_test_result(
        package,
        test_id,
        status,
        &message,
        Some(run_id),
        analysis.as_ref(),
    )?;
    let result = TestCaseRunResult {
        run_id: run_id.to_string(),
        test_id: test_id.to_string(),
        package: package.to_string(),
        status: status.to_string(),
        passed_steps,
        total_steps,
        message: message.clone(),
        failure: analysis,
        finished_at: Utc::now().to_rfc3339(),
    };
    emit(
        deps.app.as_ref(),
        run_id,
        if status == "verified" {
            "test_completed"
        } else {
            "test_failed"
        },
        if status == "verified" {
            "测试用例通过"
        } else {
            "测试用例失败"
        },
        &message,
        total_steps,
        Some(json!({ "result": result, "package": package })),
    );
    Ok(result)
}

struct StepExecution {
    result: Result<Value, String>,
    locator: Option<ReplayLocatorEvidence>,
}

struct LocatedNode<'a> {
    node: &'a UiNode,
    strategy: &'static str,
    confidence: f64,
    candidate_count: usize,
}

async fn execute_step(
    serial: &str,
    observation: &Observation,
    step: &crate::models::TestCaseStep,
    plugin: &AppKnowledgePlugin,
) -> StepExecution {
    let requires_node = matches!(
        step.action.as_str(),
        "tap_text" | "tap_id" | "tap" | "long_press"
    );
    let located = requires_node
        .then(|| locate_node_for_step(observation, step, plugin))
        .flatten();
    let locator = located
        .as_ref()
        .map(|located| locator_from_node(observation, step, located))
        .or_else(|| requires_node.then(|| missing_locator(observation, step)));
    let result = match step.action.as_str() {
        "tap_text" | "tap_id" | "tap" => {
            let Some(located) = located else {
                return StepExecution {
                    result: Err(format!("当前页面找不到控件：{}", step.label)),
                    locator,
                };
            };
            let target = located.node;
            let (x, y) = target.bounds.center();
            device::tap(serial, x, y).await.map(|message| {
                json!({
                    "message": message,
                    "locatorSource": located.strategy,
                    "locatorConfidence": located.confidence,
                    "candidateCount": located.candidate_count,
                    "elementKey": element_key(target),
                    "label": if target.text.trim().is_empty() {
                        target.content_description.trim()
                    } else {
                        target.text.trim()
                    },
                    "resourceId": target.resource_id,
                    "className": target.class_name,
                    "bounds": target.bounds
                })
            })
        }
        "long_press" => {
            let Some(located) = located else {
                return StepExecution {
                    result: Err(format!("当前页面找不到控件：{}", step.label)),
                    locator,
                };
            };
            let target = located.node;
            let (x, y) = target.bounds.center();
            let duration = step
                .args
                .get("duration_ms")
                .and_then(Value::as_u64)
                .unwrap_or(800);
            device::long_press(serial, x, y, duration)
                .await
                .map(|message| json!({ "message": message, "label": step.label }))
        }
        "swipe" => {
            let direction = step
                .args
                .get("direction")
                .and_then(Value::as_str)
                .unwrap_or("up");
            let distance = step
                .args
                .get("distance")
                .and_then(Value::as_f64)
                .unwrap_or(0.5);
            let width = observation.width.max(1);
            let height = observation.height.max(1);
            match swipe_points(direction, width, height, distance) {
                Ok((start, end)) => device::swipe(serial, start, end, 380)
                    .await
                    .map(|message| json!({ "message": message, "direction": direction })),
                Err(error) => Err(error),
            }
        }
        "input_text" => {
            let text = match step.args.get("text").and_then(Value::as_str) {
                Some(text) => text,
                None => {
                    return StepExecution {
                        result: Err("input_text 缺少 text".to_string()),
                        locator,
                    };
                }
            };
            let clear = step
                .args
                .get("clear")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            device::input_text(serial, text, clear)
                .await
                .map(|message| json!({ "message": message, "text": text }))
        }
        "press_key" => {
            let key = step
                .args
                .get("key")
                .and_then(Value::as_str)
                .unwrap_or("back");
            device::press_key(serial, key)
                .await
                .map(|message| json!({ "message": message, "key": key }))
        }
        "start_app" => {
            let package = match step.args.get("package").and_then(Value::as_str) {
                Some(package) => package,
                None => {
                    return StepExecution {
                        result: Err("start_app 缺少 package".to_string()),
                        locator,
                    };
                }
            };
            device::start_app(serial, package)
                .await
                .map(|message| json!({ "message": message, "package": package }))
        }
        "wait" => {
            let milliseconds = step.args.get("ms").and_then(Value::as_u64).unwrap_or(700);
            let message = device::wait_ms(milliseconds).await;
            Ok(json!({ "message": message }))
        }
        other => Err(format!("测试回放不支持动作：{other}")),
    };
    StepExecution { result, locator }
}

fn find_node_by_key<'a>(observation: &'a Observation, key: &str) -> Option<&'a UiNode> {
    if key.trim().is_empty() {
        return None;
    }
    observation
        .nodes
        .iter()
        .filter(|node| node.is_actionable())
        .find(|node| element_key(node) == key)
}

fn locate_node_for_step<'a>(
    observation: &'a Observation,
    step: &crate::models::TestCaseStep,
    plugin: &AppKnowledgePlugin,
) -> Option<LocatedNode<'a>> {
    let args_element_key = step
        .args
        .get("elementKey")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if let Some(node) = find_node_by_key(observation, args_element_key) {
        return Some(LocatedNode {
            node,
            strategy: "args.elementKey",
            confidence: 1.0,
            candidate_count: count_key_candidates(&observation.nodes, args_element_key),
        });
    }
    if let Some(node) = find_node_from_knowledge(observation, step, plugin, args_element_key) {
        return Some(LocatedNode {
            node,
            strategy: "knowledge.element",
            confidence: 0.92,
            candidate_count: count_key_candidates(&observation.nodes, args_element_key),
        });
    }
    if let Some(text) = step.args.get("text").and_then(Value::as_str)
        && let Some(node) = device::find_node_by_text(&observation.nodes, text)
    {
        return Some(LocatedNode {
            node,
            strategy: "args.text",
            confidence: 0.86,
            candidate_count: semantic_candidate_count(&observation.nodes, text),
        });
    }
    if let Some(node) = find_node_by_key(observation, &step.element_key) {
        return Some(LocatedNode {
            node,
            strategy: "step.elementKey",
            confidence: 0.88,
            candidate_count: count_key_candidates(&observation.nodes, &step.element_key),
        });
    }
    if let Some(node) = find_node_from_knowledge(observation, step, plugin, &step.element_key) {
        return Some(LocatedNode {
            node,
            strategy: "knowledge.fallback",
            confidence: 0.8,
            candidate_count: count_key_candidates(&observation.nodes, &step.element_key),
        });
    }
    if let Some(resource_id) = step
        .args
        .get("resource_id")
        .and_then(Value::as_str)
        .filter(|resource_id| is_specific_resource_id(resource_id))
        && let Some(node) = device::find_node_by_id(&observation.nodes, resource_id)
    {
        return Some(LocatedNode {
            node,
            strategy: "args.resourceId",
            confidence: 0.84,
            candidate_count: observation
                .nodes
                .iter()
                .filter(|node| {
                    node.is_actionable()
                        && (node.resource_id == resource_id
                            || node.resource_id.ends_with(resource_id))
                })
                .count(),
        });
    }
    if let Some(node) = device::find_node_by_text(&observation.nodes, &step.label) {
        return Some(LocatedNode {
            node,
            strategy: "step.labelText",
            confidence: 0.7,
            candidate_count: semantic_candidate_count(&observation.nodes, &step.label),
        });
    }
    find_node_by_legacy_label(&observation.nodes, &step.label).map(|node| LocatedNode {
        node,
        strategy: "legacy.bounds",
        confidence: 0.48,
        candidate_count: 1,
    })
}

fn count_key_candidates(nodes: &[UiNode], key: &str) -> usize {
    if key.trim().is_empty() {
        return 0;
    }
    nodes
        .iter()
        .filter(|node| node.is_actionable() && element_key(node) == key)
        .count()
}

fn semantic_candidate_count(nodes: &[UiNode], query: &str) -> usize {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return 0;
    }
    nodes
        .iter()
        .filter(|node| {
            node.is_actionable()
                && [&node.text, &node.content_description, &node.resource_id]
                    .iter()
                    .any(|value| value.to_lowercase().contains(&query))
        })
        .count()
}

fn locator_from_node(
    observation: &Observation,
    step: &crate::models::TestCaseStep,
    located: &LocatedNode<'_>,
) -> ReplayLocatorEvidence {
    let requested = step
        .args
        .get("elementKey")
        .and_then(Value::as_str)
        .or_else(|| step.args.get("text").and_then(Value::as_str))
        .or_else(|| step.args.get("resource_id").and_then(Value::as_str))
        .unwrap_or(&step.label)
        .to_string();
    ReplayLocatorEvidence {
        strategy: located.strategy.to_string(),
        requested,
        matched_label: node_label(located.node),
        element_key: Some(element_key(located.node)),
        resource_id: (!located.node.resource_id.trim().is_empty())
            .then(|| located.node.resource_id.clone()),
        bounds: Some(located.node.bounds.clone()),
        candidate_count: located.candidate_count.max(count_key_candidates(
            &observation.nodes,
            &element_key(located.node),
        )),
        confidence: located.confidence,
        attempted_strategies: vec![located.strategy.to_string()],
    }
}

fn missing_locator(
    observation: &Observation,
    step: &crate::models::TestCaseStep,
) -> ReplayLocatorEvidence {
    let requested = step
        .args
        .get("elementKey")
        .and_then(Value::as_str)
        .or_else(|| step.args.get("text").and_then(Value::as_str))
        .or_else(|| step.args.get("resource_id").and_then(Value::as_str))
        .unwrap_or(&step.label);
    ReplayLocatorEvidence {
        strategy: "not_found".to_string(),
        requested: requested.to_string(),
        matched_label: String::new(),
        element_key: None,
        resource_id: step
            .args
            .get("resource_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        bounds: None,
        candidate_count: semantic_candidate_count(&observation.nodes, requested),
        confidence: 0.0,
        attempted_strategies: vec![
            "args.elementKey".to_string(),
            "knowledge.element".to_string(),
            "args.text".to_string(),
            "step.elementKey".to_string(),
            "args.resourceId".to_string(),
            "step.labelText".to_string(),
            "legacy.bounds".to_string(),
        ],
    }
}

fn locator_matches_step(
    locator: &ReplayLocatorEvidence,
    step: &crate::models::TestCaseStep,
) -> bool {
    if !locator.strategy.starts_with("knowledge") {
        return true;
    }
    let expected = step
        .args
        .get("text")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&step.label);
    let expected = normalize_semantic_label(expected);
    let matched = normalize_semantic_label(&locator.matched_label);
    if expected.chars().count() < 2 || matched.chars().count() < 2 {
        return true;
    }
    expected.contains(&matched) || matched.contains(&expected)
}

fn normalize_semantic_label(value: &str) -> String {
    value
        .chars()
        .filter(|character| {
            !character.is_whitespace()
                && !character.is_ascii_digit()
                && !matches!(character, ',' | '，' | '。' | ':' | '：' | '>' | '-' | '_')
        })
        .flat_map(char::to_lowercase)
        .collect()
}

fn node_label(node: &UiNode) -> String {
    if !node.text.trim().is_empty() {
        return node.text.trim().chars().take(120).collect();
    }
    if !node.content_description.trim().is_empty() {
        return node.content_description.trim().chars().take(120).collect();
    }
    if !node.resource_id.trim().is_empty() {
        return node.resource_id.trim().chars().take(120).collect();
    }
    format!(
        "{} @ {},{}",
        node.class_name, node.bounds.left, node.bounds.top
    )
}

fn build_step_evidence(
    step_index: u32,
    step: &crate::models::TestCaseStep,
    before: &Observation,
    after: &Observation,
    locator: Option<ReplayLocatorEvidence>,
    result: Value,
    error: Option<String>,
    success: bool,
) -> ReplayStepEvidence {
    ReplayStepEvidence {
        step_index,
        label: step.label.clone(),
        action: step.action.clone(),
        from_page_id: before.page_id.clone(),
        expected_page_id: step.expected_page_id.clone(),
        actual_page_id: after.page_id.clone(),
        before_screenshot: before.screenshot_path.clone(),
        after_screenshot: after.screenshot_path.clone(),
        before_ui_path: before.ui_path.clone(),
        after_ui_path: after.ui_path.clone(),
        before_page_signature: before.page_signature.clone(),
        after_page_signature: after.page_signature.clone(),
        locator,
        result,
        error,
        success,
    }
}

fn initial_page_failure(
    plugin: &AppKnowledgePlugin,
    test_case: &crate::models::AppTestCase,
    observation: &Observation,
    previous: Option<&TestFailureAnalysis>,
) -> TestFailureAnalysis {
    let message = format!(
        "起始页面不匹配：期望 {}，实际 {}",
        page_label(plugin, &test_case.start_page_id),
        page_label(plugin, &observation.page_id)
    );
    let evidence = ReplayStepEvidence {
        step_index: 0,
        label: "启动应用并校验起始页面".to_string(),
        action: "start_app".to_string(),
        from_page_id: observation.page_id.clone(),
        expected_page_id: test_case.start_page_id.clone(),
        actual_page_id: observation.page_id.clone(),
        before_screenshot: observation.screenshot_path.clone(),
        after_screenshot: observation.screenshot_path.clone(),
        before_ui_path: observation.ui_path.clone(),
        after_ui_path: observation.ui_path.clone(),
        before_page_signature: observation.page_signature.clone(),
        after_page_signature: observation.page_signature.clone(),
        locator: None,
        result: json!({}),
        error: Some(message.clone()),
        success: false,
    };
    build_failure_analysis(
        "wrong-page",
        0.88,
        message,
        0,
        evidence,
        previous,
        vec![
            "确认实验账号的冷启动页面是否稳定，并检查登录/引导/权限弹窗。".to_string(),
            "对比页面截图和 Activity，必要时将起始页改成可稳定到达的入口页。".to_string(),
        ],
        false,
    )
}

fn classify_execution_error(error: &str) -> (&'static str, f64, Vec<String>) {
    if error.contains("找不到控件") || error.contains("找不到元素 ID") {
        return (
            "element-not-found",
            0.96,
            vec![
                "检查当前页是否仍在加载，并比较 before/after 截图与 UI XML。".to_string(),
                "优先用 OCR 孤立文本块或稳定 resource-id 重新定位，不要复用旧坐标。".to_string(),
                "若信息流内容已变化，将用例标记为动态内容而不是直接删除。".to_string(),
            ],
        );
    }
    if error.contains("缺少") || error.contains("不支持动作") {
        return (
            "unknown",
            0.45,
            vec!["检查测试用例动作参数是否完整，并升级回放器支持对应动作。".to_string()],
        );
    }
    (
        "unknown",
        0.35,
        vec![
            "读取 before/after 截图、UI XML 和 Logcat，确认是设备异常还是页面状态异常。"
                .to_string(),
        ],
    )
}

fn classify_page_mismatch(
    plugin: &AppKnowledgePlugin,
    expected_id: &str,
    actual_id: &str,
) -> (&'static str, f64, Vec<String>) {
    let expected = plugin.pages.iter().find(|page| page.id == expected_id);
    let actual = plugin.pages.iter().find(|page| page.id == actual_id);
    let same_activity = expected
        .zip(actual)
        .is_some_and(|(left, right)| left.activity == right.activity);
    let similarity = expected
        .zip(actual)
        .map(|(left, right)| page_similarity(left, right))
        .unwrap_or(0.0);
    if same_activity && similarity >= 0.1 {
        return (
            "dynamic-content",
            0.74,
            vec![
                "比较期望页面与实际页面的稳定元素集合，优先用 resource-id 作为锚点。".to_string(),
                "补充 OCR 文本和动态内容聚类规则，避免把信息流变化当成路由回归。".to_string(),
                "把用例改为校验关键控件或标题，而不是要求整个页面签名完全一致。".to_string(),
            ],
        );
    }
    if same_activity {
        return (
            "wrong-page",
            0.7,
            vec![
                "确认触发控件在当前账号状态下是否跳到其他 Tab、弹窗或中间页。".to_string(),
                "重新采集跳转图并检查该入口是否已有多条目标边。".to_string(),
            ],
        );
    }
    (
        "wrong-page",
        0.82,
        vec![
            "检查 Activity 差异，确认 App 是否升级、账号状态是否变化或入口被重定向。".to_string(),
            "对比历史截图和同源动作，更新页面图前先重复回放一次。".to_string(),
        ],
    )
}

fn build_failure_analysis(
    mut category: &'static str,
    mut confidence: f64,
    mut summary: String,
    failed_step: u32,
    evidence: ReplayStepEvidence,
    previous: Option<&TestFailureAnalysis>,
    suggestions: Vec<String>,
    can_be_regression: bool,
) -> TestFailureAnalysis {
    let repetitions = previous
        .filter(|analysis| analysis.category == category && analysis.failed_step == failed_step)
        .map(|analysis| analysis.repetitions.saturating_add(1))
        .unwrap_or(1);
    if repetitions >= 2 && can_be_regression && matches!(category, "wrong-page" | "unknown") {
        category = "real-regression";
        confidence = (confidence + 0.08).min(0.96);
        summary.push_str("；同一失败已重复出现，暂按疑似真实回归处理");
    }
    let retryable = !matches!(category, "real-regression");
    TestFailureAnalysis {
        category: category.to_string(),
        confidence,
        summary,
        failed_step,
        repetitions,
        retryable,
        suggestions,
        evidence,
        created_at: Utc::now().to_rfc3339(),
    }
}

fn find_node_from_knowledge<'a>(
    observation: &'a Observation,
    step: &crate::models::TestCaseStep,
    plugin: &AppKnowledgePlugin,
    key: &str,
) -> Option<&'a UiNode> {
    if key.trim().is_empty() {
        return None;
    }
    for page_id in [&step.from_page_id, &step.expected_page_id] {
        let Some(page) = plugin.pages.iter().find(|page| page.id == *page_id) else {
            continue;
        };
        let Some(element) = page.elements.iter().find(|element| element.key == key) else {
            continue;
        };
        if let Some(node) = find_node_for_element(&observation.nodes, element) {
            return Some(node);
        }
    }
    None
}

fn find_node_for_element<'a>(
    nodes: &'a [UiNode],
    element: &crate::models::KnowledgeElement,
) -> Option<&'a UiNode> {
    for value in [&element.text, &element.content_description] {
        for variant in semantic_variants(value) {
            if let Some(node) = device::find_node_by_text(nodes, &variant) {
                return Some(node);
            }
        }
    }
    if is_specific_resource_id(&element.resource_id)
        && let Some(node) = device::find_node_by_id(nodes, &element.resource_id)
    {
        return Some(node);
    }
    // A positional fallback is safe only for controls that had no semantic
    // text and no stable ID. Otherwise a recycled feed slot may contain a
    // different product/ad and clicking it would simulate a false regression.
    if !element.text.trim().is_empty()
        || !element.content_description.trim().is_empty()
        || is_specific_resource_id(&element.resource_id)
    {
        return None;
    }
    nodes
        .iter()
        .filter(|node| node.is_actionable() && node.class_name == element.class_name)
        .filter(|node| {
            (node.bounds.top - element.bounds.top).abs() <= 80
                && (node.bounds.left - element.bounds.left).abs() <= 80
        })
        .min_by_key(|node| {
            (node.bounds.top - element.bounds.top).abs()
                + (node.bounds.left - element.bounds.left).abs()
        })
}

fn semantic_variants(value: &str) -> Vec<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let mut variants = vec![trimmed.to_string()];
    for separator in ['\n', '，', ','] {
        if let Some(prefix) = trimmed.split(separator).next() {
            let prefix = prefix.trim();
            if prefix.chars().count() >= 2 && prefix != trimmed {
                variants.push(prefix.to_string());
            }
        }
    }
    variants.sort();
    variants.dedup();
    variants
}

fn is_specific_resource_id(resource_id: &str) -> bool {
    let resource_id = resource_id.trim();
    !resource_id.is_empty()
        && !resource_id.ends_with(":id/tab")
        && !resource_id.ends_with(":id/tab_title")
        && !resource_id.ends_with(":id/tab_icon")
}

fn find_node_by_legacy_label<'a>(nodes: &'a [UiNode], label: &str) -> Option<&'a UiNode> {
    let mut parts = label.trim().rsplitn(2, char::is_whitespace);
    let top = parts.next()?.parse::<i32>().ok()?;
    let class_name = parts.next()?.trim();
    nodes
        .iter()
        .filter(|node| node.is_actionable() && node.class_name == class_name)
        .min_by_key(|node| (node.bounds.top - top).abs())
        .filter(|node| (node.bounds.top - top).abs() <= 120)
}

fn same_page(plugin: &AppKnowledgePlugin, expected_id: &str, actual_id: &str) -> bool {
    if expected_id == actual_id {
        return true;
    }
    let expected = plugin.pages.iter().find(|page| page.id == expected_id);
    let actual = plugin.pages.iter().find(|page| page.id == actual_id);
    let (Some(left), Some(right)) = (expected, actual) else {
        return false;
    };
    if left.activity != right.activity {
        return false;
    }
    if left.name == right.name && !is_weak_page_name(&left.name) {
        return true;
    }
    page_similarity(left, right) >= 0.25
}

fn is_weak_page_name(name: &str) -> bool {
    name.starts_with("闲鱼 ·") || name.contains("Activity") || name.trim().is_empty()
}

fn page_similarity(
    left: &crate::models::KnowledgePage,
    right: &crate::models::KnowledgePage,
) -> f64 {
    let left_tokens = page_tokens(left);
    let right_tokens = page_tokens(right);
    if left_tokens.is_empty() || right_tokens.is_empty() {
        return 0.0;
    }
    let intersection = left_tokens.intersection(&right_tokens).count() as f64;
    let union = left_tokens.union(&right_tokens).count() as f64;
    intersection / union
}

fn page_tokens(page: &crate::models::KnowledgePage) -> std::collections::HashSet<String> {
    page.elements
        .iter()
        .filter(|element| element.clickable || element.scrollable || element.focusable)
        .flat_map(|element| {
            [
                element.resource_id.trim(),
                element.text.trim(),
                element.content_description.trim(),
            ]
        })
        .filter(|value| !value.is_empty())
        .map(|value| value.to_lowercase())
        .collect()
}

fn page_label(plugin: &AppKnowledgePlugin, page_id: &str) -> String {
    plugin
        .pages
        .iter()
        .find(|page| page.id == page_id)
        .map(|page| format!("{} ({})", page.name, &page.id[..page.id.len().min(8)]))
        .unwrap_or_else(|| page_id[..page_id.len().min(8)].to_string())
}

fn swipe_points(
    direction: &str,
    width: i32,
    height: i32,
    distance: f64,
) -> Result<((i32, i32), (i32, i32)), String> {
    let distance = distance.clamp(0.2, 0.8);
    let center_x = width / 2;
    let center_y = height / 2;
    let vertical = (height as f64 * distance) as i32;
    let horizontal = (width as f64 * distance) as i32;
    match direction.to_lowercase().as_str() {
        "up" => Ok((
            (center_x, center_y + vertical / 2),
            (center_x, center_y - vertical / 2),
        )),
        "down" => Ok((
            (center_x, center_y - vertical / 2),
            (center_x, center_y + vertical / 2),
        )),
        "left" => Ok((
            (center_x + horizontal / 2, center_y),
            (center_x - horizontal / 2, center_y),
        )),
        "right" => Ok((
            (center_x - horizontal / 2, center_y),
            (center_x + horizontal / 2, center_y),
        )),
        other => Err(format!("不支持的滑动方向：{other}")),
    }
}

async fn capture(
    deps: &ReplayDeps,
    device_serial: &str,
    run_id: &str,
    label: &str,
) -> Result<Observation, String> {
    let mut observation =
        device::capture_observation(device_serial, &deps.artifacts_dir, run_id, label).await?;
    deps.knowledge.register_observation_mut(&mut observation)?;
    Ok(observation)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Bounds, KnowledgeElement, KnowledgePage};

    fn element(index: usize) -> KnowledgeElement {
        KnowledgeElement {
            key: format!("el-{index}"),
            index,
            text: format!("item-{index}"),
            content_description: String::new(),
            resource_id: format!("com.example:id/item-{index}"),
            class_name: "android.widget.TextView".to_string(),
            bounds: Bounds {
                left: 0,
                top: index as i32 * 20,
                right: 200,
                bottom: index as i32 * 20 + 20,
            },
            clickable: true,
            scrollable: false,
            focusable: true,
        }
    }

    fn page(id: &str, activity: &str) -> KnowledgePage {
        let now = Utc::now().to_rfc3339();
        KnowledgePage {
            id: id.to_string(),
            name: id.to_string(),
            signature: id.to_string(),
            package: "com.example".to_string(),
            activity: activity.to_string(),
            screenshot_path: String::new(),
            ui_path: String::new(),
            width: 1080,
            height: 2400,
            first_seen_at: now.clone(),
            last_seen_at: now,
            visit_count: 1,
            elements: (0..8).map(element).collect(),
        }
    }

    fn plugin() -> AppKnowledgePlugin {
        AppKnowledgePlugin {
            schema_version: 1,
            package: "com.example".to_string(),
            name: "Example".to_string(),
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
            pages: vec![
                page("home", "com.example.MainActivity"),
                page("feed", "com.example.FeedActivity"),
                page("detail", "com.example.DetailActivity"),
            ],
            transitions: Vec::new(),
            actions: Vec::new(),
            test_cases: Vec::new(),
        }
    }

    fn evidence() -> ReplayStepEvidence {
        ReplayStepEvidence {
            step_index: 1,
            label: "点击商品".to_string(),
            action: "tap_text".to_string(),
            from_page_id: "home".to_string(),
            expected_page_id: "detail".to_string(),
            actual_page_id: "feed".to_string(),
            before_screenshot: "/tmp/before.png".to_string(),
            after_screenshot: "/tmp/after.png".to_string(),
            before_ui_path: "/tmp/before.xml".to_string(),
            after_ui_path: "/tmp/after.xml".to_string(),
            before_page_signature: "before".to_string(),
            after_page_signature: "after".to_string(),
            locator: None,
            result: json!({ "message": "clicked" }),
            error: None,
            success: true,
        }
    }

    #[test]
    fn classifies_missing_element_with_actionable_debug_hint() {
        let (category, confidence, suggestions) =
            classify_execution_error("当前页面找不到控件：图片, 08-16 10:40");
        assert_eq!(category, "element-not-found");
        assert!(confidence >= 0.9);
        assert!(!suggestions.is_empty());
    }

    #[test]
    fn upgrades_repeated_page_mismatch_to_suspected_regression() {
        let first = build_failure_analysis(
            "wrong-page",
            0.7,
            "页面不匹配".to_string(),
            1,
            evidence(),
            None,
            vec!["retry".to_string()],
            true,
        );
        assert_eq!(first.category, "wrong-page");
        assert_eq!(first.repetitions, 1);

        let second = build_failure_analysis(
            "wrong-page",
            0.7,
            "页面不匹配".to_string(),
            1,
            evidence(),
            Some(&first),
            vec!["inspect".to_string()],
            true,
        );
        assert_eq!(second.category, "real-regression");
        assert_eq!(second.repetitions, 2);
        assert!(!second.retryable);
    }

    #[test]
    fn keeps_dynamic_same_activity_mismatch_distinct_from_regression() {
        let mut plugin = plugin();
        plugin.pages[1].activity = "com.example.MainActivity".to_string();
        plugin.pages[1].name = "首页 · 动态推荐".to_string();
        let (category, _, _) = classify_page_mismatch(&plugin, "home", "feed");
        assert_eq!(category, "dynamic-content");
    }

    #[test]
    fn rejects_position_fallback_for_recycled_semantic_controls() {
        let nodes = vec![UiNode {
            index: 0,
            text: "闲鱼回收".to_string(),
            content_description: String::new(),
            resource_id: String::new(),
            class_name: "android.widget.ImageView".to_string(),
            package: "com.example".to_string(),
            bounds: Bounds {
                left: 0,
                top: 1809,
                right: 1080,
                bottom: 2026,
            },
            clickable: true,
            enabled: true,
            focusable: true,
            scrollable: false,
            selected: false,
            visible: true,
            depth: 2,
        }];
        let stale = KnowledgeElement {
            key: "stale".to_string(),
            index: 99,
            text: "图片, 08-14 20:25\n去领取".to_string(),
            content_description: String::new(),
            resource_id: String::new(),
            class_name: "android.widget.ImageView".to_string(),
            bounds: Bounds {
                left: 0,
                top: 1809,
                right: 1080,
                bottom: 2026,
            },
            clickable: true,
            scrollable: false,
            focusable: true,
        };
        assert!(find_node_for_element(&nodes, &stale).is_none());
    }

    #[test]
    fn detects_semantically_wrong_knowledge_locator() {
        let locator = ReplayLocatorEvidence {
            strategy: "knowledge.element".to_string(),
            requested: "el-old".to_string(),
            matched_label: "闲鱼回收\n省心极速卖得高".to_string(),
            element_key: Some("el-new".to_string()),
            resource_id: None,
            bounds: None,
            candidate_count: 1,
            confidence: 0.92,
            attempted_strategies: vec!["knowledge.element".to_string()],
        };
        let step = crate::models::TestCaseStep {
            from_page_id: "feed".to_string(),
            expected_page_id: "ad".to_string(),
            element_key: "el-old".to_string(),
            label: "图片, 08-14 20:25\n去领取".to_string(),
            action: "tap_text".to_string(),
            args: json!({ "text": "去领取" }),
        };
        assert!(!locator_matches_step(&locator, &step));
    }
}
