use std::fs;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use base64::Engine;
use chrono::Utc;
use roxmltree::Document;
use sha2::{Digest, Sha256};
use tokio::process::Command;
use tokio::time::sleep;

use crate::models::{Bounds, DeviceInfo, Observation, UiNode};

pub async fn list_devices() -> Result<Vec<DeviceInfo>, String> {
    ensure_adb_available().await?;
    let output = adb_text(vec!["devices".into(), "-l".into()]).await?;
    let mut devices = Vec::new();

    for line in output.lines().skip(1) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut fields = line.split_whitespace();
        let Some(serial) = fields.next() else {
            continue;
        };
        let Some(state) = fields.next() else {
            continue;
        };

        if state == "device" {
            devices.push(read_device_info(serial, state).await);
        } else {
            devices.push(DeviceInfo {
                serial: serial.to_string(),
                state: state.to_string(),
                model: None,
                manufacturer: None,
                android_version: None,
                sdk: None,
                resolution: None,
                foreground_app: None,
                ready: false,
            });
        }
    }

    Ok(devices)
}

async fn read_device_info(serial: &str, state: &str) -> DeviceInfo {
    let model = getprop(serial, "ro.product.model").await.ok();
    let manufacturer = getprop(serial, "ro.product.manufacturer").await.ok();
    let android_version = getprop(serial, "ro.build.version.release").await.ok();
    let sdk = getprop(serial, "ro.build.version.sdk").await.ok();
    let resolution = display_size(serial)
        .await
        .ok()
        .map(|(width, height)| format!("{width} × {height}"));
    let foreground_app = foreground_component(serial).await.ok().flatten();

    DeviceInfo {
        serial: serial.to_string(),
        state: state.to_string(),
        model: non_empty(model),
        manufacturer: non_empty(manufacturer),
        android_version: non_empty(android_version),
        sdk: non_empty(sdk),
        resolution,
        foreground_app,
        ready: true,
    }
}

pub async fn ensure_device_ready(serial: &str) -> Result<(), String> {
    ensure_adb_available().await?;
    let output = adb_text(vec!["-s".into(), serial.into(), "get-state".into()]).await?;
    if output.trim() == "device" {
        Ok(())
    } else {
        Err(format!("设备 {serial} 当前状态不可用: {}", output.trim()))
    }
}

pub async fn capture_observation(
    serial: &str,
    artifacts_dir: &Path,
    run_id: &str,
    label: &str,
) -> Result<Observation, String> {
    ensure_device_ready(serial).await?;

    let run_dir = artifacts_dir.join(run_id);
    fs::create_dir_all(&run_dir).map_err(|error| format!("创建任务证据目录失败: {error}"))?;

    let screenshot = adb_bytes(vec![
        "-s".into(),
        serial.into(),
        "exec-out".into(),
        "screencap".into(),
        "-p".into(),
    ])
    .await?;
    if screenshot.is_empty() {
        return Err("设备返回了空截图，设备可能已锁屏".to_string());
    }

    let safe_label = sanitize_file_label(label);
    let screenshot_path = run_dir.join(format!("{safe_label}.png"));
    let ui_path = run_dir.join(format!("{safe_label}.xml"));
    fs::write(&screenshot_path, &screenshot).map_err(|error| format!("保存截图失败: {error}"))?;

    let ui_xml = dump_ui_xml(serial).await.unwrap_or_default();
    let nodes = parse_ui_nodes(&ui_xml).unwrap_or_default();
    if !ui_xml.is_empty() {
        fs::write(&ui_path, &ui_xml).map_err(|error| format!("保存 UI 树失败: {error}"))?;
    }

    let (width, height) = png_dimensions(&screenshot)
        .or_else(|| inferred_dimensions(&nodes))
        .unwrap_or((0, 0));
    let foreground = foreground_component(serial).await.ok().flatten();
    let (package, activity) = split_component(foreground.as_deref()).unwrap_or_else(|| {
        let package = nodes
            .iter()
            .find(|node| !node.package.is_empty())
            .map(|node| node.package.clone())
            .unwrap_or_default();
        (package, String::new())
    });

    let page_signature = page_signature(&package, &activity, &nodes);
    let page_id = page_signature[..16].to_string();
    let screenshot_data_url = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&screenshot)
    );

    Ok(Observation {
        device_serial: serial.to_string(),
        screenshot_data_url,
        screenshot_path: screenshot_path.to_string_lossy().to_string(),
        ui_path: ui_path.to_string_lossy().to_string(),
        nodes,
        package,
        activity,
        page_id,
        page_signature,
        captured_at: Utc::now().to_rfc3339(),
        width,
        height,
    })
}

pub async fn tap(serial: &str, x: i32, y: i32) -> Result<String, String> {
    adb_text(vec![
        "-s".into(),
        serial.into(),
        "shell".into(),
        "input".into(),
        "tap".into(),
        x.to_string(),
        y.to_string(),
    ])
    .await?;
    Ok(format!("已点击坐标 ({x}, {y})"))
}

pub async fn long_press(serial: &str, x: i32, y: i32, duration_ms: u64) -> Result<String, String> {
    let duration = duration_ms.clamp(300, 5_000);
    adb_text(vec![
        "-s".into(),
        serial.into(),
        "shell".into(),
        "input".into(),
        "swipe".into(),
        x.to_string(),
        y.to_string(),
        x.to_string(),
        y.to_string(),
        duration.to_string(),
    ])
    .await?;
    Ok(format!("已长按 ({x}, {y})，时长 {duration}ms"))
}

pub async fn swipe(
    serial: &str,
    start: (i32, i32),
    end: (i32, i32),
    duration_ms: u64,
) -> Result<String, String> {
    let duration = duration_ms.clamp(120, 3_000);
    adb_text(vec![
        "-s".into(),
        serial.into(),
        "shell".into(),
        "input".into(),
        "swipe".into(),
        start.0.to_string(),
        start.1.to_string(),
        end.0.to_string(),
        end.1.to_string(),
        duration.to_string(),
    ])
    .await?;
    Ok(format!(
        "已从 ({}, {}) 滑动到 ({}, {})",
        start.0, start.1, end.0, end.1
    ))
}

pub async fn input_text(serial: &str, text: &str, clear: bool) -> Result<String, String> {
    if clear {
        clear_input(serial).await?;
    }

    let encoded = base64::engine::general_purpose::STANDARD.encode(text.as_bytes());
    let broadcast = adb_text(vec![
        "-s".into(),
        serial.into(),
        "shell".into(),
        "am".into(),
        "broadcast".into(),
        "-a".into(),
        "ADB_INPUT_B64".into(),
        "--es".into(),
        "msg".into(),
        encoded,
    ])
    .await;

    if broadcast
        .as_ref()
        .map(|value| value.contains("Broadcast completed"))
        .unwrap_or(false)
    {
        return Ok("已通过 ADB Keyboard 输入文本".to_string());
    }

    if text.is_ascii() {
        let escaped = text.replace(' ', "%s");
        adb_text(vec![
            "-s".into(),
            serial.into(),
            "shell".into(),
            "input".into(),
            "text".into(),
            escaped,
        ])
        .await?;
        Ok("已通过系统 input text 输入文本".to_string())
    } else {
        Err("Unicode 文本输入失败，请在设备安装并启用 ADB Keyboard".to_string())
    }
}

pub async fn clear_input(serial: &str) -> Result<(), String> {
    adb_text(vec![
        "-s".into(),
        serial.into(),
        "shell".into(),
        "input".into(),
        "keyevent".into(),
        "KEYCODE_MOVE_END".into(),
    ])
    .await?;
    adb_text(vec![
        "-s".into(),
        serial.into(),
        "shell".into(),
        "sh".into(),
        "-c".into(),
        "i=0; while [ $i -lt 80 ]; do input keyevent 67; i=$((i+1)); done".into(),
    ])
    .await?;
    Ok(())
}

pub async fn press_key(serial: &str, key: &str) -> Result<String, String> {
    let mapped = match key.to_lowercase().as_str() {
        "back" => "KEYCODE_BACK",
        "home" => "KEYCODE_HOME",
        "enter" => "KEYCODE_ENTER",
        "recents" | "recent" => "KEYCODE_APP_SWITCH",
        "delete" => "KEYCODE_DEL",
        "search" => "KEYCODE_SEARCH",
        other => return Err(format!("不支持的按键: {other}")),
    };
    adb_text(vec![
        "-s".into(),
        serial.into(),
        "shell".into(),
        "input".into(),
        "keyevent".into(),
        mapped.into(),
    ])
    .await?;
    Ok(format!("已发送系统按键 {key}"))
}

pub async fn start_app(serial: &str, package: &str) -> Result<String, String> {
    if package.trim().is_empty() {
        return Err("包名不能为空".to_string());
    }
    let component = resolve_launcher_component(serial, package).await?;
    let mut last_foreground = String::new();
    for attempt in 0..3 {
        adb_text(vec![
            "-s".into(),
            serial.into(),
            "shell".into(),
            "am".into(),
            "start".into(),
            "-W".into(),
            "-S".into(),
            "-a".into(),
            "android.intent.action.MAIN".into(),
            "-c".into(),
            "android.intent.category.LAUNCHER".into(),
            "-n".into(),
            component.clone(),
        ])
        .await?;
        sleep(Duration::from_millis(1_600)).await;
        if let Some(foreground) = foreground_component(serial).await? {
            let foreground_package = foreground.split('/').next().unwrap_or_default();
            let target_is_foreground = foreground_package == package;
            last_foreground = foreground;
            if target_is_foreground {
                return Ok(format!("已启动 {package}（第 {} 次尝试）", attempt + 1));
            }
        }
        // Some apps or their launch ads open a system settings page on top.
        // Back out of that page before retrying instead of operating the wrong app.
        press_key(serial, "back").await?;
        sleep(Duration::from_millis(500)).await;
    }
    Err(format!(
        "无法将 {package} 切到前台，当前前台为 {}",
        if last_foreground.is_empty() {
            "未知"
        } else {
            &last_foreground
        }
    ))
}

async fn resolve_launcher_component(serial: &str, package: &str) -> Result<String, String> {
    let resolved = adb_text(vec![
        "-s".into(),
        serial.into(),
        "shell".into(),
        "cmd".into(),
        "package".into(),
        "resolve-activity".into(),
        "--brief".into(),
        "-a".into(),
        "android.intent.action.MAIN".into(),
        "-c".into(),
        "android.intent.category.LAUNCHER".into(),
        package.to_string(),
    ])
    .await?;
    resolved
        .lines()
        .map(str::trim)
        .filter(|line| line.contains('/'))
        .last()
        .map(ToString::to_string)
        .ok_or_else(|| format!("应用 {package} 没有可启动的 Launcher Activity"))
}

pub async fn restart_app_cleanly(serial: &str, package: &str) -> Result<String, String> {
    adb_text(vec![
        "-s".into(),
        serial.into(),
        "shell".into(),
        "am".into(),
        "force-stop".into(),
        package.to_string(),
    ])
    .await?;
    start_app(serial, package)
        .await
        .map(|message| format!("已冷启动：{message}"))
}

pub async fn list_packages(serial: &str) -> Result<Vec<String>, String> {
    let output = adb_text(vec![
        "-s".into(),
        serial.into(),
        "shell".into(),
        "pm".into(),
        "list".into(),
        "packages".into(),
        "-3".into(),
    ])
    .await?;
    Ok(output
        .lines()
        .filter_map(|line| line.trim().strip_prefix("package:"))
        .map(ToString::to_string)
        .collect())
}

pub async fn launch_scrcpy(serial: &str) -> Result<String, String> {
    Command::new("scrcpy")
        .args(["--serial", serial, "--no-audio"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("启动 scrcpy 失败: {error}"))?;
    Ok(format!("已为设备 {serial} 启动 scrcpy"))
}

pub async fn dump_ui_nodes(serial: &str) -> Result<Vec<UiNode>, String> {
    let xml = dump_ui_xml(serial).await?;
    parse_ui_nodes(&xml)
}

pub async fn wait_ms(milliseconds: u64) -> String {
    let duration = milliseconds.clamp(100, 10_000);
    sleep(Duration::from_millis(duration)).await;
    format!("已等待 {duration}ms")
}

async fn dump_ui_xml(serial: &str) -> Result<String, String> {
    adb_text(vec![
        "-s".into(),
        serial.into(),
        "shell".into(),
        "uiautomator".into(),
        "dump".into(),
        "/sdcard/ai_apptest_window.xml".into(),
    ])
    .await?;
    adb_text(vec![
        "-s".into(),
        serial.into(),
        "exec-out".into(),
        "cat".into(),
        "/sdcard/ai_apptest_window.xml".into(),
    ])
    .await
}

pub fn parse_ui_nodes(xml: &str) -> Result<Vec<UiNode>, String> {
    if xml.trim().is_empty() {
        return Ok(Vec::new());
    }
    let document = Document::parse(xml).map_err(|error| format!("解析 UI XML 失败: {error}"))?;
    let mut nodes = Vec::new();

    for (index, node) in document
        .descendants()
        .filter(|node| node.has_tag_name("node"))
        .enumerate()
    {
        let bounds = node
            .attribute("bounds")
            .and_then(parse_bounds)
            .unwrap_or_default();
        let depth = node
            .ancestors()
            .filter(|ancestor| ancestor.has_tag_name("node"))
            .count()
            .saturating_sub(1);
        nodes.push(UiNode {
            index,
            text: node.attribute("text").unwrap_or_default().to_string(),
            content_description: node
                .attribute("content-desc")
                .unwrap_or_default()
                .to_string(),
            resource_id: node
                .attribute("resource-id")
                .unwrap_or_default()
                .to_string(),
            class_name: node.attribute("class").unwrap_or_default().to_string(),
            package: node.attribute("package").unwrap_or_default().to_string(),
            bounds,
            clickable: node.attribute("clickable") == Some("true"),
            enabled: node.attribute("enabled") != Some("false"),
            focusable: node.attribute("focusable") == Some("true"),
            scrollable: node.attribute("scrollable") == Some("true"),
            selected: node.attribute("selected") == Some("true"),
            visible: node.attribute("visible-to-user") != Some("false"),
            depth,
        });
    }

    Ok(nodes)
}

pub fn parse_bounds(raw: &str) -> Option<Bounds> {
    let cleaned = raw.replace("][", " ");
    let values = cleaned
        .trim_matches(|character| character == '[' || character == ']')
        .split(|character: char| character == ',' || character.is_whitespace())
        .filter(|part| !part.is_empty())
        .map(str::parse::<i32>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if values.len() != 4 {
        return None;
    }
    Some(Bounds {
        left: values[0],
        top: values[1],
        right: values[2],
        bottom: values[3],
    })
}

pub fn find_node_by_text<'a>(nodes: &'a [UiNode], needle: &str) -> Option<&'a UiNode> {
    let needle = needle.trim().to_lowercase();
    if needle.is_empty() {
        return None;
    }

    nodes
        .iter()
        .filter(|node| node.is_actionable())
        .filter_map(|node| {
            let text = node.text.to_lowercase();
            let description = node.content_description.to_lowercase();
            let id = node.resource_id.to_lowercase();
            let score = if text == needle || description == needle {
                0
            } else if id == needle {
                1
            } else if text.contains(&needle) || description.contains(&needle) {
                2 + text.len().saturating_sub(needle.len()) as i32
            } else if id.contains(&needle) {
                3
            } else {
                return None;
            };
            Some((score, node))
        })
        .min_by_key(|(score, node)| (*score, -node.bounds.area()))
        .map(|(_, node)| node)
}

pub fn find_node_by_id<'a>(nodes: &'a [UiNode], needle: &str) -> Option<&'a UiNode> {
    let needle = needle.trim().to_lowercase();
    nodes
        .iter()
        .filter(|node| node.is_actionable())
        .filter(|node| {
            let id = node.resource_id.to_lowercase();
            !needle.is_empty() && (id == needle || id.ends_with(&needle))
        })
        .max_by_key(|node| node.bounds.area())
}

pub fn content_viewport(nodes: &[UiNode]) -> (i32, i32) {
    nodes
        .iter()
        .filter(|node| node.is_actionable())
        .map(|node| (node.bounds.right, node.bounds.bottom))
        .fold((0, 0), |(width, height), (right, bottom)| {
            (width.max(right), height.max(bottom))
        })
}

fn inferred_dimensions(nodes: &[UiNode]) -> Option<(i32, i32)> {
    let dimensions = content_viewport(nodes);
    (dimensions.0 > 0 && dimensions.1 > 0).then_some(dimensions)
}

fn page_signature(package: &str, activity: &str, nodes: &[UiNode]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(package.as_bytes());
    hasher.update(activity.as_bytes());
    let height = nodes
        .iter()
        .filter(|node| node.visible)
        .map(|node| node.bounds.bottom)
        .max()
        .unwrap_or(0);
    let mut selected = nodes
        .iter()
        .filter(|node| node.visible && node.selected)
        .filter_map(node_semantic)
        .collect::<Vec<_>>();
    selected.sort();
    selected.dedup();
    for semantic in &selected {
        hasher.update(b"selected:");
        hasher.update(semantic.as_bytes());
    }

    let mut bottom_navigation = nodes
        .iter()
        .filter(|node| node.visible && node.clickable && node.bounds.top > height * 88 / 100)
        .filter(|node| {
            node.content_description.contains("选中状态")
                || node.content_description.contains("未选中状态")
                || node.resource_id.ends_with("tab_title")
        })
        .filter_map(node_semantic)
        .collect::<Vec<_>>();
    bottom_navigation.sort();
    bottom_navigation.dedup();
    for semantic in &bottom_navigation {
        hasher.update(b"nav:");
        hasher.update(semantic.as_bytes());
    }

    if selected.is_empty() && bottom_navigation.is_empty() {
        let mut structural = nodes
            .iter()
            .filter(|node| {
                node.visible
                    && node.is_actionable()
                    && (!node.resource_id.is_empty() || node.scrollable)
            })
            .map(|node| format!("{}|{}|{}", node.class_name, node.resource_id, node.selected))
            .collect::<Vec<_>>();
        structural.sort();
        structural.dedup();
        for semantic in structural.iter().take(48) {
            hasher.update(b"structure:");
            hasher.update(semantic.as_bytes());
        }
    }
    format!("{:x}", hasher.finalize())
}

fn node_semantic(node: &UiNode) -> Option<String> {
    let value = if !node.text.trim().is_empty() {
        node.text.trim()
    } else if !node.content_description.trim().is_empty() {
        node.content_description.trim()
    } else if !node.resource_id.trim().is_empty() {
        node.resource_id.trim()
    } else {
        return None;
    };
    meaningful_text(value).map(|value| {
        let normalized = if value.contains("未读消息数") {
            let name = value.split('，').next().unwrap_or(value);
            let state = value.rsplit('，').next().unwrap_or("");
            format!("{name}，{state}")
        } else {
            value.to_string()
        };
        normalized.chars().take(120).collect()
    })
}

fn meaningful_text(value: &str) -> Option<&str> {
    let value = value.trim();
    if value.is_empty() || value.contains(":id/") {
        return None;
    }
    value.lines().map(str::trim).find(|line| {
        !line.is_empty()
            && !(line.starts_with("第") && line.contains("标签"))
            && !line.contains(":id/")
    })
}

fn png_dimensions(bytes: &[u8]) -> Option<(i32, i32)> {
    if bytes.len() < 24 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    Some((width as i32, height as i32))
}

async fn display_size(serial: &str) -> Result<(i32, i32), String> {
    let output = adb_text(vec![
        "-s".into(),
        serial.into(),
        "shell".into(),
        "wm".into(),
        "size".into(),
    ])
    .await?;
    let mut physical = None;
    let mut override_size = None;
    for line in output.lines() {
        if let Some(value) = line.trim().strip_prefix("Physical size:") {
            physical = parse_dimensions(value);
        } else if let Some(value) = line.trim().strip_prefix("Override size:") {
            override_size = parse_dimensions(value);
        }
    }
    override_size
        .or(physical)
        .ok_or_else(|| format!("无法解析设备分辨率: {}", output.trim()))
}

fn parse_dimensions(value: &str) -> Option<(i32, i32)> {
    let (width, height) = value.trim().split_once('x')?;
    Some((width.trim().parse().ok()?, height.trim().parse().ok()?))
}

async fn foreground_component(serial: &str) -> Result<Option<String>, String> {
    let activity = adb_text(vec![
        "-s".into(),
        serial.into(),
        "shell".into(),
        "dumpsys".into(),
        "activity".into(),
        "activities".into(),
    ])
    .await?;
    for line in activity.lines() {
        if line.contains("topResumedActivity") || line.contains("mResumedActivity") {
            if let Some(component) = component_from_line(line) {
                return Ok(Some(component));
            }
        }
    }

    let windows = adb_text(vec![
        "-s".into(),
        serial.into(),
        "shell".into(),
        "dumpsys".into(),
        "window".into(),
    ])
    .await?;
    for line in windows.lines() {
        if line.contains("mCurrentFocus") {
            if let Some(component) = component_from_line(line) {
                return Ok(Some(component));
            }
        }
    }
    Ok(None)
}

fn component_from_line(line: &str) -> Option<String> {
    line.split_whitespace()
        .find(|token| token.contains('/') && token.contains('.'))
        .map(|token| {
            token
                .trim_matches(|character: char| {
                    !(character.is_alphanumeric()
                        || matches!(character, '/' | '.' | '_' | '$' | '-'))
                })
                .to_string()
        })
        .filter(|token| !token.is_empty())
}

fn split_component(component: Option<&str>) -> Option<(String, String)> {
    let component = component?;
    let (package, activity) = component.split_once('/')?;
    let activity = if activity.starts_with('.') {
        format!("{package}{activity}")
    } else {
        activity.to_string()
    };
    Some((package.to_string(), activity))
}

async fn getprop(serial: &str, key: &str) -> Result<String, String> {
    adb_text(vec![
        "-s".into(),
        serial.into(),
        "shell".into(),
        "getprop".into(),
        key.into(),
    ])
    .await
}

async fn ensure_adb_available() -> Result<(), String> {
    match Command::new("adb").arg("version").output().await {
        Ok(output) if output.status.success() => Ok(()),
        Ok(_) => Err("adb 已安装但无法正常启动".to_string()),
        Err(_) => Err("未找到 adb，请安装 Android Platform Tools".to_string()),
    }
}

async fn adb_text(args: Vec<String>) -> Result<String, String> {
    let output = adb_output(args).await?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

async fn adb_bytes(args: Vec<String>) -> Result<Vec<u8>, String> {
    let output = adb_output(args).await?;
    Ok(output.stdout)
}

async fn adb_output(args: Vec<String>) -> Result<std::process::Output, String> {
    let output = Command::new("adb")
        .args(&args)
        .output()
        .await
        .map_err(|error| format!("执行 adb 失败: {error}"))?;
    if output.status.success() {
        Ok(output)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if !stderr.is_empty() { stderr } else { stdout };
        Err(if detail.is_empty() {
            "adb 命令执行失败".to_string()
        } else {
            detail
        })
    }
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_string();
        (!value.is_empty()).then_some(value)
    })
}

fn sanitize_file_label(label: &str) -> String {
    let sanitized = label
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    if sanitized.is_empty() {
        "capture".to_string()
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_android_bounds() {
        assert_eq!(
            parse_bounds("[10,20][110,220]"),
            Some(Bounds {
                left: 10,
                top: 20,
                right: 110,
                bottom: 220
            })
        );
    }

    #[test]
    fn extracts_component_from_activity_line() {
        let line = "mResumedActivity: ActivityRecord{12 u0 com.example/.MainActivity t42}";
        assert_eq!(
            component_from_line(line),
            Some("com.example/.MainActivity".to_string())
        );
    }

    #[test]
    fn finds_best_text_node() {
        let nodes = vec![
            UiNode {
                index: 0,
                text: "搜索商品".into(),
                content_description: String::new(),
                resource_id: String::new(),
                class_name: "android.widget.TextView".into(),
                package: "com.example".into(),
                bounds: Bounds {
                    left: 0,
                    top: 0,
                    right: 200,
                    bottom: 100,
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
                text: "搜索".into(),
                content_description: String::new(),
                resource_id: String::new(),
                class_name: "android.widget.EditText".into(),
                package: "com.example".into(),
                bounds: Bounds {
                    left: 0,
                    top: 120,
                    right: 300,
                    bottom: 220,
                },
                clickable: true,
                enabled: true,
                focusable: true,
                scrollable: false,
                selected: false,
                visible: true,
                depth: 2,
            },
        ];
        assert_eq!(find_node_by_text(&nodes, "搜索").unwrap().index, 1);
    }
}
