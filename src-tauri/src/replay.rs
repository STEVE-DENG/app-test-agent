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
    let plugin = deps.knowledge.get_plugin(&package)?;
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
        let result = execute_step(&device_serial, &before, step).await;
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
) -> Result<Value, String> {
    let node = find_node(observation, &step.element_key)
        .or_else(|| device::find_node_by_text(&observation.nodes, &step.label));
    match step.action.as_str() {
        "tap_text" | "tap_id" | "tap" => {
            let target = node.ok_or_else(|| format!("当前页面找不到控件：{}", step.label))?;
            let (x, y) = target.bounds.center();
            let message = device::tap(serial, x, y).await?;
            Ok(json!({
                "message": message,
                "elementKey": step.element_key,
                "label": step.label,
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

fn find_node<'a>(observation: &'a Observation, key: &str) -> Option<&'a UiNode> {
    observation
        .nodes
        .iter()
        .filter(|node| node.is_actionable())
        .find(|node| element_key(node) == key)
}

fn same_page(plugin: &AppKnowledgePlugin, expected_id: &str, actual_id: &str) -> bool {
    if expected_id == actual_id {
        return true;
    }
    let expected = plugin.pages.iter().find(|page| page.id == expected_id);
    let actual = plugin.pages.iter().find(|page| page.id == actual_id);
    matches!(
        (expected, actual),
        (Some(left), Some(right))
            if left.activity == right.activity && left.name == right.name
    )
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
    let observation =
        device::capture_observation(device_serial, &deps.artifacts_dir, run_id, label).await?;
    deps.knowledge.register_observation(&observation)?;
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
