use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::Utc;
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::models::{
    ActionTrace, AppKnowledgePlugin, AppPluginSummary, AppTestCase, KnowledgeElement,
    KnowledgePage, Observation, PageTransition, TestCaseStep, UiNode,
};

const SCHEMA_VERSION: u32 = 1;
const MAX_ELEMENTS_PER_PAGE: usize = 360;
const MAX_ACTIONS: usize = 5_000;

pub struct KnowledgeStore {
    root: PathBuf,
    lock: Mutex<()>,
}

impl KnowledgeStore {
    pub fn new(data_dir: &Path) -> Result<Self, String> {
        let root = data_dir.join("plugins");
        fs::create_dir_all(&root).map_err(|error| format!("创建 App 插件目录失败: {error}"))?;
        Ok(Self {
            root,
            lock: Mutex::new(()),
        })
    }

    pub fn list_plugins(&self) -> Result<Vec<AppPluginSummary>, String> {
        let _guard = self.lock()?;
        let mut plugins = Vec::new();
        let entries =
            fs::read_dir(&self.root).map_err(|error| format!("读取 App 插件目录失败: {error}"))?;
        for entry in entries {
            let entry = entry.map_err(|error| format!("读取 App 插件失败: {error}"))?;
            let path = entry.path().join("plugin.json");
            if !path.is_file() {
                continue;
            }
            if let Ok(plugin) = read_plugin(&path) {
                plugins.push(summary(&plugin));
            }
        }
        plugins.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        Ok(plugins)
    }

    pub fn get_plugin(&self, package: &str) -> Result<AppKnowledgePlugin, String> {
        let _guard = self.lock()?;
        self.load_or_create(package)
    }

    pub fn known_action_keys(&self, package: &str) -> Result<HashSet<String>, String> {
        let _guard = self.lock()?;
        let plugin = self.load_or_create(package)?;
        let mut keys = HashSet::new();
        for action in plugin.actions.iter().filter(|action| action.success) {
            let element_key = action
                .args
                .get("elementKey")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let text = action
                .args
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let resource_id = action
                .args
                .get("resource_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !element_key.is_empty() {
                keys.insert(format!("{}:{}", action.from_page_id, element_key));
            }
            if !text.is_empty() {
                keys.insert(format!("{}:text:{}", action.from_page_id, text));
            }
            if !resource_id.is_empty() {
                keys.insert(format!("{}:id:{}", action.from_page_id, resource_id));
            }
        }
        Ok(keys)
    }

    pub fn export_plugin(&self, package: &str) -> Result<String, String> {
        let _guard = self.lock()?;
        let plugin = self.load_or_create(package)?;
        serde_json::to_string_pretty(&plugin).map_err(|error| format!("导出 App 插件失败: {error}"))
    }

    pub fn compact_plugin(&self, package: &str) -> Result<AppPluginSummary, String> {
        let _guard = self.lock()?;
        let mut plugin = self.load_or_create(package)?;
        compact_duplicate_pages(&mut plugin);
        plugin.updated_at = Utc::now().to_rfc3339();
        self.save(&plugin)?;
        Ok(summary(&plugin))
    }

    pub fn import_plugin(&self, raw: &str) -> Result<AppPluginSummary, String> {
        let incoming: AppKnowledgePlugin =
            serde_json::from_str(raw).map_err(|error| format!("解析 App 插件失败: {error}"))?;
        if incoming.package.trim().is_empty() {
            return Err("插件缺少 package".to_string());
        }
        if incoming.schema_version != SCHEMA_VERSION {
            return Err(format!(
                "不支持的插件版本 {}，当前版本 {}",
                incoming.schema_version, SCHEMA_VERSION
            ));
        }

        let _guard = self.lock()?;
        let mut merged = if self.path_for(&incoming.package).exists() {
            self.load_or_create(&incoming.package)?
        } else {
            AppKnowledgePlugin {
                schema_version: SCHEMA_VERSION,
                package: incoming.package.clone(),
                name: incoming.name.clone(),
                created_at: incoming.created_at.clone(),
                updated_at: incoming.updated_at.clone(),
                pages: Vec::new(),
                transitions: Vec::new(),
                actions: Vec::new(),
                test_cases: Vec::new(),
            }
        };
        merge_plugins(&mut merged, incoming);
        merged.updated_at = Utc::now().to_rfc3339();
        self.save(&merged)?;
        Ok(summary(&merged))
    }

    pub fn register_observation(&self, observation: &Observation) -> Result<KnowledgePage, String> {
        if observation.package.trim().is_empty() {
            return Err("无法识别当前前台应用包名".to_string());
        }

        let _guard = self.lock()?;
        let mut plugin = self.load_or_create(&observation.package)?;
        let page = upsert_page(&mut plugin, observation);
        plugin.updated_at = Utc::now().to_rfc3339();
        self.save(&plugin)?;
        Ok(page)
    }

    pub fn register_observation_mut(
        &self,
        observation: &mut Observation,
    ) -> Result<KnowledgePage, String> {
        let page = self.register_observation(observation)?;
        observation.page_id = page.id.clone();
        observation.page_signature = page.signature.clone();
        Ok(page)
    }

    pub fn record_action(
        &self,
        run_id: &str,
        step_index: u32,
        before: &Observation,
        after: &Observation,
        tool: &str,
        args: &Value,
        result: &Value,
        success: bool,
    ) -> Result<(), String> {
        if before.package.trim().is_empty() || after.package.trim().is_empty() {
            return Err("页面缺少包名，无法记录动作".to_string());
        }
        if before.package != after.package {
            return Ok(());
        }

        let _guard = self.lock()?;
        let mut plugin = self.load_or_create(&before.package)?;
        let before_page = upsert_page(&mut plugin, before);
        let after_page = upsert_page(&mut plugin, after);
        let trigger = find_trigger_element(&before.nodes, args)
            .map(|node| (element_key(node), element_label(node)))
            .unwrap_or_else(|| (String::new(), tool.to_string()));
        let now = Utc::now().to_rfc3339();

        if before_page.id != after_page.id {
            let transition = PageTransition {
                id: Uuid::new_v4().to_string(),
                from_page_id: before_page.id.clone(),
                to_page_id: after_page.id.clone(),
                trigger_key: trigger.0.clone(),
                trigger_label: trigger.1.clone(),
                tool: tool.to_string(),
                args: args.clone(),
                result_summary: result_summary(result),
                success,
                observed_at: now.clone(),
            };
            if !plugin.transitions.iter().any(|existing| {
                existing.from_page_id == transition.from_page_id
                    && existing.to_page_id == transition.to_page_id
                    && existing.trigger_key == transition.trigger_key
                    && existing.tool == transition.tool
            }) {
                plugin.transitions.push(transition);
            }
        }

        plugin.actions.push(ActionTrace {
            id: Uuid::new_v4().to_string(),
            run_id: run_id.to_string(),
            step_index,
            from_page_id: before_page.id,
            to_page_id: after_page.id,
            tool: tool.to_string(),
            args: args.clone(),
            result: result.clone(),
            success,
            screenshot_before: before.screenshot_path.clone(),
            screenshot_after: after.screenshot_path.clone(),
            created_at: now,
        });
        if plugin.actions.len() > MAX_ACTIONS {
            let drain = plugin.actions.len() - MAX_ACTIONS;
            plugin.actions.drain(0..drain);
        }
        plugin.updated_at = Utc::now().to_rfc3339();
        self.save(&plugin)
    }

    pub fn generate_test_cases(&self, package: &str) -> Result<Vec<AppTestCase>, String> {
        let _guard = self.lock()?;
        let mut plugin = self.load_or_create(package)?;
        let start_page = choose_start_page(&plugin)
            .ok_or_else(|| "当前 App 还没有页面数据".to_string())?
            .clone();
        let mut adjacency = HashMap::<String, Vec<&PageTransition>>::new();
        for transition in plugin
            .transitions
            .iter()
            .filter(|transition| transition.success)
        {
            adjacency
                .entry(transition.from_page_id.clone())
                .or_default()
                .push(transition);
        }

        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();
        let mut parents = HashMap::<String, &PageTransition>::new();
        queue.push_back(start_page.id.clone());
        visited.insert(start_page.id.clone());
        while let Some(page_id) = queue.pop_front() {
            if let Some(edges) = adjacency.get(&page_id) {
                for edge in edges {
                    if visited.insert(edge.to_page_id.clone()) {
                        parents.insert(edge.to_page_id.clone(), edge);
                        queue.push_back(edge.to_page_id.clone());
                    }
                }
            }
        }

        let now = Utc::now().to_rfc3339();
        let mut generated = Vec::new();
        for target_page_id in visited.iter() {
            if target_page_id == &start_page.id {
                continue;
            }
            let mut path = Vec::new();
            let mut cursor = target_page_id.clone();
            while cursor != start_page.id {
                let Some(edge) = parents.get(&cursor) else {
                    path.clear();
                    break;
                };
                path.push((*edge).clone());
                cursor = edge.from_page_id.clone();
            }
            path.reverse();
            if path.is_empty() {
                continue;
            }
            let steps = path
                .iter()
                .map(|edge| TestCaseStep {
                    from_page_id: edge.from_page_id.clone(),
                    expected_page_id: edge.to_page_id.clone(),
                    element_key: edge.trigger_key.clone(),
                    label: edge.trigger_label.clone(),
                    action: edge.tool.clone(),
                    args: edge.args.clone(),
                })
                .collect::<Vec<_>>();
            let target_name = plugin
                .pages
                .iter()
                .find(|page| page.id == *target_page_id)
                .map(|page| page.name.clone())
                .unwrap_or_else(|| target_page_id.clone());
            let id = format!("{}:{}", package, target_page_id);
            let existing = plugin
                .test_cases
                .iter()
                .position(|test_case| test_case.id == id);
            let (created_at, status) = existing
                .map(|index| {
                    (
                        plugin.test_cases[index].created_at.clone(),
                        plugin.test_cases[index].status.clone(),
                    )
                })
                .unwrap_or_else(|| (now.clone(), "generated".to_string()));
            let test_case = AppTestCase {
                id: id.clone(),
                name: format!("从 {} 导航到 {}", start_page.name, target_name),
                package: package.to_string(),
                start_page_id: start_page.id.clone(),
                target_page_id: target_page_id.clone(),
                steps,
                status,
                last_result: existing
                    .map(|index| plugin.test_cases[index].last_result.clone())
                    .unwrap_or_default(),
                created_at,
                updated_at: now.clone(),
            };
            if let Some(index) = existing {
                plugin.test_cases[index] = test_case.clone();
            } else {
                plugin.test_cases.push(test_case.clone());
            }
            generated.push(test_case);
        }
        plugin.updated_at = now;
        self.save(&plugin)?;
        generated.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(generated)
    }

    pub fn get_test_case(&self, package: &str, test_id: &str) -> Result<AppTestCase, String> {
        let _guard = self.lock()?;
        let plugin = self.load_or_create(package)?;
        plugin
            .test_cases
            .into_iter()
            .find(|test_case| test_case.id == test_id)
            .ok_or_else(|| format!("找不到测试用例 {test_id}"))
    }

    pub fn set_test_result(
        &self,
        package: &str,
        test_id: &str,
        status: &str,
        result: &str,
    ) -> Result<(), String> {
        let _guard = self.lock()?;
        let mut plugin = self.load_or_create(package)?;
        let test_case = plugin
            .test_cases
            .iter_mut()
            .find(|test_case| test_case.id == test_id)
            .ok_or_else(|| format!("找不到测试用例 {test_id}"))?;
        test_case.status = status.to_string();
        test_case.last_result = result.chars().take(2_000).collect();
        test_case.updated_at = Utc::now().to_rfc3339();
        plugin.updated_at = Utc::now().to_rfc3339();
        self.save(&plugin)
    }

    fn load_or_create(&self, package: &str) -> Result<AppKnowledgePlugin, String> {
        if package.trim().is_empty() {
            return Err("包名不能为空".to_string());
        }
        let path = self.path_for(package);
        if path.exists() {
            read_plugin(&path)
        } else {
            let now = Utc::now().to_rfc3339();
            Ok(AppKnowledgePlugin {
                schema_version: SCHEMA_VERSION,
                package: package.to_string(),
                name: package.to_string(),
                created_at: now.clone(),
                updated_at: now,
                pages: Vec::new(),
                transitions: Vec::new(),
                actions: Vec::new(),
                test_cases: Vec::new(),
            })
        }
    }

    fn save(&self, plugin: &AppKnowledgePlugin) -> Result<(), String> {
        let directory = self.directory_for(&plugin.package);
        fs::create_dir_all(&directory)
            .map_err(|error| format!("创建 App 插件目录失败: {error}"))?;
        let path = directory.join("plugin.json");
        let temporary = directory.join("plugin.json.tmp");
        let raw = serde_json::to_string_pretty(plugin)
            .map_err(|error| format!("序列化 App 插件失败: {error}"))?;
        fs::write(&temporary, raw).map_err(|error| format!("写入 App 插件失败: {error}"))?;
        fs::rename(&temporary, &path).map_err(|error| format!("保存 App 插件失败: {error}"))
    }

    fn path_for(&self, package: &str) -> PathBuf {
        self.directory_for(package).join("plugin.json")
    }

    fn directory_for(&self, package: &str) -> PathBuf {
        self.root.join(sanitize_package(package))
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, ()>, String> {
        self.lock
            .lock()
            .map_err(|_| "App 插件存储锁已损坏".to_string())
    }
}

fn read_plugin(path: &Path) -> Result<AppKnowledgePlugin, String> {
    let raw = fs::read_to_string(path).map_err(|error| format!("读取 App 插件失败: {error}"))?;
    serde_json::from_str(&raw).map_err(|error| format!("解析 App 插件失败: {error}"))
}

fn summary(plugin: &AppKnowledgePlugin) -> AppPluginSummary {
    AppPluginSummary {
        package: plugin.package.clone(),
        name: plugin.name.clone(),
        pages: plugin.pages.len(),
        transitions: plugin.transitions.len(),
        actions: plugin.actions.len(),
        test_cases: plugin.test_cases.len(),
        updated_at: plugin.updated_at.clone(),
    }
}

fn upsert_page(plugin: &mut AppKnowledgePlugin, observation: &Observation) -> KnowledgePage {
    let elements = observation
        .nodes
        .iter()
        .filter(|node| {
            node.visible
                && (node.clickable
                    || node.scrollable
                    || node.focusable
                    || !node.text.trim().is_empty()
                    || !node.content_description.trim().is_empty()
                    || !node.resource_id.trim().is_empty())
        })
        .take(MAX_ELEMENTS_PER_PAGE)
        .map(to_knowledge_element)
        .collect::<Vec<_>>();
    let inferred_name = infer_page_name(observation);
    let now = Utc::now().to_rfc3339();
    let existing_index = plugin
        .pages
        .iter()
        .position(|page| page.id == observation.page_id)
        .or_else(|| find_similar_page_index(plugin, observation, &inferred_name, &elements));

    if let Some(index) = existing_index {
        let page = &mut plugin.pages[index];
        page.last_seen_at = now;
        page.visit_count += 1;
        page.screenshot_path = observation.screenshot_path.clone();
        page.ui_path = observation.ui_path.clone();
        page.width = observation.width;
        page.height = observation.height;
        if is_weak_page_name(&page.name) && !is_weak_page_name(&inferred_name) {
            page.name = inferred_name;
        }
        let mut merged = page
            .elements
            .drain(..)
            .map(|element| (element_slot_key(&element), element))
            .collect::<HashMap<_, _>>();
        for element in elements {
            let slot = element_slot_key(&element);
            if let Some(existing) = merged.get_mut(&slot) {
                let existing_key = existing.key.clone();
                *existing = element;
                // Keep the first observed key so existing transitions and
                // generated test cases remain stable across dynamic labels.
                existing.key = existing_key;
            } else {
                merged.insert(slot, element);
            }
        }
        page.elements = merged.into_values().collect();
        page.elements
            .sort_by_key(|element| (element.bounds.top, element.bounds.left));
        page.elements.truncate(MAX_ELEMENTS_PER_PAGE);
        return page.clone();
    }

    let page = KnowledgePage {
        id: observation.page_id.clone(),
        name: inferred_name,
        signature: observation.page_signature.clone(),
        package: observation.package.clone(),
        activity: observation.activity.clone(),
        screenshot_path: observation.screenshot_path.clone(),
        ui_path: observation.ui_path.clone(),
        width: observation.width,
        height: observation.height,
        first_seen_at: now.clone(),
        last_seen_at: now,
        visit_count: 1,
        elements,
    };
    plugin.pages.push(page.clone());
    if plugin.name == plugin.package {
        plugin.name = package_display_name(&observation.package);
    }
    page
}

fn find_similar_page_index(
    plugin: &AppKnowledgePlugin,
    observation: &Observation,
    inferred_name: &str,
    elements: &[KnowledgeElement],
) -> Option<usize> {
    let incoming = stable_element_tokens(elements);
    if incoming.len() < 2 {
        return None;
    }
    plugin
        .pages
        .iter()
        .enumerate()
        .filter(|(_, page)| {
            page.package == observation.package && page.activity == observation.activity
        })
        .filter_map(|(index, page)| {
            let existing = stable_element_tokens(&page.elements);
            if existing.len() < 2 {
                return None;
            }
            let intersection = incoming.intersection(&existing).count();
            let union = incoming.union(&existing).count();
            let similarity = intersection as f64 / union.max(1) as f64;
            let same_name = page.name == inferred_name;
            let both_weak = is_weak_page_name(&page.name) && is_weak_page_name(inferred_name);
            let (min_intersection, min_similarity) =
                if same_name && !is_weak_page_name(inferred_name) {
                    (8, 0.12)
                } else if both_weak {
                    (5, 0.35)
                } else {
                    (4, 0.45)
                };
            (intersection >= min_intersection && similarity >= min_similarity).then_some((
                index,
                intersection,
                similarity,
            ))
        })
        .max_by(|left, right| {
            left.1
                .cmp(&right.1)
                .then_with(|| left.2.total_cmp(&right.2))
        })
        .map(|(index, _, _)| index)
}

fn stable_element_tokens(elements: &[KnowledgeElement]) -> HashSet<String> {
    elements
        .iter()
        .filter_map(|element| {
            if !element.resource_id.trim().is_empty() {
                return Some(format!("id:{}", element.resource_id.trim()));
            }
            if !element.content_description.trim().is_empty() {
                return Some(format!(
                    "desc:{}|{}",
                    element.class_name.trim(),
                    element.content_description.trim()
                ));
            }
            let text = element.text.trim();
            if text.is_empty() || text.chars().all(|character| character.is_ascii_digit()) {
                None
            } else {
                Some(format!("text:{}|{}", element.class_name.trim(), text))
            }
        })
        .collect()
}

fn element_slot_key(element: &KnowledgeElement) -> String {
    if !element.resource_id.trim().is_empty() {
        return format!("id:{}", element.resource_id.trim());
    }
    if !element.content_description.trim().is_empty() {
        return format!(
            "desc:{}|{}",
            element.class_name.trim(),
            element.content_description.trim()
        );
    }
    format!(
        "slot:{}|{}|{}|{}",
        element.class_name.trim(),
        element.bounds.left,
        element.bounds.top,
        element.bounds.right
    )
}

fn to_knowledge_element(node: &UiNode) -> KnowledgeElement {
    KnowledgeElement {
        key: element_key(node),
        index: node.index,
        text: node.text.clone(),
        content_description: node.content_description.clone(),
        resource_id: node.resource_id.clone(),
        class_name: node.class_name.clone(),
        bounds: node.bounds.clone(),
        clickable: node.clickable,
        scrollable: node.scrollable,
        focusable: node.focusable,
    }
}

pub fn element_key(node: &UiNode) -> String {
    let semantic = if !node.resource_id.trim().is_empty() {
        format!(
            "id:{}|{}|{}",
            node.resource_id.trim(),
            node.text.trim(),
            node.content_description.trim()
        )
    } else if !node.text.trim().is_empty() || !node.content_description.trim().is_empty() {
        format!(
            "semantic:{}|{}|{}",
            node.class_name.trim(),
            node.text.trim(),
            node.content_description.trim()
        )
    } else {
        format!(
            "bounds:{}|{}|{}|{}",
            node.class_name.trim(),
            node.bounds.left,
            node.bounds.top,
            node.bounds.right
        )
    };
    let digest = Sha256::digest(semantic.as_bytes());
    format!("el-{}", hex_prefix(&digest, 8))
}

fn find_trigger_element<'a>(nodes: &'a [UiNode], args: &Value) -> Option<&'a UiNode> {
    if let Some(target_key) = args.get("elementKey").and_then(Value::as_str) {
        if let Some(node) = nodes
            .iter()
            .filter(|node| node.is_actionable())
            .find(|node| element_key(node) == target_key)
        {
            return Some(node);
        }
    }
    if let (Some(x), Some(y)) = (
        args.get("x").and_then(Value::as_i64),
        args.get("y").and_then(Value::as_i64),
    ) {
        return nodes
            .iter()
            .filter(|node| node.is_actionable())
            .filter(|node| {
                (x as i32) >= node.bounds.left
                    && (x as i32) <= node.bounds.right
                    && (y as i32) >= node.bounds.top
                    && (y as i32) <= node.bounds.bottom
            })
            .min_by_key(|node| node.bounds.area());
    }
    if let Some(text) = args.get("text").and_then(Value::as_str) {
        return nodes
            .iter()
            .filter(|node| node.is_actionable())
            .find(|node| {
                node.text.contains(text)
                    || node.content_description.contains(text)
                    || node.resource_id.contains(text)
            });
    }
    if let Some(resource_id) = args.get("resource_id").and_then(Value::as_str) {
        return nodes
            .iter()
            .filter(|node| node.is_actionable())
            .find(|node| node.resource_id.contains(resource_id));
    }
    None
}

fn element_label(node: &UiNode) -> String {
    if !node.text.trim().is_empty() {
        return truncate(node.text.trim(), 80);
    }
    if !node.content_description.trim().is_empty() {
        return truncate(node.content_description.trim(), 80);
    }
    if !node.resource_id.trim().is_empty() {
        return truncate(node.resource_id.trim(), 80);
    }
    format!("{} {}", node.class_name, node.bounds.top)
}

fn infer_page_name(observation: &Observation) -> String {
    let selected_tab = selected_tab_label(observation);
    let context = page_context(observation, selected_tab.as_deref());
    if !context.is_empty() {
        return context;
    }
    let activity = observation.activity.rsplit('.').next().unwrap_or("页面");
    let mut candidates = observation
        .nodes
        .iter()
        .filter(|node| node.visible)
        .filter(|node| node.package.is_empty() || node.package == observation.package)
        .filter(|node| {
            let text = node.text.trim();
            (2..=36).contains(&text.chars().count())
                && !text.chars().all(|character| character.is_ascii_digit())
                && !text.contains("http")
        })
        .filter(|node| observation.height <= 0 || node.bounds.top < observation.height / 2)
        .collect::<Vec<_>>();
    candidates.sort_by_key(|node| {
        (
            node.bounds.top,
            node.bounds.left,
            if node.class_name.contains("TextView") {
                0
            } else {
                1
            },
        )
    });
    if let Some(node) = candidates.first() {
        let text = truncate(node.text.trim(), 28);
        if !matches!(
            text.as_str(),
            "首页" | "推荐" | "搜索" | "消息" | "我的" | "闲鱼"
        ) {
            return text;
        }
    }
    let app = package_display_name(&observation.package);
    format!("{app} · {activity}")
}

fn selected_tab_label(observation: &Observation) -> Option<String> {
    let selected = observation.nodes.iter().find(|node| {
        node.visible
            && (node.selected
                || (node.content_description.contains("选中状态")
                    && !node.content_description.contains("未选中状态")))
    })?;
    let center_y = (selected.bounds.top + selected.bounds.bottom) / 2;
    observation
        .nodes
        .iter()
        .filter(|node| node.visible)
        .filter(|node| {
            let node_center = (node.bounds.top + node.bounds.bottom) / 2;
            (node_center - center_y).abs() <= 18
        })
        .filter_map(|node| {
            let label = if !node.text.trim().is_empty() {
                node.text.trim()
            } else if !node.content_description.trim().is_empty() {
                node.content_description.trim()
            } else {
                return None;
            };
            let label = label
                .split(['，', ','])
                .next()
                .unwrap_or(label)
                .trim()
                .lines()
                .map(str::trim)
                .find(|line| {
                    !line.is_empty()
                        && !(line.starts_with("第") && line.contains("标签"))
                        && !line.contains(":id/")
                })?;
            if label.is_empty() {
                return None;
            }
            let distance = (node.bounds.left - selected.bounds.left).abs();
            Some((distance, label.chars().take(20).collect::<String>()))
        })
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, label)| label)
}

fn page_context(observation: &Observation, selected_tab: Option<&str>) -> String {
    let has_profile_markers = observation.nodes.iter().any(|node| {
        let value = format!("{} {}", node.text, node.content_description);
        value.contains("粉丝") && value.contains("关注")
    });
    if has_profile_markers {
        return selected_tab
            .map(|tab| format!("用户主页 · {tab}"))
            .unwrap_or_else(|| "用户主页".to_string());
    }

    if observation.activity.ends_with("MainActivity") {
        let bottom_tab = observation
            .nodes
            .iter()
            .filter(|node| {
                node.selected
                    || (node.content_description.contains("选中状态")
                        && !node.content_description.contains("未选中状态"))
            })
            .filter_map(|node| node.content_description.split(['，', ',']).next())
            .find(|label| !label.trim().is_empty());
        if let Some(label) = bottom_tab {
            return match label.trim() {
                "闲鱼" => "首页".to_string(),
                "北京" => "同城".to_string(),
                "消息" => "消息".to_string(),
                "我的" => "我的".to_string(),
                other => other.to_string(),
            };
        }
    }

    selected_tab.map(|tab| tab.to_string()).unwrap_or_default()
}

fn is_weak_page_name(name: &str) -> bool {
    name.trim().is_empty()
        || name == "页面"
        || name.ends_with("· MainActivity")
        || name.ends_with("· Unknown")
}

fn package_display_name(package: &str) -> String {
    match package {
        "com.taobao.idlefish" => "闲鱼".to_string(),
        _ => package
            .rsplit('.')
            .next()
            .filter(|value| !value.is_empty())
            .unwrap_or(package)
            .to_string(),
    }
}

fn choose_start_page(plugin: &AppKnowledgePlugin) -> Option<&KnowledgePage> {
    plugin
        .pages
        .iter()
        .find(|page| page.activity.ends_with("MainActivity"))
        .or_else(|| {
            plugin
                .pages
                .iter()
                .min_by(|left, right| left.first_seen_at.cmp(&right.first_seen_at))
        })
}

fn compact_duplicate_pages(plugin: &mut AppKnowledgePlugin) {
    if plugin.pages.len() < 2 {
        return;
    }
    let pages = plugin.pages.clone();
    let mut consumed = HashSet::<usize>::new();
    let mut remap = HashMap::<String, String>::new();
    let mut compacted = Vec::<KnowledgePage>::new();

    for index in 0..pages.len() {
        if consumed.contains(&index) {
            continue;
        }
        let mut canonical = pages[index].clone();
        consumed.insert(index);
        for candidate_index in index + 1..pages.len() {
            if consumed.contains(&candidate_index) {
                continue;
            }
            let candidate = &pages[candidate_index];
            if !pages_are_similar(&canonical, candidate) {
                continue;
            }
            merge_knowledge_page(&mut canonical, candidate);
            remap.insert(candidate.id.clone(), canonical.id.clone());
            consumed.insert(candidate_index);
        }
        compacted.push(canonical);
    }
    for transition in &mut plugin.transitions {
        transition.from_page_id = remap
            .get(&transition.from_page_id)
            .cloned()
            .unwrap_or_else(|| transition.from_page_id.clone());
        transition.to_page_id = remap
            .get(&transition.to_page_id)
            .cloned()
            .unwrap_or_else(|| transition.to_page_id.clone());
    }
    let mut seen_transitions = HashSet::new();
    plugin.transitions.retain(|transition| {
        transition.from_page_id != transition.to_page_id
            && seen_transitions.insert((
                transition.from_page_id.clone(),
                transition.to_page_id.clone(),
                transition.trigger_key.clone(),
                transition.tool.clone(),
            ))
    });

    for action in &mut plugin.actions {
        action.from_page_id = remap
            .get(&action.from_page_id)
            .cloned()
            .unwrap_or_else(|| action.from_page_id.clone());
        action.to_page_id = remap
            .get(&action.to_page_id)
            .cloned()
            .unwrap_or_else(|| action.to_page_id.clone());
    }

    let mut best_tests = HashMap::<(String, String), crate::models::AppTestCase>::new();
    for mut test_case in plugin.test_cases.drain(..) {
        test_case.start_page_id = remap
            .get(&test_case.start_page_id)
            .cloned()
            .unwrap_or_else(|| test_case.start_page_id.clone());
        test_case.target_page_id = remap
            .get(&test_case.target_page_id)
            .cloned()
            .unwrap_or_else(|| test_case.target_page_id.clone());
        test_case.id = format!("{}:{}", test_case.package, test_case.target_page_id);
        for step in &mut test_case.steps {
            step.from_page_id = remap
                .get(&step.from_page_id)
                .cloned()
                .unwrap_or_else(|| step.from_page_id.clone());
            step.expected_page_id = remap
                .get(&step.expected_page_id)
                .cloned()
                .unwrap_or_else(|| step.expected_page_id.clone());
        }
        let key = (
            test_case.start_page_id.clone(),
            test_case.target_page_id.clone(),
        );
        let should_replace = best_tests.get(&key).is_none_or(|existing| {
            test_case_rank(&test_case.status) < test_case_rank(&existing.status)
                || (test_case_rank(&test_case.status) == test_case_rank(&existing.status)
                    && test_case.updated_at > existing.updated_at)
        });
        if should_replace {
            best_tests.insert(key, test_case);
        }
    }
    plugin.test_cases = best_tests.into_values().collect();
    plugin
        .test_cases
        .sort_by(|left, right| left.name.cmp(&right.name));
    plugin.pages = compacted;
}

fn test_case_rank(status: &str) -> u8 {
    match status {
        "verified" => 0,
        "running" => 1,
        "failed" => 2,
        _ => 3,
    }
}

fn pages_are_similar(left: &KnowledgePage, right: &KnowledgePage) -> bool {
    if left.package != right.package || left.activity != right.activity {
        return false;
    }
    let left_tokens = stable_element_tokens(&left.elements);
    let right_tokens = stable_element_tokens(&right.elements);
    if left_tokens.len() < 2 || right_tokens.len() < 2 {
        return false;
    }
    let intersection = left_tokens.intersection(&right_tokens).count();
    let union = left_tokens.union(&right_tokens).count();
    let similarity = intersection as f64 / union.max(1) as f64;
    let same_name = left.name == right.name;
    let both_weak = is_weak_page_name(&left.name) && is_weak_page_name(&right.name);
    let (min_intersection, min_similarity) = if same_name && !is_weak_page_name(&left.name) {
        (8, 0.12)
    } else if both_weak {
        (5, 0.35)
    } else {
        (4, 0.45)
    };
    intersection >= min_intersection && similarity >= min_similarity
}

fn merge_knowledge_page(target: &mut KnowledgePage, incoming: &KnowledgePage) {
    target.visit_count = target.visit_count.saturating_add(incoming.visit_count);
    if incoming.first_seen_at < target.first_seen_at {
        target.first_seen_at = incoming.first_seen_at.clone();
    }
    if incoming.last_seen_at > target.last_seen_at {
        target.last_seen_at = incoming.last_seen_at.clone();
        target.screenshot_path = incoming.screenshot_path.clone();
        target.ui_path = incoming.ui_path.clone();
        target.width = incoming.width;
        target.height = incoming.height;
    }
    if is_weak_page_name(&target.name) && !is_weak_page_name(&incoming.name) {
        target.name = incoming.name.clone();
    }

    let mut elements = target
        .elements
        .drain(..)
        .map(|element| (element_slot_key(&element), element))
        .collect::<HashMap<_, _>>();
    for element in &incoming.elements {
        let slot = element_slot_key(element);
        if let Some(existing) = elements.get_mut(&slot) {
            let existing_key = existing.key.clone();
            *existing = element.clone();
            existing.key = existing_key;
        } else {
            elements.insert(slot, element.clone());
        }
    }
    target.elements = elements.into_values().collect();
    target
        .elements
        .sort_by_key(|element| (element.bounds.top, element.bounds.left));
    target.elements.truncate(MAX_ELEMENTS_PER_PAGE);
}

fn merge_plugins(target: &mut AppKnowledgePlugin, incoming: AppKnowledgePlugin) {
    for page in incoming.pages {
        if let Some(existing) = target.pages.iter_mut().find(|item| item.id == page.id) {
            if page.last_seen_at > existing.last_seen_at {
                *existing = page;
            }
        } else {
            target.pages.push(page);
        }
    }
    let mut transition_ids = target
        .transitions
        .iter()
        .map(|item| item.id.clone())
        .collect::<HashSet<_>>();
    for transition in incoming.transitions {
        if transition_ids.insert(transition.id.clone()) {
            target.transitions.push(transition);
        }
    }
    let mut action_ids = target
        .actions
        .iter()
        .map(|item| item.id.clone())
        .collect::<HashSet<_>>();
    for action in incoming.actions {
        if action_ids.insert(action.id.clone()) {
            target.actions.push(action);
        }
    }
    let mut test_ids = target
        .test_cases
        .iter()
        .map(|item| item.id.clone())
        .collect::<HashSet<_>>();
    for test_case in incoming.test_cases {
        if test_ids.insert(test_case.id.clone()) {
            target.test_cases.push(test_case);
        }
    }
    if !incoming.name.trim().is_empty() {
        target.name = incoming.name;
    }
}

fn result_summary(result: &Value) -> String {
    result
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| result.get("error").and_then(Value::as_str))
        .unwrap_or("")
        .chars()
        .take(120)
        .collect()
}

fn sanitize_package(package: &str) -> String {
    package
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn truncate(text: &str, limit: usize) -> String {
    let mut result = text.chars().take(limit).collect::<String>();
    if text.chars().count() > limit {
        result.push('…');
    }
    result
}

fn hex_prefix(bytes: &[u8], length: usize) -> String {
    bytes
        .iter()
        .take(length)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Bounds;
    use serde_json::json;

    fn observation(page_id: &str, title: &str) -> Observation {
        Observation {
            device_serial: "test".to_string(),
            screenshot_data_url: String::new(),
            model_screenshot_data_url: None,
            model_screenshot_path: None,
            model_screenshot_error: None,
            screenshot_path: "/tmp/page.png".to_string(),
            ui_path: "/tmp/page.xml".to_string(),
            nodes: vec![UiNode {
                index: 0,
                text: title.to_string(),
                content_description: String::new(),
                resource_id: "com.example:id/title".to_string(),
                class_name: "android.widget.TextView".to_string(),
                package: "com.example".to_string(),
                bounds: Bounds {
                    left: 10,
                    top: 20,
                    right: 300,
                    bottom: 100,
                },
                clickable: false,
                enabled: true,
                focusable: false,
                scrollable: false,
                selected: false,
                visible: true,
                depth: 2,
            }],
            package: "com.example".to_string(),
            activity: "com.example.MainActivity".to_string(),
            page_id: page_id.to_string(),
            page_signature: format!("{page_id}0000000000000000"),
            captured_at: Utc::now().to_rfc3339(),
            width: 1080,
            height: 2400,
            ocr: None,
            ocr_path: None,
            ocr_error: None,
        }
    }

    #[test]
    fn registers_pages_actions_and_generates_tests() {
        let root = std::env::temp_dir().join(format!("knowledge-test-{}", Uuid::new_v4()));
        let store = KnowledgeStore::new(&root).expect("create knowledge store");
        let home = observation("home-page-00001", "首页");
        let result = observation("result-page-0001", "搜索结果");
        store.register_observation(&home).expect("register home");
        store
            .register_observation(&result)
            .expect("register result");
        store
            .record_action(
                "run-1",
                1,
                &home,
                &result,
                "tap_text",
                &json!({ "text": "搜索" }),
                &json!({ "message": "clicked" }),
                true,
            )
            .expect("record action");
        let tests = store
            .generate_test_cases("com.example")
            .expect("generate tests");
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].steps.len(), 1);
        let plugin = store.get_plugin("com.example").expect("read plugin");
        assert_eq!(plugin.pages.len(), 2);
        assert_eq!(plugin.transitions.len(), 1);
        assert_eq!(plugin.actions.len(), 1);
        drop(store);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn imports_plugins_without_duplicate_actions() {
        let root = std::env::temp_dir().join(format!("knowledge-test-{}", Uuid::new_v4()));
        let store = KnowledgeStore::new(&root).expect("create knowledge store");
        let home = observation("home-page-00001", "首页");
        store.register_observation(&home).expect("register home");
        let raw = store.export_plugin("com.example").expect("export plugin");
        store.import_plugin(&raw).expect("import plugin");
        let plugin = store.get_plugin("com.example").expect("read plugin");
        assert_eq!(plugin.pages.len(), 1);
        drop(store);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ignores_unselected_navigation_labels() {
        let mut obs = observation("home-page-00001", "首页");
        obs.activity = "com.example.MainActivity".to_string();
        obs.nodes = vec![
            UiNode {
                index: 0,
                text: String::new(),
                content_description: "闲鱼，未选中状态".to_string(),
                resource_id: String::new(),
                class_name: "android.widget.TextView".to_string(),
                package: "com.example".to_string(),
                bounds: Bounds {
                    left: 22,
                    top: 2218,
                    right: 218,
                    bottom: 2260,
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
                text: String::new(),
                content_description: "北京，选中状态".to_string(),
                resource_id: String::new(),
                class_name: "android.widget.TextView".to_string(),
                package: "com.example".to_string(),
                bounds: Bounds {
                    left: 218,
                    top: 2218,
                    right: 415,
                    bottom: 2260,
                },
                clickable: true,
                enabled: true,
                focusable: true,
                scrollable: false,
                selected: true,
                visible: true,
                depth: 2,
            },
        ];
        assert_eq!(selected_tab_label(&obs).as_deref(), Some("北京"));
        assert_eq!(infer_page_name(&obs), "同城");
    }

    #[test]
    fn clusters_same_named_pages_with_dynamic_content() {
        let now = Utc::now().to_rfc3339();
        let element = |index: usize, text: &str| KnowledgeElement {
            key: format!("el-{index}"),
            index,
            text: text.to_string(),
            content_description: String::new(),
            resource_id: format!("com.example:id/slot-{index}"),
            class_name: "android.widget.TextView".to_string(),
            bounds: Bounds {
                left: 0,
                top: index as i32 * 10,
                right: 100,
                bottom: index as i32 * 10 + 10,
            },
            clickable: true,
            scrollable: false,
            focusable: true,
        };
        let page = |id: &str, text: &str| KnowledgePage {
            id: id.to_string(),
            name: "首页".to_string(),
            signature: id.to_string(),
            package: "com.example".to_string(),
            activity: "com.example.MainActivity".to_string(),
            screenshot_path: String::new(),
            ui_path: String::new(),
            width: 1080,
            height: 2400,
            first_seen_at: now.clone(),
            last_seen_at: now.clone(),
            visit_count: 1,
            elements: (0..10).map(|index| element(index, text)).collect(),
        };
        let left = page("page-a", "动态商品 A");
        let mut right = page("page-b", "动态商品 B");
        right.elements[0].text = "另一个标题".to_string();
        assert!(pages_are_similar(&left, &right));
    }
}
