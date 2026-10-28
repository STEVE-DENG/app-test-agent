use std::collections::HashMap;
use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    let summary = args.first().is_some_and(|value| value == "--summary");
    if summary {
        args.remove(0);
    }
    let mut args = args.into_iter();
    let data_dir = PathBuf::from(args.next().unwrap_or_else(|| {
        eprintln!("usage: agent-debug [--summary] <data-dir> <device-serial> [package]");
        std::process::exit(2);
    }));
    let device_serial = args.next().unwrap_or_else(|| {
        eprintln!("missing device serial");
        std::process::exit(2);
    });
    let package = args.next();
    let summary_data_dir = data_dir.clone();
    let summary_package = package.clone();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("create tokio runtime");
    let result = runtime.block_on(ai_apptest_agent_lib::run_headless_debug(
        data_dir,
        device_serial,
        package,
    ));
    match result {
        Ok(snapshot) => {
            let output = if summary {
                let ocr_blocks = snapshot
                    .ocr
                    .as_ref()
                    .map(|ocr| ocr.blocks.as_slice())
                    .unwrap_or_default();
                let knowledge = summary_package
                    .as_deref()
                    .and_then(|package| {
                        let store =
                            ai_apptest_agent_lib::knowledge::KnowledgeStore::new(&summary_data_dir)
                                .ok()?;
                        let plugin = store.get_plugin(package).ok()?;
                        let mut statuses = HashMap::<String, usize>::new();
                        let mut failure_categories = HashMap::<String, usize>::new();
                        for test_case in &plugin.test_cases {
                            *statuses.entry(test_case.status.clone()).or_default() += 1;
                            if let Some(analysis) = &test_case.last_analysis {
                                *failure_categories
                                    .entry(analysis.category.clone())
                                    .or_default() += 1;
                            }
                        }
                        Some(serde_json::json!({
                            "pages": plugin.pages.len(),
                            "transitions": plugin.transitions.len(),
                            "actions": plugin.actions.len(),
                            "testStatuses": statuses,
                            "failureCategories": failure_categories,
                            "failedTests": plugin.test_cases.iter()
                                .filter(|test_case| test_case.status == "failed")
                                .map(|test_case| serde_json::json!({
                                    "id": test_case.id,
                                    "name": test_case.name,
                                    "runId": test_case.last_run_id,
                                    "result": test_case.last_result,
                                    "category": test_case.last_analysis.as_ref().map(|analysis| analysis.category.clone()),
                                    "confidence": test_case.last_analysis.as_ref().map(|analysis| analysis.confidence),
                                    "repetitions": test_case.last_analysis.as_ref().map(|analysis| analysis.repetitions),
                                    "failedStep": test_case.last_analysis.as_ref().map(|analysis| analysis.failed_step),
                                    "retryable": test_case.last_analysis.as_ref().map(|analysis| analysis.retryable)
                                }))
                                .collect::<Vec<_>>()
                        }))
                    });
                let conversations =
                    ai_apptest_agent_lib::storage::AppStore::new(summary_data_dir.clone())
                        .and_then(|store| store.list_conversations(6))
                        .unwrap_or_default()
                        .into_iter()
                        .map(|conversation| {
                            serde_json::json!({
                                "id": conversation.id,
                                "title": conversation.title,
                                "messageCount": conversation.message_count,
                                "lastMessage": conversation.last_message,
                                "lastRunId": conversation.last_run_id,
                                "updatedAt": conversation.updated_at
                            })
                        })
                        .collect::<Vec<_>>();
                serde_json::to_string_pretty(&serde_json::json!({
                    "deviceSerial": snapshot.device_serial,
                    "package": snapshot.package,
                    "activity": snapshot.activity,
                    "pageId": snapshot.page_id,
                    "pageSignature": snapshot.page_signature,
                    "capturedAt": snapshot.captured_at,
                    "screenshotPath": snapshot.screenshot_path,
                    "uiPath": snapshot.ui_path,
                    "ocrPath": snapshot.ocr_path,
                    "ocrError": snapshot.ocr_error,
                    "modelScreenshotPath": snapshot.model_screenshot_path,
                    "modelScreenshotError": snapshot.model_screenshot_error,
                    "nodes": {
                        "total": snapshot.node_count,
                        "visible": snapshot.visible_node_count,
                        "actionable": snapshot.actionable_node_count,
                        "scrollable": snapshot.scrollable_node_count
                    },
                    "ocr": {
                        "engine": snapshot.ocr.as_ref().map(|ocr| ocr.engine.clone()),
                        "elapsedMs": snapshot.ocr.as_ref().map(|ocr| ocr.elapsed_ms),
                        "blocks": ocr_blocks.len(),
                        "linked": ocr_blocks.iter().filter(|block| block.matched_node_index.is_some()).count(),
                        "orphan": ocr_blocks.iter().filter(|block| block.matched_node_index.is_none()).count(),
                        "sample": ocr_blocks.iter().take(12).collect::<Vec<_>>()
                    },
                    "diagnostics": snapshot.diagnostics,
                    "efficiency": snapshot.efficiency,
                    "knowledge": knowledge,
                    "recentConversations": conversations,
                    "artifactCount": snapshot.artifacts.len()
                }))
                .unwrap_or_default()
            } else {
                serde_json::to_string_pretty(&snapshot).unwrap_or_default()
            };
            println!("{output}");
        }
        Err(error) => {
            eprintln!("debug snapshot failed: {error}");
            std::process::exit(1);
        }
    }
}
