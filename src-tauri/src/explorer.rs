use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::json;
use tauri::{AppHandle, Emitter};

use crate::device;
use crate::knowledge::{KnowledgeStore, element_key};
use crate::models::{
    AgentEvent, AppKnowledgePlugin, ExplorationReport, KnowledgePage, Observation, PageTransition,
    UiNode,
};

const MAX_SCROLLS_PER_PAGE: u32 = 3;
const EXPLORATION_BLOCKLIST: &[&str] = &[
    "立即购买",
    "确认购买",
    "提交订单",
    "支付",
    "付款",
    "发布",
    "发送",
    "删除",
    "注销",
    "退出登录",
    "举报",
    "拉黑",
];

pub struct ExplorerDeps {
    pub app: Option<AppHandle>,
    pub knowledge: Arc<KnowledgeStore>,
    pub artifacts_dir: PathBuf,
    pub cancellation: Arc<AtomicBool>,
}

pub async fn run_exploration(
    deps: ExplorerDeps,
    run_id: String,
    device_serial: String,
    package: String,
    max_steps: u32,
) -> Result<ExplorationReport, String> {
    let max_steps = max_steps.clamp(1, 300);
    device::ensure_device_ready(&device_serial).await?;
    device::start_app(&device_serial, &package).await?;
    device::wait_ms(1_200).await;

    let before_plugin = deps.knowledge.get_plugin(&package)?;
    let known_actions = deps.knowledge.known_action_keys(&package)?;
    let mut explored = HashSet::<String>::new();
    let mut scrolls = HashMap::<String, u32>::new();
    let mut navigation = Vec::<String>::new();
    let mut attempted_routes = HashSet::<String>::new();
    let mut recovery_attempts = HashMap::<String, u32>::new();
    let mut last_page_id = String::new();
    let mut completed_steps = 0_u32;
    let mut summary = String::new();

    emit(
        deps.app.as_ref(),
        &run_id,
        "exploration_started",
        "开始自动遍历",
        &format!("目标 {package}，最多 {max_steps} 步"),
        0,
        Some(json!({ "package": package })),
    );

    let mut observation = capture(&deps, &device_serial, &run_id, "explore-000-initial").await?;

    while completed_steps < max_steps {
        if deps.cancellation.load(Ordering::Relaxed) {
            summary = "自动遍历已停止".to_string();
            break;
        }
        if observation.package != package {
            if let Some(choice) = permission_choice(&observation) {
                let (x, y) = choice.bounds.center();
                device::tap(&device_serial, x, y).await?;
                device::wait_ms(650).await;
                completed_steps += 1;
                emit(
                    deps.app.as_ref(),
                    &run_id,
                    "exploration_permission",
                    "处理系统弹窗",
                    &format!("选择「{}」后继续", choice.text.trim()),
                    completed_steps,
                    Some(json!({
                        "package": observation.package,
                        "choice": choice.text,
                    })),
                );
                observation = capture(
                    &deps,
                    &device_serial,
                    &run_id,
                    &label("permission", completed_steps),
                )
                .await?;
                continue;
            }
            device::start_app(&device_serial, &package).await?;
            device::wait_ms(900).await;
            observation = capture(
                &deps,
                &device_serial,
                &run_id,
                &label("restore", completed_steps),
            )
            .await?;
            navigation.clear();
        }

        if last_page_id != observation.page_id {
            emit(
                deps.app.as_ref(),
                &run_id,
                "exploration_page",
                "发现页面",
                &format!(
                    "{} · {} 个可交互元素",
                    short_page_id(&observation.page_id),
                    interaction_count(&observation)
                ),
                completed_steps,
                Some(json!({
                    "pageId": observation.page_id,
                    "package": observation.package,
                    "activity": observation.activity,
                    "screenshotDataUrl": observation.screenshot_data_url,
                    "nodes": observation.nodes.len()
                })),
            );
            last_page_id = observation.page_id.clone();
        }

        if let Some(candidate) = select_candidate(&observation, &explored, &known_actions) {
            let before = observation.clone();
            let exploration_key = format!("{}:{}", before.page_id, candidate.key);
            explored.insert(exploration_key);
            let (x, y) = candidate.node.bounds.center();
            device::tap(&device_serial, x, y).await?;
            device::wait_ms(850).await;
            let after = capture(
                &deps,
                &device_serial,
                &run_id,
                &label("after", completed_steps + 1),
            )
            .await?;
            let changed = before.page_id != after.page_id;
            deps.knowledge.record_action(
                &run_id,
                completed_steps + 1,
                &before,
                &after,
                "tap_text",
                &json!({
                    "text": candidate.label,
                    "x": x,
                    "y": y,
                    "elementKey": candidate.key
                }),
                &json!({
                    "message": format!("自动遍历点击 {}", candidate.label),
                    "pageChanged": changed
                }),
                true,
            )?;
            if changed && navigation.last() != Some(&before.page_id) {
                navigation.push(before.page_id.clone());
            }
            completed_steps += 1;
            emit(
                deps.app.as_ref(),
                &run_id,
                "exploration_action",
                "执行探索操作",
                &format!(
                    "{} → {}",
                    candidate.label,
                    if changed { "新页面" } else { "当前页面" }
                ),
                completed_steps,
                Some(json!({
                    "fromPageId": before.page_id,
                    "toPageId": after.page_id,
                    "elementKey": candidate.key,
                    "label": candidate.label,
                    "changed": changed
                })),
            );
            observation = after;
            continue;
        }

        let plugin = deps.knowledge.get_plugin(&package)?;
        if let Some(route) = find_route_to_unexplored(
            &plugin,
            &observation.page_id,
            &known_actions,
            &attempted_routes,
        ) {
            let transition = route[0].clone();
            attempted_routes.insert(transition.id.clone());
            if let Some(target) = find_transition_target(&observation.nodes, &transition) {
                let before = observation.clone();
                let (x, y) = target.bounds.center();
                device::tap(&device_serial, x, y).await?;
                device::wait_ms(850).await;
                let after = capture(
                    &deps,
                    &device_serial,
                    &run_id,
                    &label("route", completed_steps + 1),
                )
                .await?;
                let result = json!({
                    "message": format!("沿已验证路径前往 {}", transition.trigger_label),
                    "fromPageId": before.page_id,
                    "toPageId": after.page_id,
                    "targetPageId": route.last().map(|edge| edge.to_page_id.clone()),
                    "elementKey": transition.trigger_key,
                });
                deps.knowledge.record_action(
                    &run_id,
                    completed_steps + 1,
                    &before,
                    &after,
                    &transition.tool,
                    &transition.args,
                    &result,
                    before.page_id == after.page_id || after.page_id == transition.to_page_id,
                )?;
                if before.page_id != after.page_id && navigation.last() != Some(&before.page_id) {
                    navigation.push(before.page_id.clone());
                }
                completed_steps += 1;
                emit(
                    deps.app.as_ref(),
                    &run_id,
                    "exploration_route",
                    "导航到未探索页面",
                    &format!(
                        "{} → {} · 目标 {}",
                        short_page_id(&before.page_id),
                        short_page_id(&after.page_id),
                        short_page_id(
                            route
                                .last()
                                .map(|edge| edge.to_page_id.as_str())
                                .unwrap_or("")
                        )
                    ),
                    completed_steps,
                    Some(json!({
                        "transitionId": transition.id,
                        "fromPageId": before.page_id,
                        "toPageId": after.page_id,
                        "targetPageId": route.last().map(|edge| edge.to_page_id.clone()),
                        "label": transition.trigger_label
                    })),
                );
                observation = after;
                continue;
            }
        }

        if let Some(previous_page) = navigation.pop() {
            device::press_key(&device_serial, "back").await?;
            device::wait_ms(650).await;
            let after = capture(
                &deps,
                &device_serial,
                &run_id,
                &label("back", completed_steps + 1),
            )
            .await?;
            completed_steps += 1;
            emit(
                deps.app.as_ref(),
                &run_id,
                "exploration_back",
                "返回上层页面",
                &format!(
                    "{} → {}",
                    short_page_id(&observation.page_id),
                    short_page_id(&after.page_id)
                ),
                completed_steps,
                Some(json!({
                    "expectedPageId": previous_page,
                    "actualPageId": after.page_id
                })),
            );
            if after.package != package {
                device::start_app(&device_serial, &package).await?;
                device::wait_ms(900).await;
                observation = capture(
                    &deps,
                    &device_serial,
                    &run_id,
                    &label("restore", completed_steps),
                )
                .await?;
                navigation.clear();
            } else {
                observation = after;
            }
            continue;
        }

        let scroll_count = scrolls.entry(observation.page_id.clone()).or_default();
        if *scroll_count < MAX_SCROLLS_PER_PAGE
            && observation.nodes.iter().any(|node| node.scrollable)
        {
            *scroll_count += 1;
            let before = observation.clone();
            let width = before.width.max(1);
            let height = before.height.max(1);
            device::swipe(
                &device_serial,
                (width / 2, height * 3 / 4),
                (width / 2, height / 3),
                420,
            )
            .await?;
            device::wait_ms(700).await;
            let after = capture(
                &deps,
                &device_serial,
                &run_id,
                &label("scroll", completed_steps + 1),
            )
            .await?;
            deps.knowledge.record_action(
                &run_id,
                completed_steps + 1,
                &before,
                &after,
                "swipe",
                &json!({ "direction": "up", "distance": 0.42, "duration_ms": 420 }),
                &json!({ "message": "自动遍历滚动页面", "pageChanged": before.page_id != after.page_id }),
                true,
            )?;
            completed_steps += 1;
            observation = after;
            emit(
                deps.app.as_ref(),
                &run_id,
                "exploration_scroll",
                "滚动探索",
                &format!("第 {scroll_count}/{MAX_SCROLLS_PER_PAGE} 次滚动"),
                completed_steps,
                Some(json!({ "pageId": observation.page_id })),
            );
            continue;
        }

        // A page can be a dead end after its visible controls are exhausted.
        // Restart the app once to return to the known root, then let the page
        // graph route the explorer toward another frontier.
        let attempts = recovery_attempts
            .entry(observation.page_id.clone())
            .or_default();
        if *attempts < 1 {
            *attempts += 1;
            device::start_app(&device_serial, &package).await?;
            device::wait_ms(1_000).await;
            let after = capture(
                &deps,
                &device_serial,
                &run_id,
                &label("recover-root", completed_steps + 1),
            )
            .await?;
            completed_steps += 1;
            navigation.clear();
            scrolls.clear();
            emit(
                deps.app.as_ref(),
                &run_id,
                "exploration_recovery",
                "回到 App 根页面",
                &format!(
                    "{} → {}，继续寻找可达的未知前沿",
                    short_page_id(&observation.page_id),
                    short_page_id(&after.page_id)
                ),
                completed_steps,
                Some(json!({
                    "fromPageId": observation.page_id,
                    "toPageId": after.page_id,
                    "reason": "exhausted-current-page"
                })),
            );
            observation = after;
            continue;
        }

        summary = format!(
            "当前页面没有更多未探索控件，且已尝试回到根页面；已完成 {completed_steps} 步，保留已有页面图。"
        );
        break;
    }

    if summary.is_empty() {
        summary = format!("达到自动遍历步数上限 {max_steps}，已保存全部页面与跳转关系。");
    }
    let tests = deps
        .knowledge
        .generate_test_cases(&package)
        .unwrap_or_default();
    let after_plugin = deps.knowledge.get_plugin(&package)?;
    let report = ExplorationReport {
        package: package.clone(),
        status: if deps.cancellation.load(Ordering::Relaxed) {
            "cancelled".to_string()
        } else {
            "completed".to_string()
        },
        steps: completed_steps,
        pages_discovered: after_plugin
            .pages
            .len()
            .saturating_sub(before_plugin.pages.len()),
        transitions_discovered: after_plugin
            .transitions
            .len()
            .saturating_sub(before_plugin.transitions.len()),
        summary: format!(
            "{summary} 共 {} 个页面、{} 条跳转边、{} 条生成用例。",
            after_plugin.pages.len(),
            after_plugin.transitions.len(),
            tests.len()
        ),
    };
    emit(
        deps.app.as_ref(),
        &run_id,
        if report.status == "cancelled" {
            "exploration_cancelled"
        } else {
            "exploration_completed"
        },
        "自动遍历结束",
        &report.summary,
        completed_steps,
        Some(json!({
            "report": report,
            "package": package,
            "testCount": tests.len()
        })),
    );
    Ok(report)
}

fn find_route_to_unexplored<'a>(
    plugin: &'a AppKnowledgePlugin,
    current_page_id: &str,
    known_actions: &HashSet<String>,
    attempted_routes: &HashSet<String>,
) -> Option<Vec<&'a PageTransition>> {
    let page_by_id = plugin
        .pages
        .iter()
        .map(|page| (page.id.as_str(), page))
        .collect::<HashMap<_, _>>();
    let mut edges_by_page = HashMap::<&str, Vec<&PageTransition>>::new();
    for transition in plugin
        .transitions
        .iter()
        .filter(|transition| transition.success)
    {
        if attempted_routes.contains(&transition.id) {
            continue;
        }
        edges_by_page
            .entry(transition.from_page_id.as_str())
            .or_default()
            .push(transition);
    }

    let mut queue = VecDeque::from([current_page_id]);
    let mut visited = HashSet::from([current_page_id]);
    let mut parent = HashMap::<&str, &PageTransition>::new();
    while let Some(page_id) = queue.pop_front() {
        if page_id != current_page_id
            && page_by_id
                .get(page_id)
                .is_some_and(|page| page_has_unexplored(page, known_actions))
        {
            let mut route = Vec::new();
            let mut cursor = page_id;
            while cursor != current_page_id {
                let edge = parent.get(cursor)?;
                route.push(*edge);
                cursor = edge.from_page_id.as_str();
            }
            route.reverse();
            return Some(route);
        }
        for edge in edges_by_page.get(page_id).into_iter().flatten() {
            let next = edge.to_page_id.as_str();
            if visited.insert(next) {
                parent.insert(next, edge);
                queue.push_back(next);
            }
        }
    }
    None
}

fn page_has_unexplored(page: &KnowledgePage, known_actions: &HashSet<String>) -> bool {
    page.elements
        .iter()
        .filter(|element| element.clickable || element.scrollable || element.focusable)
        .filter_map(|element| {
            let label = if !element.text.trim().is_empty() {
                element.text.trim()
            } else if !element.content_description.trim().is_empty() {
                element.content_description.trim()
            } else {
                return None;
            };
            if EXPLORATION_BLOCKLIST
                .iter()
                .any(|term| label.contains(term))
            {
                return None;
            }
            Some((
                format!("{}:{}", page.id, element.key),
                format!("{}:text:{}", page.id, label),
                if element.resource_id.trim().is_empty() {
                    String::new()
                } else {
                    format!("{}:id:{}", page.id, element.resource_id.trim())
                },
            ))
        })
        .any(|(element_key, text_key, id_key)| {
            !known_actions.contains(&element_key)
                && !known_actions.contains(&text_key)
                && (id_key.is_empty() || !known_actions.contains(&id_key))
        })
}

fn find_transition_target<'a>(
    nodes: &'a [UiNode],
    transition: &PageTransition,
) -> Option<&'a UiNode> {
    let element_key = transition
        .args
        .get("elementKey")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    nodes
        .iter()
        .filter(|node| node.is_actionable())
        .find(|node| {
            (!element_key.is_empty() && crate::knowledge::element_key(node) == element_key)
                || (!transition.trigger_key.is_empty()
                    && crate::knowledge::element_key(node) == transition.trigger_key)
        })
        .or_else(|| {
            transition
                .args
                .get("text")
                .and_then(serde_json::Value::as_str)
                .and_then(|text| device::find_node_by_text(nodes, text))
        })
        .or_else(|| {
            transition
                .args
                .get("resource_id")
                .and_then(serde_json::Value::as_str)
                .and_then(|resource_id| device::find_node_by_id(nodes, resource_id))
        })
        .or_else(|| device::find_node_by_text(nodes, &transition.trigger_label))
}

struct Candidate<'a> {
    node: &'a UiNode,
    key: String,
    label: String,
}

fn select_candidate<'a>(
    observation: &'a Observation,
    explored: &HashSet<String>,
    known_actions: &HashSet<String>,
) -> Option<Candidate<'a>> {
    let mut candidates = observation
        .nodes
        .iter()
        .filter(|node| node.clickable && node.visible && node.enabled && node.is_actionable())
        .filter(|node| node.package.is_empty() || node.package == observation.package)
        .filter(|node| node.bounds.top > 20)
        .filter(|node| {
            observation.height <= 0 || node.bounds.bottom < observation.height * 97 / 100
        })
        .filter(|node| !node.class_name.contains("EditText"))
        .filter_map(|node| {
            let label = node_label(node)?;
            if EXPLORATION_BLOCKLIST
                .iter()
                .any(|term| label.contains(term))
            {
                return None;
            }
            let key = element_key(node);
            let exploration_key = format!("{}:{}", observation.page_id, key);
            if explored.contains(&exploration_key) {
                return None;
            }
            let text_key = format!("{}:text:{}", observation.page_id, label);
            let id_key = if node.resource_id.trim().is_empty() {
                String::new()
            } else {
                format!("{}:id:{}", observation.page_id, node.resource_id.trim())
            };
            if known_actions.contains(&exploration_key)
                || known_actions.contains(&text_key)
                || (!id_key.is_empty() && known_actions.contains(&id_key))
            {
                return None;
            }
            Some(Candidate { node, key, label })
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|candidate| {
        (
            if candidate.node.resource_id.is_empty() {
                1
            } else {
                0
            },
            candidate.node.bounds.top,
            candidate.node.bounds.left,
            -candidate.node.bounds.area(),
        )
    });
    candidates.into_iter().next()
}

fn node_label(node: &UiNode) -> Option<String> {
    let value = if !node.text.trim().is_empty() {
        node.text.trim()
    } else if !node.content_description.trim().is_empty() {
        node.content_description.trim()
    } else {
        let id = node
            .resource_id
            .trim()
            .strip_prefix("android:id/")
            .or_else(|| node.resource_id.trim().rsplit('/').next())
            .unwrap_or_default();
        if !id.is_empty() {
            return Some(format!("id:{id}"));
        }
        let class = node.class_name.rsplit('.').next().unwrap_or("控件");
        return Some(format!("{class}#{}", node.index));
    };
    let label = value.lines().map(str::trim).find(|line| {
        !line.is_empty()
            && !(line.starts_with("第") && line.contains("标签"))
            && !line.contains(":id/")
    })?;
    Some(label.chars().take(100).collect())
}

fn interaction_count(observation: &Observation) -> usize {
    observation
        .nodes
        .iter()
        .filter(|node| node.clickable && node.visible && node.enabled)
        .count()
}

fn permission_choice(observation: &Observation) -> Option<&UiNode> {
    const CHOICES: &[&str] = &["仅在使用中允许", "本次运行允许", "始终允许", "允许", "拒绝"];
    const DIALOG_PACKAGES: &[&str] = &[
        "permissioncontroller",
        "com.lbe.security",
        "com.android.settings",
    ];
    if !DIALOG_PACKAGES
        .iter()
        .any(|package| observation.package.contains(package))
    {
        return None;
    }
    CHOICES
        .iter()
        .find_map(|choice| device::find_node_by_text(&observation.nodes, choice))
}

async fn capture(
    deps: &ExplorerDeps,
    device_serial: &str,
    run_id: &str,
    label: &str,
) -> Result<Observation, String> {
    let mut observation =
        device::capture_observation(device_serial, &deps.artifacts_dir, run_id, label).await?;
    deps.knowledge.register_observation_mut(&mut observation)?;
    Ok(observation)
}

fn label(prefix: &str, step: u32) -> String {
    format!("explore-{step:03}-{prefix}")
}

fn short_page_id(page_id: &str) -> &str {
    &page_id[..page_id.len().min(10)]
}

fn emit(
    app: Option<&AppHandle>,
    run_id: &str,
    event_type: &str,
    title: &str,
    detail: &str,
    step_index: u32,
    data: Option<serde_json::Value>,
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
            timestamp: chrono::Utc::now().to_rfc3339(),
            data,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Bounds;

    fn observation() -> Observation {
        Observation {
            device_serial: "test".to_string(),
            screenshot_data_url: String::new(),
            model_screenshot_data_url: None,
            model_screenshot_path: None,
            model_screenshot_error: None,
            screenshot_path: String::new(),
            ui_path: String::new(),
            nodes: vec![
                UiNode {
                    index: 0,
                    text: "搜索".to_string(),
                    content_description: String::new(),
                    resource_id: "com.example:id/search".to_string(),
                    class_name: "android.widget.TextView".to_string(),
                    package: "com.example".to_string(),
                    bounds: Bounds {
                        left: 20,
                        top: 100,
                        right: 200,
                        bottom: 180,
                    },
                    clickable: true,
                    enabled: true,
                    focusable: true,
                    scrollable: false,
                    selected: false,
                    visible: true,
                    depth: 2,
                },
                UiNode {
                    index: 1,
                    text: "立即购买".to_string(),
                    content_description: String::new(),
                    resource_id: "com.example:id/buy".to_string(),
                    class_name: "android.widget.Button".to_string(),
                    package: "com.example".to_string(),
                    bounds: Bounds {
                        left: 20,
                        top: 300,
                        right: 200,
                        bottom: 380,
                    },
                    clickable: true,
                    enabled: true,
                    focusable: true,
                    scrollable: false,
                    selected: false,
                    visible: true,
                    depth: 2,
                },
            ],
            package: "com.example".to_string(),
            activity: "com.example.MainActivity".to_string(),
            page_id: "page-1".to_string(),
            page_signature: "signature".to_string(),
            captured_at: String::new(),
            width: 1080,
            height: 2400,
            ocr: None,
            ocr_path: None,
            ocr_error: None,
        }
    }

    #[test]
    fn skips_blocked_actions_and_already_explored_elements() {
        let observation = observation();
        let first =
            select_candidate(&observation, &HashSet::new(), &HashSet::new()).expect("candidate");
        assert_eq!(first.label, "搜索");
        let explored = HashSet::from([format!("page-1:{}", first.key)]);
        assert!(select_candidate(&observation, &explored, &HashSet::new()).is_none());
    }
}
