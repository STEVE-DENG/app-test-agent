use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use chrono::Utc;
use serde_json::json;
use tauri::{AppHandle, Emitter};

use crate::device;
use crate::explorer::{self, ExplorerDeps};
use crate::knowledge::KnowledgeStore;
use crate::models::{AgentEvent, LearningLoopReport, LearningRoundReport};

pub struct LearningDeps {
    pub app: Option<AppHandle>,
    pub knowledge: Arc<KnowledgeStore>,
    pub artifacts_dir: PathBuf,
    pub cancellation: Arc<AtomicBool>,
}

pub async fn run_learning_loop(
    deps: LearningDeps,
    run_id: String,
    device_serial: String,
    package: String,
    max_cycles: u32,
    max_steps_per_cycle: u32,
    stop_after_stagnant_cycles: u32,
) -> Result<LearningLoopReport, String> {
    let max_cycles = max_cycles.clamp(1, 100);
    let max_steps_per_cycle = max_steps_per_cycle.clamp(1, 300);
    let stop_after_stagnant_cycles = stop_after_stagnant_cycles.clamp(1, 20);
    let started_at = Utc::now().to_rfc3339();
    device::ensure_device_ready(&device_serial).await?;

    emit(
        deps.app.as_ref(),
        &run_id,
        "learning_started",
        "持续学习已启动",
        &format!(
            "{} · 最多 {} 轮 · 每轮 {} 步 · 连续 {} 轮无增长后停止",
            package, max_cycles, max_steps_per_cycle, stop_after_stagnant_cycles
        ),
        0,
        Some(json!({
            "package": package,
            "maxCycles": max_cycles,
            "maxStepsPerCycle": max_steps_per_cycle,
            "stopAfterStagnantCycles": stop_after_stagnant_cycles
        })),
    );

    let mut rounds = Vec::new();
    let mut total_steps = 0_u32;
    let mut stagnant_cycles = 0_u32;
    let mut status = "completed".to_string();

    for cycle in 1..=max_cycles {
        if deps.cancellation.load(Ordering::Relaxed) {
            status = "cancelled".to_string();
            break;
        }
        let before = deps.knowledge.get_plugin(&package)?;
        let cycle_started = Instant::now();
        let cycle_run_id = format!("{run_id}-cycle-{cycle:02}");
        let exploration = match explorer::run_exploration(
            ExplorerDeps {
                app: deps.app.clone(),
                knowledge: deps.knowledge.clone(),
                artifacts_dir: deps.artifacts_dir.clone(),
                cancellation: deps.cancellation.clone(),
            },
            cycle_run_id,
            device_serial.clone(),
            package.clone(),
            max_steps_per_cycle,
        )
        .await
        {
            Ok(report) => report,
            Err(error) => {
                emit(
                    deps.app.as_ref(),
                    &run_id,
                    "learning_failed",
                    "持续学习中断",
                    &error,
                    total_steps,
                    Some(json!({ "package": package, "cycle": cycle, "error": error.clone() })),
                );
                return Err(error);
            }
        };
        let after = deps.knowledge.get_plugin(&package)?;
        let new_pages = after.pages.len().saturating_sub(before.pages.len());
        let new_transitions = after
            .transitions
            .len()
            .saturating_sub(before.transitions.len());
        let round = LearningRoundReport {
            cycle,
            steps: exploration.steps,
            pages_before: before.pages.len(),
            pages_after: after.pages.len(),
            transitions_before: before.transitions.len(),
            transitions_after: after.transitions.len(),
            new_pages,
            new_transitions,
            elapsed_ms: cycle_started.elapsed().as_millis() as u64,
            summary: exploration.summary,
        };
        total_steps += exploration.steps;
        rounds.push(round.clone());
        if new_pages == 0 && new_transitions == 0 {
            stagnant_cycles += 1;
        } else {
            stagnant_cycles = 0;
        }
        emit(
            deps.app.as_ref(),
            &run_id,
            "learning_round_completed",
            "持续学习完成一轮",
            &format!(
                "第 {cycle} 轮：新增 {} 页面、{} 跳转；累计 {} 页面、{} 跳转",
                new_pages,
                new_transitions,
                after.pages.len(),
                after.transitions.len()
            ),
            total_steps,
            Some(json!({ "round": round, "package": package })),
        );
        if stagnant_cycles >= stop_after_stagnant_cycles {
            break;
        }
    }

    let tests = deps
        .knowledge
        .generate_test_cases(&package)
        .unwrap_or_default();
    let final_plugin = deps.knowledge.get_plugin(&package)?;
    let summary = format!(
        "{} 轮完成，共 {} 步；页面 {}，跳转 {}，测试用例 {}。",
        rounds.len(),
        total_steps,
        final_plugin.pages.len(),
        final_plugin.transitions.len(),
        tests.len()
    );
    let report = LearningLoopReport {
        package,
        status: status.clone(),
        cycles_completed: rounds.len() as u32,
        total_steps,
        pages: final_plugin.pages.len(),
        transitions: final_plugin.transitions.len(),
        test_cases: tests.len(),
        rounds,
        started_at,
        finished_at: Utc::now().to_rfc3339(),
        summary: summary.clone(),
    };
    emit(
        deps.app.as_ref(),
        &run_id,
        if status == "cancelled" {
            "learning_cancelled"
        } else {
            "learning_completed"
        },
        "持续学习结束",
        &summary,
        total_steps,
        Some(json!({ "report": report })),
    );
    Ok(report)
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
            timestamp: Utc::now().to_rfc3339(),
            data,
        },
    );
}
