use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;

use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::models::{
    EfficiencyStats, ModelTraceRecord, RunRecord, SkillHint, SkillPlanStep, SkillRecord,
    StepMetricRecord, ToolRecord,
};

pub struct AppStore {
    connection: Mutex<Connection>,
    pub data_dir: PathBuf,
}

impl AppStore {
    pub fn new(data_dir: PathBuf) -> Result<Self, String> {
        std::fs::create_dir_all(&data_dir)
            .map_err(|error| format!("创建应用数据目录失败: {error}"))?;
        let connection = Connection::open(data_dir.join("agent.db"))
            .map_err(|error| format!("打开任务数据库失败: {error}"))?;
        connection
            .execute_batch(
                r#"
                PRAGMA journal_mode = WAL;
                PRAGMA foreign_keys = ON;

                CREATE TABLE IF NOT EXISTS runs (
                    id TEXT PRIMARY KEY,
                    instruction TEXT NOT NULL,
                    device_serial TEXT NOT NULL,
                    status TEXT NOT NULL,
                    summary TEXT NOT NULL DEFAULT '',
                    model TEXT NOT NULL DEFAULT '',
                    started_at TEXT NOT NULL,
                    finished_at TEXT,
                    steps INTEGER NOT NULL DEFAULT 0,
                    skill_candidate_id TEXT
                );

                CREATE TABLE IF NOT EXISTS steps (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    run_id TEXT NOT NULL,
                    step_index INTEGER NOT NULL,
                    thought TEXT NOT NULL DEFAULT '',
                    tool TEXT NOT NULL,
                    args_json TEXT NOT NULL,
                    result_json TEXT NOT NULL,
                    page_signature_before TEXT NOT NULL DEFAULT '',
                    page_signature_after TEXT NOT NULL DEFAULT '',
                    screenshot_path TEXT NOT NULL DEFAULT '',
                    created_at TEXT NOT NULL
                );

                CREATE INDEX IF NOT EXISTS idx_steps_run_id
                ON steps(run_id, step_index);

                CREATE TABLE IF NOT EXISTS skills (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    description TEXT NOT NULL,
                    package TEXT NOT NULL DEFAULT '',
                    keywords_json TEXT NOT NULL,
                    plan_json TEXT NOT NULL,
                    status TEXT NOT NULL,
                    success_count INTEGER NOT NULL DEFAULT 0,
                    failure_count INTEGER NOT NULL DEFAULT 0,
                    reuse_count INTEGER NOT NULL DEFAULT 0,
                    source_run_id TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );

                CREATE INDEX IF NOT EXISTS idx_skills_status
                ON skills(status);

                CREATE TABLE IF NOT EXISTS step_metrics (
                    run_id TEXT NOT NULL,
                    step_index INTEGER NOT NULL,
                    model_ms INTEGER NOT NULL DEFAULT 0,
                    tool_ms INTEGER NOT NULL DEFAULT 0,
                    observation_ms INTEGER NOT NULL DEFAULT 0,
                    knowledge_ms INTEGER NOT NULL DEFAULT 0,
                    total_ms INTEGER NOT NULL DEFAULT 0,
                    created_at TEXT NOT NULL,
                    PRIMARY KEY (run_id, step_index)
                );

                CREATE TABLE IF NOT EXISTS model_traces (
                    run_id TEXT NOT NULL,
                    step_index INTEGER NOT NULL,
                    model TEXT NOT NULL DEFAULT '',
                    prompt_text TEXT NOT NULL DEFAULT '',
                    endpoint TEXT NOT NULL DEFAULT '',
                    prompt_chars INTEGER NOT NULL DEFAULT 0,
                    system_chars INTEGER NOT NULL DEFAULT 0,
                    node_count INTEGER NOT NULL DEFAULT 0,
                    use_vision INTEGER NOT NULL DEFAULT 0,
                    attempt_count INTEGER NOT NULL DEFAULT 0,
                    raw_response TEXT NOT NULL DEFAULT '',
                    usage_json TEXT NOT NULL DEFAULT 'null',
                    duration_ms INTEGER NOT NULL DEFAULT 0,
                    created_at TEXT NOT NULL,
                    PRIMARY KEY (run_id, step_index)
                );
                "#,
            )
            .map_err(|error| format!("初始化任务数据库失败: {error}"))?;

        // CREATE TABLE IF NOT EXISTS does not add new columns to databases
        // created by older releases. These migrations are intentionally idempotent.
        for migration in [
            "ALTER TABLE steps ADD COLUMN screenshot_after_path TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE steps ADD COLUMN page_id_before TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE steps ADD COLUMN page_id_after TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE model_traces ADD COLUMN prompt_text TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE model_traces ADD COLUMN endpoint TEXT NOT NULL DEFAULT ''",
        ] {
            let _ = connection.execute(migration, []);
        }

        Ok(Self {
            connection: Mutex::new(connection),
            data_dir,
        })
    }

    pub fn begin_run(
        &self,
        run_id: &str,
        instruction: &str,
        device_serial: &str,
        model: &str,
    ) -> Result<(), String> {
        let connection = self.lock()?;
        connection
            .execute(
                "INSERT INTO runs (id, instruction, device_serial, status, summary, model, started_at)
                 VALUES (?1, ?2, ?3, 'running', '', ?4, ?5)",
                params![
                    run_id,
                    instruction,
                    device_serial,
                    model,
                    Utc::now().to_rfc3339()
                ],
            )
            .map_err(|error| format!("创建任务记录失败: {error}"))?;
        Ok(())
    }

    pub fn save_step_for_run(&self, run_id: &str, record: &ToolRecord) -> Result<(), String> {
        let connection = self.lock()?;
        connection
            .execute(
                "INSERT INTO steps (
                    run_id, step_index, thought, tool, args_json, result_json,
                    page_signature_before, page_signature_after, screenshot_path,
                    screenshot_after_path, page_id_before, page_id_after, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    run_id,
                    record.step_index,
                    record.thought,
                    record.tool,
                    serde_json::to_string(&record.args).unwrap_or_else(|_| "{}".into()),
                    serde_json::to_string(&record.result).unwrap_or_else(|_| "{}".into()),
                    record.page_signature_before,
                    record.page_signature_after,
                    record.screenshot_path,
                    record.screenshot_after_path,
                    record.page_id_before,
                    record.page_id_after,
                    record.created_at
                ],
            )
            .map_err(|error| format!("保存执行步骤失败: {error}"))?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn save_step_metric(
        &self,
        run_id: &str,
        step_index: u32,
        model_ms: u64,
        tool_ms: u64,
        observation_ms: u64,
        knowledge_ms: u64,
        total_ms: u64,
    ) -> Result<(), String> {
        let connection = self.lock()?;
        connection
            .execute(
                "INSERT INTO step_metrics (
                    run_id, step_index, model_ms, tool_ms, observation_ms,
                    knowledge_ms, total_ms, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(run_id, step_index) DO UPDATE SET
                    model_ms = excluded.model_ms,
                    tool_ms = excluded.tool_ms,
                    observation_ms = excluded.observation_ms,
                    knowledge_ms = excluded.knowledge_ms,
                    total_ms = excluded.total_ms",
                params![
                    run_id,
                    step_index,
                    model_ms,
                    tool_ms,
                    observation_ms,
                    knowledge_ms,
                    total_ms,
                    Utc::now().to_rfc3339()
                ],
            )
            .map_err(|error| format!("保存步骤耗时失败: {error}"))?;
        Ok(())
    }

    pub fn finish_run(
        &self,
        run_id: &str,
        status: &str,
        summary: &str,
        steps: u32,
        skill_candidate_id: Option<&str>,
    ) -> Result<(), String> {
        let connection = self.lock()?;
        connection
            .execute(
                "UPDATE runs
                 SET status = ?2, summary = ?3, steps = ?4, finished_at = ?5,
                     skill_candidate_id = ?6
                 WHERE id = ?1",
                params![
                    run_id,
                    status,
                    summary,
                    steps,
                    Utc::now().to_rfc3339(),
                    skill_candidate_id
                ],
            )
            .map_err(|error| format!("更新任务结果失败: {error}"))?;
        Ok(())
    }

    pub fn list_runs(&self, limit: u32) -> Result<Vec<RunRecord>, String> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare(
                "SELECT id, instruction, device_serial, status, summary, model,
                        started_at, finished_at, steps
                 FROM runs
                 ORDER BY started_at DESC
                 LIMIT ?1",
            )
            .map_err(|error| format!("准备任务查询失败: {error}"))?;
        let rows = statement
            .query_map(params![limit.clamp(1, 200)], |row| {
                Ok(RunRecord {
                    id: row.get(0)?,
                    instruction: row.get(1)?,
                    device_serial: row.get(2)?,
                    status: row.get(3)?,
                    summary: row.get(4)?,
                    model: row.get(5)?,
                    started_at: row.get(6)?,
                    finished_at: row.get(7)?,
                    steps: row.get::<_, i64>(8)? as u32,
                })
            })
            .map_err(|error| format!("查询任务失败: {error}"))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("读取任务失败: {error}"))
    }

    pub fn list_run_steps(&self, run_id: &str) -> Result<Vec<ToolRecord>, String> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare(
                "SELECT step_index, thought, tool, args_json, result_json,
                        page_signature_before, page_signature_after, screenshot_path,
                        screenshot_after_path, page_id_before, page_id_after, created_at
                 FROM steps
                 WHERE run_id = ?1
                 ORDER BY step_index ASC",
            )
            .map_err(|error| format!("准备步骤查询失败: {error}"))?;
        let rows = statement
            .query_map(params![run_id], |row| {
                let args_json: String = row.get(3)?;
                let result_json: String = row.get(4)?;
                Ok(ToolRecord {
                    step_index: row.get::<_, i64>(0)? as u32,
                    thought: row.get(1)?,
                    tool: row.get(2)?,
                    args: serde_json::from_str(&args_json)
                        .unwrap_or_else(|_| serde_json::json!({})),
                    result: serde_json::from_str(&result_json)
                        .unwrap_or_else(|_| serde_json::json!({})),
                    page_signature_before: row.get(5)?,
                    page_signature_after: row.get(6)?,
                    screenshot_path: row.get(7)?,
                    screenshot_after_path: row.get(8)?,
                    page_id_before: row.get(9)?,
                    page_id_after: row.get(10)?,
                    created_at: row.get(11)?,
                })
            })
            .map_err(|error| format!("查询步骤失败: {error}"))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("读取步骤失败: {error}"))
    }

    pub fn list_step_metrics(&self, run_id: &str) -> Result<Vec<StepMetricRecord>, String> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare(
                "SELECT run_id, step_index, model_ms, tool_ms, observation_ms,
                        knowledge_ms, total_ms, created_at
                 FROM step_metrics
                 WHERE run_id = ?1
                 ORDER BY step_index ASC",
            )
            .map_err(|error| format!("准备步骤耗时查询失败: {error}"))?;
        let rows = statement
            .query_map(params![run_id], |row| {
                Ok(StepMetricRecord {
                    run_id: row.get(0)?,
                    step_index: row.get::<_, i64>(1)? as u32,
                    model_ms: row.get::<_, i64>(2)? as u64,
                    tool_ms: row.get::<_, i64>(3)? as u64,
                    observation_ms: row.get::<_, i64>(4)? as u64,
                    knowledge_ms: row.get::<_, i64>(5)? as u64,
                    total_ms: row.get::<_, i64>(6)? as u64,
                    created_at: row.get(7)?,
                })
            })
            .map_err(|error| format!("查询步骤耗时失败: {error}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("读取步骤耗时失败: {error}"))
    }

    pub fn save_model_trace(&self, trace: &ModelTraceRecord) -> Result<(), String> {
        let connection = self.lock()?;
        connection
            .execute(
                "INSERT INTO model_traces (
                    run_id, step_index, model, prompt_text, endpoint, prompt_chars,
                    system_chars, node_count, use_vision, attempt_count, raw_response,
                    usage_json, duration_ms, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
                 ON CONFLICT(run_id, step_index) DO UPDATE SET
                    model = excluded.model,
                    prompt_text = excluded.prompt_text,
                    endpoint = excluded.endpoint,
                    prompt_chars = excluded.prompt_chars,
                    system_chars = excluded.system_chars,
                    node_count = excluded.node_count,
                    use_vision = excluded.use_vision,
                    attempt_count = excluded.attempt_count,
                    raw_response = excluded.raw_response,
                    usage_json = excluded.usage_json,
                    duration_ms = excluded.duration_ms,
                    created_at = excluded.created_at",
                params![
                    trace.run_id,
                    trace.step_index,
                    trace.model,
                    trace.prompt_text,
                    trace.endpoint,
                    trace.prompt_chars as i64,
                    trace.system_chars as i64,
                    trace.node_count as i64,
                    if trace.use_vision { 1 } else { 0 },
                    trace.attempt_count as i64,
                    trace.raw_response,
                    serde_json::to_string(&trace.usage)
                        .map_err(|error| format!("序列化模型 usage 失败: {error}"))?,
                    trace.duration_ms as i64,
                    trace.created_at,
                ],
            )
            .map_err(|error| format!("保存模型轨迹失败: {error}"))?;
        Ok(())
    }

    pub fn list_model_traces(&self, run_id: &str) -> Result<Vec<ModelTraceRecord>, String> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare(
                "SELECT run_id, step_index, model, prompt_chars, system_chars, node_count,
                        use_vision, attempt_count, raw_response, usage_json, duration_ms,
                        created_at, prompt_text, endpoint
                 FROM model_traces
                 WHERE run_id = ?1
                 ORDER BY step_index ASC",
            )
            .map_err(|error| format!("准备模型轨迹查询失败: {error}"))?;
        let rows = statement
            .query_map(params![run_id], |row| {
                let usage_json: String = row.get(9)?;
                Ok(ModelTraceRecord {
                    run_id: row.get(0)?,
                    step_index: row.get::<_, i64>(1)? as u32,
                    model: row.get(2)?,
                    prompt_text: row.get(12)?,
                    endpoint: row.get(13)?,
                    prompt_chars: row.get::<_, i64>(3)? as usize,
                    system_chars: row.get::<_, i64>(4)? as usize,
                    node_count: row.get::<_, i64>(5)? as usize,
                    use_vision: row.get::<_, i64>(6)? != 0,
                    attempt_count: row.get::<_, i64>(7)? as u32,
                    raw_response: row.get(8)?,
                    usage: serde_json::from_str(&usage_json).unwrap_or(serde_json::Value::Null),
                    duration_ms: row.get::<_, i64>(10)? as u64,
                    created_at: row.get(11)?,
                })
            })
            .map_err(|error| format!("查询模型轨迹失败: {error}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("读取模型轨迹失败: {error}"))
    }

    pub fn create_skill_candidate(
        &self,
        run_id: &str,
        instruction: &str,
        package: &str,
        records: &[ToolRecord],
    ) -> Result<Option<SkillRecord>, String> {
        let mut seen_steps = HashSet::new();
        let executable = records
            .iter()
            .filter(|record| {
                !matches!(
                    record.tool.as_str(),
                    "observe_screen" | "finish" | "ask_user"
                )
            })
            .filter(|record| !is_redundant_step(record))
            .filter(|record| {
                seen_steps.insert(format!(
                    "{}|{}|{}|{}",
                    record.tool,
                    record.args,
                    record.page_signature_before,
                    record.page_signature_after
                ))
            })
            .take(20)
            .collect::<Vec<_>>();
        if executable.len() < 2 {
            return Ok(None);
        }

        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let name = truncate_chars(instruction, 26);
        let description = format!("成功路线：{instruction}");
        let keywords = extract_keywords(instruction);
        let plan = executable
            .iter()
            .map(|record| SkillPlanStep {
                tool: record.tool.clone(),
                args: record.args.clone(),
                note: result_summary(&record.result),
            })
            .collect::<Vec<_>>();

        let skill = SkillRecord {
            id: id.clone(),
            name,
            description,
            package: package.to_string(),
            keywords,
            plan,
            status: "pending".to_string(),
            success_count: 0,
            failure_count: 0,
            reuse_count: 0,
            source_run_id: run_id.to_string(),
            created_at: now.clone(),
            updated_at: now,
        };

        let connection = self.lock()?;
        connection
            .execute(
                "INSERT INTO skills (
                    id, name, description, package, keywords_json, plan_json,
                    status, success_count, failure_count, reuse_count,
                    source_run_id, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, 0, 0, ?8, ?9, ?10)",
                params![
                    skill.id,
                    skill.name,
                    skill.description,
                    skill.package,
                    serde_json::to_string(&skill.keywords)
                        .map_err(|error| format!("序列化 Skill 关键词失败: {error}"))?,
                    serde_json::to_string(&skill.plan)
                        .map_err(|error| format!("序列化 Skill 计划失败: {error}"))?,
                    skill.status,
                    skill.source_run_id,
                    skill.created_at,
                    skill.updated_at
                ],
            )
            .map_err(|error| format!("创建 Skill 候选失败: {error}"))?;

        Ok(Some(skill))
    }

    pub fn retrieve_skills(
        &self,
        instruction: &str,
        package: &str,
        limit: usize,
    ) -> Result<Vec<SkillHint>, String> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare(
                "SELECT id, name, description, package, keywords_json, plan_json
                 FROM skills
                 WHERE status = 'approved'",
            )
            .map_err(|error| format!("准备 Skill 查询失败: {error}"))?;
        let rows = statement
            .query_map([], |row| {
                let keywords_json: String = row.get(4)?;
                let plan_json: String = row.get(5)?;
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    serde_json::from_str::<Vec<String>>(&keywords_json).unwrap_or_default(),
                    serde_json::from_str::<Vec<SkillPlanStep>>(&plan_json).unwrap_or_default(),
                ))
            })
            .map_err(|error| format!("查询 Skill 失败: {error}"))?;

        let normalized_instruction = instruction.to_lowercase();
        let mut scored = Vec::new();
        for row in rows {
            let (id, name, description, skill_package, keywords, plan) =
                row.map_err(|error| format!("读取 Skill 失败: {error}"))?;
            if !package.is_empty() && !skill_package.is_empty() && skill_package != package {
                continue;
            }
            let mut score = 0_i32;
            if !package.is_empty() && !skill_package.is_empty() && skill_package == package {
                score += 6;
            }
            for keyword in &keywords {
                if keyword.chars().count() >= 2
                    && normalized_instruction.contains(&keyword.to_lowercase())
                {
                    score += if keyword.chars().count() >= 3 { 3 } else { 1 };
                }
            }
            if score > 0 {
                scored.push((
                    score,
                    SkillHint {
                        id,
                        name,
                        description,
                        package: skill_package,
                        plan,
                    },
                ));
            }
        }
        scored.sort_by(|left, right| right.0.cmp(&left.0));
        Ok(scored
            .into_iter()
            .take(limit.clamp(1, 3))
            .map(|(_, hint)| hint)
            .collect())
    }

    pub fn list_skills(&self) -> Result<Vec<SkillRecord>, String> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare(
                "SELECT id, name, description, package, keywords_json, plan_json,
                        status, success_count, failure_count, reuse_count,
                        source_run_id, created_at, updated_at
                 FROM skills
                 ORDER BY
                    CASE status WHEN 'pending' THEN 0 WHEN 'approved' THEN 1 ELSE 2 END,
                    updated_at DESC",
            )
            .map_err(|error| format!("准备 Skill 列表查询失败: {error}"))?;
        let rows = statement
            .query_map([], |row| {
                let keywords_json: String = row.get(4)?;
                let plan_json: String = row.get(5)?;
                Ok(SkillRecord {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    package: row.get(3)?,
                    keywords: serde_json::from_str(&keywords_json).unwrap_or_default(),
                    plan: serde_json::from_str(&plan_json).unwrap_or_default(),
                    status: row.get(6)?,
                    success_count: row.get::<_, i64>(7)? as u32,
                    failure_count: row.get::<_, i64>(8)? as u32,
                    reuse_count: row.get::<_, i64>(9)? as u32,
                    source_run_id: row.get(10)?,
                    created_at: row.get(11)?,
                    updated_at: row.get(12)?,
                })
            })
            .map_err(|error| format!("查询 Skill 列表失败: {error}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("读取 Skill 列表失败: {error}"))
    }

    pub fn set_skill_status(&self, id: &str, status: &str) -> Result<(), String> {
        if !matches!(status, "approved" | "rejected" | "archived") {
            return Err("不支持的 Skill 状态".to_string());
        }
        let connection = self.lock()?;
        connection
            .execute(
                "UPDATE skills SET status = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, status, Utc::now().to_rfc3339()],
            )
            .map_err(|error| format!("更新 Skill 状态失败: {error}"))?;
        Ok(())
    }

    pub fn mark_skills_used(&self, ids: &[String]) -> Result<(), String> {
        let connection = self.lock()?;
        for id in ids {
            connection
                .execute(
                    "UPDATE skills
                     SET reuse_count = reuse_count + 1, updated_at = ?2
                     WHERE id = ?1",
                    params![id, Utc::now().to_rfc3339()],
                )
                .map_err(|error| format!("更新 Skill 复用次数失败: {error}"))?;
        }
        Ok(())
    }

    pub fn record_skill_outcome(&self, ids: &[String], success: bool) -> Result<(), String> {
        let connection = self.lock()?;
        for id in ids {
            let field = if success {
                "success_count"
            } else {
                "failure_count"
            };
            connection
                .execute(
                    &format!(
                        "UPDATE skills SET {field} = {field} + 1, updated_at = ?2 WHERE id = ?1"
                    ),
                    params![id, Utc::now().to_rfc3339()],
                )
                .map_err(|error| format!("更新 Skill 质量统计失败: {error}"))?;
        }
        Ok(())
    }

    pub fn skill_exists(&self, source_run_id: &str) -> Result<bool, String> {
        let connection = self.lock()?;
        let existing = connection
            .query_row(
                "SELECT id FROM skills WHERE source_run_id = ?1 LIMIT 1",
                params![source_run_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| format!("检查 Skill 是否存在失败: {error}"))?;
        Ok(existing.is_some())
    }

    pub fn efficiency_stats(&self) -> Result<EfficiencyStats, String> {
        let connection = self.lock()?;
        let (
            total_runs,
            completed_runs,
            failed_runs,
            total_steps,
            total_model_ms,
            total_tool_ms,
            total_observation_ms,
            total_knowledge_ms,
            total_step_ms,
            avg_step_ms,
        ) = connection
            .query_row(
                "SELECT
                    COUNT(*) AS total_runs,
                    COALESCE(SUM(CASE WHEN status = 'completed' THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM(CASE WHEN status IN ('failed', 'cancelled') THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM(steps), 0),
                    COALESCE((SELECT SUM(model_ms) FROM step_metrics), 0),
                    COALESCE((SELECT SUM(tool_ms) FROM step_metrics), 0),
                    COALESCE((SELECT SUM(observation_ms) FROM step_metrics), 0),
                    COALESCE((SELECT SUM(knowledge_ms) FROM step_metrics), 0),
                    COALESCE((SELECT SUM(total_ms) FROM step_metrics), 0),
                    COALESCE((SELECT AVG(total_ms) FROM step_metrics), 0)
                 FROM runs",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)? as u32,
                        row.get::<_, i64>(1)? as u32,
                        row.get::<_, i64>(2)? as u32,
                        row.get::<_, i64>(3)? as u32,
                        row.get::<_, i64>(4)? as u64,
                        row.get::<_, i64>(5)? as u64,
                        row.get::<_, i64>(6)? as u64,
                        row.get::<_, i64>(7)? as u64,
                        row.get::<_, i64>(8)? as u64,
                        row.get::<_, f64>(9)?,
                    ))
                },
            )
            .map_err(|error| format!("查询运行效率失败: {error}"))?;
        let pending_skills = connection
            .query_row(
                "SELECT COUNT(*) FROM skills WHERE status = 'pending'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| format!("查询待审核 Skill 失败: {error}"))?
            as u32;
        let approved_skills = connection
            .query_row(
                "SELECT COUNT(*) FROM skills WHERE status = 'approved'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| format!("查询已批准 Skill 失败: {error}"))?
            as u32;
        let avg_steps = if total_runs == 0 {
            0.0
        } else {
            total_steps as f64 / total_runs as f64
        };
        Ok(EfficiencyStats {
            total_runs,
            completed_runs,
            failed_runs,
            total_steps,
            avg_steps,
            total_model_ms,
            total_tool_ms,
            total_observation_ms,
            total_knowledge_ms,
            total_step_ms,
            avg_step_ms,
            pending_skills,
            approved_skills,
            pages: 0,
            transitions: 0,
            test_cases: 0,
            artifacts: 0,
            updated_at: Utc::now().to_rfc3339(),
        })
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, String> {
        self.connection
            .lock()
            .map_err(|_| "任务数据库锁已损坏".to_string())
    }
}

pub fn extract_keywords(text: &str) -> Vec<String> {
    let mut keywords = HashSet::new();
    let stopwords = [
        "帮我", "一下", "然后", "打开", "进行", "这个", "那个", "可以", "需要", "并且", "点击",
        "搜索", "找到", "完成", "任务", "自动", "应用", "app",
    ];

    for word in text
        .split(|character: char| {
            character.is_whitespace()
                || matches!(
                    character,
                    ',' | '，' | '。' | '.' | '!' | '！' | '?' | '？' | ':' | '：' | ';' | '；'
                )
        })
        .map(str::trim)
        .filter(|word| !word.is_empty())
    {
        let chars = word.chars().collect::<Vec<_>>();
        if chars.len() >= 2 && chars.len() <= 12 && !stopwords.contains(&word) {
            keywords.insert(word.to_lowercase());
        }
        if chars.iter().any(|character| is_cjk(*character)) {
            for size in 2..=4 {
                if chars.len() < size {
                    break;
                }
                for window in chars.windows(size) {
                    let candidate = window.iter().collect::<String>();
                    if !stopwords.contains(&candidate.as_str()) {
                        keywords.insert(candidate);
                    }
                }
            }
        }
    }

    let mut values = keywords.into_iter().collect::<Vec<_>>();
    values.sort_by_key(|value| std::cmp::Reverse(value.chars().count()));
    values.truncate(20);
    values
}

fn is_cjk(character: char) -> bool {
    matches!(
        character as u32,
        0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xF900..=0xFAFF
            | 0x3040..=0x30FF
            | 0xAC00..=0xD7AF
    )
}

fn truncate_chars(text: &str, limit: usize) -> String {
    let mut result = text.chars().take(limit).collect::<String>();
    if text.chars().count() > limit {
        result.push('…');
    }
    result
}

fn result_summary(result: &serde_json::Value) -> String {
    result
        .get("message")
        .and_then(|value| value.as_str())
        .or_else(|| result.get("error").and_then(|value| value.as_str()))
        .unwrap_or("已验证的历史步骤")
        .chars()
        .take(80)
        .collect()
}

fn is_redundant_step(record: &ToolRecord) -> bool {
    if record.tool == "wait" && record.page_signature_before == record.page_signature_after {
        return true;
    }
    if matches!(
        record.tool.as_str(),
        "tap" | "tap_text" | "tap_id" | "long_press"
    ) && record.page_signature_before == record.page_signature_after
    {
        return true;
    }
    let args = record.args.to_string();
    let result = record.result.to_string();
    args == "{}" && result.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_chinese_keywords() {
        let keywords = extract_keywords("在小红书搜索机械键盘并记录价格");
        assert!(keywords.iter().any(|keyword| keyword.contains("小红书")));
        assert!(keywords.iter().any(|keyword| keyword.contains("机械键盘")));
    }

    #[test]
    fn truncates_unicode_safely() {
        assert_eq!(truncate_chars("一二三四五", 3), "一二三…");
    }

    #[test]
    fn persists_approves_and_recalls_skill() {
        let data_dir =
            std::env::temp_dir().join(format!("ai-apptest-agent-test-{}", Uuid::new_v4()));
        let store = AppStore::new(data_dir.clone()).expect("create store");
        let run_id = "run-skill-test";
        let instruction = "在小红书搜索机械键盘并记录价格";
        let package = "com.xingin.xhs";
        store
            .begin_run(run_id, instruction, "emulator-5554", "test-model")
            .expect("begin run");
        let records = vec![
            ToolRecord {
                step_index: 1,
                thought: "打开搜索框".to_string(),
                tool: "tap_text".to_string(),
                args: json!({ "text": "搜索" }),
                result: json!({ "message": "已点击搜索" }),
                page_signature_before: "before".to_string(),
                page_signature_after: "after".to_string(),
                screenshot_path: String::new(),
                screenshot_after_path: String::new(),
                page_id_before: String::new(),
                page_id_after: String::new(),
                created_at: Utc::now().to_rfc3339(),
            },
            ToolRecord {
                step_index: 2,
                thought: "输入关键词并提交".to_string(),
                tool: "input_text".to_string(),
                args: json!({ "text": "机械键盘", "clear": true }),
                result: json!({ "message": "已输入机械键盘" }),
                page_signature_before: "after".to_string(),
                page_signature_after: "results".to_string(),
                screenshot_path: String::new(),
                screenshot_after_path: String::new(),
                page_id_before: String::new(),
                page_id_after: String::new(),
                created_at: Utc::now().to_rfc3339(),
            },
        ];

        let skill = store
            .create_skill_candidate(run_id, instruction, package, &records)
            .expect("create skill")
            .expect("skill candidate");
        assert_eq!(skill.status, "pending");
        assert_eq!(skill.plan.len(), 2);
        assert!(
            store
                .retrieve_skills(instruction, package, 2)
                .expect("retrieve pending skills")
                .is_empty()
        );

        store
            .set_skill_status(&skill.id, "approved")
            .expect("approve skill");
        let recalled = store
            .retrieve_skills(instruction, package, 2)
            .expect("retrieve approved skills");
        assert_eq!(recalled.len(), 1);
        assert_eq!(recalled[0].package, package);

        store
            .mark_skills_used(&[skill.id.clone()])
            .expect("mark skill used");
        store
            .record_skill_outcome(&[skill.id.clone()], true)
            .expect("record skill success");
        let stored = store
            .list_skills()
            .expect("list skills")
            .into_iter()
            .find(|item| item.id == skill.id)
            .expect("stored skill");
        assert_eq!(stored.reuse_count, 1);
        assert_eq!(stored.success_count, 1);

        drop(store);
        let _ = std::fs::remove_dir_all(data_dir);
    }
}
