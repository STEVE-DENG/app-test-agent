use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter};

use crate::device;
use crate::knowledge::{KnowledgeStore, element_key};
use crate::models::{AgentEvent, AppKnowledgePlugin, Observation, TestCaseRunResult, UiNode};

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
    deps.knowledge
        .set_test_result(&package, &test_id, "running", "正在重放")?;
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
        return finish(
            &deps,
            &run_id,
            &package,
            &test_id,
            "failed",
            0,
            total_steps,
            message,
        );
    }

    let mut passed_steps = 0_u32;
    for (index, step) in test_case.steps.iter().enumerate() {
        let before = observation.clone();
        let result = execute_step(&device_serial, &before, step, &plugin).await;
        match result {
            Ok(result) => {
                device::wait_ms(700).await;
                let after = capture(
                    &deps,
                    &device_serial,
                    &run_id,
                    &format!("test-{:03}-after", index + 1),
                )
                .await?;
                deps.knowledge.record_action(
                    &run_id,
                    index as u32 + 1,
                    &before,
                    &after,
                    &step.action,
                    &step.args,
                    &result,
                    true,
                )?;
                plugin = deps.knowledge.get_plugin(&package)?;
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
                    return finish(
                        &deps,
                        &run_id,
                        &package,
                        &test_id,
                        "failed",
                        passed_steps,
                        total_steps,
                        format!(
                            "第 {} 步未到达预期页面：期望 {}，实际 {}",
                            index + 1,
                            page_label(&plugin, &step.expected_page_id),
                            page_label(&plugin, &after.page_id)
                        ),
                    );
                }
                passed_steps += 1;
                observation = after;
            }
            Err(error) => {
                deps.knowledge.set_test_result(
                    &package,
                    &test_id,
                    "failed",
                    &format!("第 {} 步执行失败：{error}", index + 1),
                )?;
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
                        "error": error
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
                    format!("第 {} 步执行失败：{error}", index + 1),
                );
            }
        }
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
) -> Result<TestCaseRunResult, String> {
    deps.knowledge
        .set_test_result(package, test_id, status, &message)?;
    let result = TestCaseRunResult {
        test_id: test_id.to_string(),
        package: package.to_string(),
        status: status.to_string(),
        passed_steps,
        total_steps,
        message: message.clone(),
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

async fn execute_step(
    serial: &str,
    observation: &Observation,
    step: &crate::models::TestCaseStep,
    plugin: &AppKnowledgePlugin,
) -> Result<Value, String> {
    let node = find_node_for_step(observation, step, plugin);
    match step.action.as_str() {
        "tap_text" | "tap_id" | "tap" => {
            let target = node.ok_or_else(|| format!("当前页面找不到控件：{}", step.label))?;
            let (x, y) = target.bounds.center();
            let message = device::tap(serial, x, y).await?;
            Ok(json!({
                "message": message,
                "elementKey": element_key(target),
                "label": if target.text.trim().is_empty() {
                    target.content_description.trim()
                } else {
                    target.text.trim()
                },
                "resourceId": target.resource_id,
                "className": target.class_name,
                "bounds": target.bounds
            }))
        }
        "long_press" => {
            let target = node.ok_or_else(|| format!("当前页面找不到控件：{}", step.label))?;
            let (x, y) = target.bounds.center();
            let duration = step
                .args
                .get("duration_ms")
                .and_then(Value::as_u64)
                .unwrap_or(800);
            let message = device::long_press(serial, x, y, duration).await?;
            Ok(json!({ "message": message, "label": step.label }))
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
            let (start, end) = swipe_points(direction, width, height, distance)?;
            let message = device::swipe(serial, start, end, 380).await?;
            Ok(json!({ "message": message, "direction": direction }))
        }
        "input_text" => {
            let text = step
                .args
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| "input_text 缺少 text".to_string())?;
            let clear = step
                .args
                .get("clear")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let message = device::input_text(serial, text, clear).await?;
            Ok(json!({ "message": message, "text": text }))
        }
        "press_key" => {
            let key = step
                .args
                .get("key")
                .and_then(Value::as_str)
                .unwrap_or("back");
            let message = device::press_key(serial, key).await?;
            Ok(json!({ "message": message, "key": key }))
        }
        "start_app" => {
            let package = step
                .args
                .get("package")
                .and_then(Value::as_str)
                .ok_or_else(|| "start_app 缺少 package".to_string())?;
            let message = device::start_app(serial, package).await?;
            Ok(json!({ "message": message, "package": package }))
        }
        "wait" => {
            let milliseconds = step.args.get("ms").and_then(Value::as_u64).unwrap_or(700);
            let message = device::wait_ms(milliseconds).await;
            Ok(json!({ "message": message }))
        }
        other => Err(format!("测试回放不支持动作：{other}")),
    }
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

fn find_node_for_step<'a>(
    observation: &'a Observation,
    step: &crate::models::TestCaseStep,
    plugin: &AppKnowledgePlugin,
) -> Option<&'a UiNode> {
    let args_element_key = step
        .args
        .get("elementKey")
        .and_then(Value::as_str)
        .unwrap_or_default();
    find_node_by_key(observation, args_element_key)
        .or_else(|| find_node_from_knowledge(observation, step, plugin, args_element_key))
        .or_else(|| {
            step.args
                .get("text")
                .and_then(Value::as_str)
                .and_then(|text| device::find_node_by_text(&observation.nodes, text))
        })
        .or_else(|| find_node_by_key(observation, &step.element_key))
        .or_else(|| find_node_from_knowledge(observation, step, plugin, &step.element_key))
        .or_else(|| {
            step.args
                .get("resource_id")
                .and_then(Value::as_str)
                .filter(|resource_id| is_specific_resource_id(resource_id))
                .and_then(|resource_id| device::find_node_by_id(&observation.nodes, resource_id))
        })
        .or_else(|| device::find_node_by_text(&observation.nodes, &step.label))
        .or_else(|| find_node_by_legacy_label(&observation.nodes, &step.label))
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
