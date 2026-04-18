//! Tauri commands for the Rules Engine (Theme E2-d).

use tauri::AppHandle;

use crate::rules::{self, executor, ExecutionReport, Rule};

#[tauri::command]
pub async fn list_rules(app: AppHandle) -> Result<Vec<Rule>, String> {
    Ok(rules::load_all(&app))
}

#[tauri::command]
pub async fn create_rule(app: AppHandle, mut rule: Rule) -> Result<Rule, String> {
    if rule.id.is_empty() {
        rule.id = uuid::Uuid::new_v4().to_string();
    }
    rules::upsert(&app, rule.clone()).map_err(|e| e.to_string())?;
    Ok(rule)
}

#[tauri::command]
pub async fn update_rule(app: AppHandle, id: String, mut rule: Rule) -> Result<(), String> {
    rule.id = id;
    rules::upsert(&app, rule).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_rule(app: AppHandle, id: String) -> Result<(), String> {
    rules::delete(&app, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn preview_rule_hits(app: AppHandle, rule: Rule) -> Result<Vec<String>, String> {
    executor::preview(&app, &rule).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn run_rule_now(app: AppHandle, id: String) -> Result<ExecutionReport, String> {
    let all = rules::load_all(&app);
    let rule = all
        .into_iter()
        .find(|r| r.id == id)
        .ok_or_else(|| format!("Rule not found: {id}"))?;
    executor::execute(&app, &rule).map_err(|e| e.to_string())
}
