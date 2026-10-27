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
