use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
    sync::Mutex,
};
use tauri::Manager;
use uuid::Uuid;

struct AppState(Mutex<Connection>);

const CODEX_MODELS: &[&str] = &["gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna"];
const CLAUDE_MODELS: &[&str] = &["claude-opus-5", "claude-sonnet-5", "claude-haiku-4-5-20251001"];
const DEFAULT_REASONING: &[&str] = &["low", "medium", "high", "xhigh", "max"];

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Preferences {
    max_concurrent_tasks: u32,
    enabled_agents: Vec<String>,
    allowed_models: Vec<String>,
    allowed_reasoning: Vec<String>,
    execution_profile: String,
    remote_endpoint: String,
    #[serde(default)]
    remote_token: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct QueueState {
    ticket_id: String,
    position: u32,
    retry_required: bool,
    last_error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Repository {
    id: String,
    name: String,
    description: String,
    path: String,
    created_at: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateRepository {
    name: String,
    description: String,
    path: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Ticket {
    id: String,
    title: String,
    description: String,
    kind: String,
    status: String,
    agent: String,
    model: String,
    reasoning: String,
    workspace: String,
    branch: Option<String>,
    parent_id: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateTicket {
    title: String,
    description: String,
    kind: String,
    agent: String,
    model: String,
    reasoning: String,
    workspace: String,
    repository_id: Option<String>,
    parent_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateTicket {
    title: String,
    description: String,
    kind: String,
    agent: String,
    model: String,
    reasoning: String,
    workspace: String,
    repository_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GitSnapshot {
    base_commit: Option<String>,
    working_tree_status: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Preflight {
    dirty_worktree: bool,
    active_execution: bool,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PlanItem {
    title: String,
    description: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Plan {
    ticket_id: String,
    items: Vec<PlanItem>,
    approved_at: Option<String>,
}

struct AgentInvocation {
    program: &'static str,
    args: Vec<String>,
}

trait AgentAdapter {
    fn cli_name(&self) -> &'static str;
    fn is_available(&self) -> bool {
        Command::new(self.cli_name())
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success())
    }
    fn start(&self, ticket: &Ticket, prompt: &str) -> AgentInvocation;
    fn resume(&self, ticket: &Ticket, session_id: &str, prompt: &str) -> Option<AgentInvocation>;
    fn session_id_from_output(&self, _output: &str) -> Option<String> {
        None
    }
}

struct CodexAdapter;
impl AgentAdapter for CodexAdapter {
    fn cli_name(&self) -> &'static str {
        "codex"
    }
    fn start(&self, ticket: &Ticket, prompt: &str) -> AgentInvocation {
        AgentInvocation {
            program: "codex",
            args: vec![
                "exec".into(),
                "-m".into(),
                ticket.model.clone(),
                "-c".into(),
                format!("model_reasoning_effort=\"{}\"", ticket.reasoning),
                "--approve-for-me".into(),
                "--json".into(),
                prompt.into(),
            ],
        }
    }
    fn resume(&self, _ticket: &Ticket, session_id: &str, prompt: &str) -> Option<AgentInvocation> {
        Some(AgentInvocation {
            program: "codex",
            args: vec![
                "exec".into(),
                "resume".into(),
                session_id.into(),
                prompt.into(),
            ],
        })
    }
    fn session_id_from_output(&self, output: &str) -> Option<String> {
        output.lines().find_map(|line| {
            let event: serde_json::Value = serde_json::from_str(line).ok()?;
            (event.get("type")?.as_str()? == "thread.started")
                .then(|| event.get("thread_id")?.as_str().map(str::to_owned))
                .flatten()
        })
    }
}

struct ClaudeAdapter;
impl AgentAdapter for ClaudeAdapter {
    fn cli_name(&self) -> &'static str {
        "claude"
    }
    fn start(&self, ticket: &Ticket, prompt: &str) -> AgentInvocation {
        AgentInvocation {
            program: "claude",
            args: vec![
                "--print".into(),
                "--model".into(),
                ticket.model.clone(),
                "--effort".into(),
                ticket.reasoning.clone(),
                "--permission-mode".into(),
                "acceptEdits".into(),
                "--output-format".into(),
                "json".into(),
                prompt.into(),
            ],
        }
    }
    fn resume(&self, ticket: &Ticket, session_id: &str, prompt: &str) -> Option<AgentInvocation> {
        Some(AgentInvocation {
            program: "claude",
            args: vec![
                "--resume".into(),
                session_id.into(),
                "--print".into(),
                "--model".into(),
                ticket.model.clone(),
                "--effort".into(),
                ticket.reasoning.clone(),
                "--permission-mode".into(),
                "acceptEdits".into(),
                prompt.into(),
            ],
        })
    }
    fn session_id_from_output(&self, output: &str) -> Option<String> {
        serde_json::from_str::<serde_json::Value>(output)
            .ok()?
            .get("session_id")?
            .as_str()
            .map(str::to_owned)
    }
}

fn adapter_for(agent: &str) -> Result<Box<dyn AgentAdapter>, String> {
    match agent {
        "codex" => Ok(Box::new(CodexAdapter)),
        "claude" => Ok(Box::new(ClaudeAdapter)),
        _ => Err("Agente no compatible.".into()),
    }
}

fn default_preferences() -> Preferences {
    Preferences {
        max_concurrent_tasks: 1,
        enabled_agents: vec!["codex".into(), "claude".into()],
        allowed_models: CODEX_MODELS
            .iter()
            .chain(CLAUDE_MODELS.iter())
            .map(|value| (*value).into())
            .collect(),
        allowed_reasoning: DEFAULT_REASONING
            .iter()
            .map(|value| (*value).into())
            .collect(),
        execution_profile: "local".into(),
        remote_endpoint: String::new(),
        remote_token: String::new(),
    }
}

fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE IF NOT EXISTS tickets (id TEXT PRIMARY KEY,title TEXT NOT NULL,description TEXT NOT NULL,kind TEXT NOT NULL,status TEXT NOT NULL,agent TEXT NOT NULL,model TEXT NOT NULL,reasoning TEXT NOT NULL,workspace TEXT NOT NULL,branch TEXT,parent_id TEXT,created_at TEXT NOT NULL,updated_at TEXT NOT NULL); CREATE TABLE IF NOT EXISTS repositories (id TEXT PRIMARY KEY,name TEXT NOT NULL,description TEXT NOT NULL DEFAULT '',path TEXT NOT NULL UNIQUE,created_at TEXT NOT NULL); CREATE TABLE IF NOT EXISTS ticket_repositories (ticket_id TEXT PRIMARY KEY,repository_id TEXT NOT NULL,FOREIGN KEY(ticket_id) REFERENCES tickets(id),FOREIGN KEY(repository_id) REFERENCES repositories(id)); CREATE TABLE IF NOT EXISTS ticket_archives (ticket_id TEXT PRIMARY KEY,archived_at TEXT NOT NULL,FOREIGN KEY(ticket_id) REFERENCES tickets(id)); CREATE TABLE IF NOT EXISTS executions (id TEXT PRIMARY KEY,ticket_id TEXT NOT NULL,session_id TEXT,status TEXT NOT NULL,effective_config TEXT NOT NULL,base_commit TEXT,started_at TEXT NOT NULL,finished_at TEXT,summary TEXT,parent_execution_id TEXT,follow_up_message TEXT,report_version INTEGER NOT NULL DEFAULT 1); CREATE TABLE IF NOT EXISTS execution_logs (id TEXT PRIMARY KEY,execution_id TEXT NOT NULL,content TEXT NOT NULL,created_at TEXT NOT NULL); CREATE TABLE IF NOT EXISTS execution_reports (id TEXT PRIMARY KEY,execution_id TEXT NOT NULL UNIQUE,changes_detected TEXT NOT NULL,created_at TEXT NOT NULL); CREATE TABLE IF NOT EXISTS review_messages (id TEXT PRIMARY KEY,ticket_id TEXT NOT NULL,execution_id TEXT,role TEXT NOT NULL,content TEXT NOT NULL,created_at TEXT NOT NULL); CREATE TABLE IF NOT EXISTS plans (ticket_id TEXT PRIMARY KEY,items TEXT NOT NULL,approved_at TEXT,created_at TEXT NOT NULL); CREATE TABLE IF NOT EXISTS application_preferences (id INTEGER PRIMARY KEY CHECK(id=1), value TEXT NOT NULL); CREATE TABLE IF NOT EXISTS execution_queue (ticket_id TEXT PRIMARY KEY,queued_at TEXT NOT NULL,retry_required INTEGER NOT NULL DEFAULT 0,last_error TEXT,FOREIGN KEY(ticket_id) REFERENCES tickets(id));")?;
    // Upgrades databases created by earlier development builds.
    for statement in [
        "ALTER TABLE executions ADD COLUMN parent_execution_id TEXT",
        "ALTER TABLE executions ADD COLUMN follow_up_message TEXT",
        "ALTER TABLE executions ADD COLUMN report_version INTEGER NOT NULL DEFAULT 1",
    ] {
        let _ = db.execute(statement, []);
    }
    let historical_tickets = {
        let mut tickets = db.prepare("SELECT id,workspace FROM tickets WHERE workspace <> ''")?;
        let entries = tickets
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        entries
    };
    for (ticket_id, path) in historical_tickets {
        let repository_id: Option<String> = db
            .query_row(
                "SELECT id FROM repositories WHERE path=?1",
                params![path],
                |r| r.get(0),
            )
            .optional()?;
        let repository_id = match repository_id {
            Some(id) => id,
            None => {
                let id = Uuid::new_v4().to_string();
                db.execute(
                    "INSERT INTO repositories VALUES(?1,?2,'',?3,?4)",
                    params![id, path, path, Utc::now().to_rfc3339()],
                )?;
                id
            }
        };
        db.execute(
            "INSERT OR IGNORE INTO ticket_repositories VALUES(?1,?2)",
            params![ticket_id, repository_id],
        )?;
    }
    let preferences = serde_json::to_string(&default_preferences()).unwrap();
    db.execute(
        "INSERT OR IGNORE INTO application_preferences (id,value) VALUES(1,?1)",
        params![preferences],
    )?;
    db.execute("INSERT OR IGNORE INTO execution_queue (ticket_id,queued_at) SELECT id,updated_at FROM tickets WHERE status='todo'", [])?;
    Ok(())
}

fn preferences(db: &Connection) -> Result<Preferences, String> {
    let value: String = db
        .query_row(
            "SELECT value FROM application_preferences WHERE id=1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    serde_json::from_str(&value).map_err(|_| "Las preferencias guardadas no son válidas.".into())
}

fn validate_preferences(value: &Preferences) -> Result<(), String> {
    if value.max_concurrent_tasks == 0 {
        return Err("El límite de concurrencia debe ser mayor que cero.".into());
    }
    if value.execution_profile != "local" && value.execution_profile != "remote" {
        return Err("El perfil de ejecución es inválido.".into());
    }
    if value.enabled_agents.is_empty()
        || value.allowed_models.is_empty()
        || value.allowed_reasoning.is_empty()
    {
        return Err(
            "La política debe conservar al menos un agente, modelo y nivel de razonamiento.".into(),
        );
    }
    if value
        .enabled_agents
        .iter()
        .any(|agent| !matches!(agent.as_str(), "codex" | "claude"))
    {
        return Err("Los agentes disponibles son Codex y Claude.".into());
    }
    Ok(())
}

fn validate_execution_policy(db: &Connection, ticket: &Ticket) -> Result<Preferences, String> {
    let value = preferences(db)?;
    validate_preferences(&value)?;
    if value.execution_profile == "remote" {
        return Err(
            "El perfil Remoto está guardado, pero la ejecución remota todavía no está disponible."
                .into(),
        );
    }
    if !value.enabled_agents.contains(&ticket.agent)
        || !value.allowed_models.contains(&ticket.model)
        || !value.allowed_reasoning.contains(&ticket.reasoning)
    {
        return Err("La configuración del ticket no está permitida por la política actual.".into());
    }
    Ok(value)
}

#[tauri::command]
fn get_preferences(state: tauri::State<'_, AppState>) -> Result<Preferences, String> {
    let db = state.0.lock().map_err(|_| "Base de datos no disponible")?;
    preferences(&db)
}

#[tauri::command]
fn update_preferences(
    input: Preferences,
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<Preferences, String> {
    validate_preferences(&input)?;
    let db = state.0.lock().map_err(|_| "Base de datos no disponible")?;
    db.execute(
        "UPDATE application_preferences SET value=?1 WHERE id=1",
        params![serde_json::to_string(&input).map_err(|e| e.to_string())?],
    )
    .map_err(|e| e.to_string())?;
    drop(db);
    schedule_queued(&app);
    Ok(input)
}

#[tauri::command]
fn test_remote_connection(endpoint: String, token: String) -> Result<String, String> {
    let base = endpoint.trim_end_matches('/');
    if base.is_empty() {
        return Err("Ingresá el endpoint remoto.".into());
    }
    ureq::get(&format!("{base}/health"))
        .timeout(std::time::Duration::from_secs(5))
        .call()
        .map_err(|e| format!("No se pudo conectar con el servidor: {e}"))?;
    ureq::get(&format!("{base}/tickets"))
        .set("Authorization", &format!("Bearer {token}"))
        .timeout(std::time::Duration::from_secs(5))
        .call()
        .map_err(|e| format!("El servidor respondió, pero el token no fue aceptado: {e}"))?;
    Ok("Conexión verificada: el servidor remoto está disponible y el token es válido.".into())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteRequest {
    endpoint: String,
    token: String,
    method: String,
    path: String,
    body: Option<serde_json::Value>,
}

// Generic proxy to a palantir-server instance. The frontend calls this instead
// of `fetch`-ing the remote endpoint directly: a Tauri webview enforces CORS
// like a browser would, and palantir-server doesn't send CORS headers, so a
// direct frontend fetch would silently fail. A native HTTP request from Rust
// isn't subject to that.
#[tauri::command]
fn remote_request(request: RemoteRequest) -> Result<serde_json::Value, String> {
    let base = request.endpoint.trim_end_matches('/');
    if base.is_empty() {
        return Err("Configurá el endpoint remoto en Configuración.".into());
    }
    let url = format!("{base}{}", request.path);
    let call = match request.method.as_str() {
        "GET" => ureq::get(&url),
        "POST" => ureq::post(&url),
        "PUT" => ureq::put(&url),
        "DELETE" => ureq::delete(&url),
        other => return Err(format!("Método no soportado: {other}")),
    }
    .set("Authorization", &format!("Bearer {}", request.token))
    .timeout(std::time::Duration::from_secs(20));
    let result = match &request.body {
        Some(body) => call.send_json(body.clone()),
        None => call.call(),
    };
    match result {
        Ok(response) => {
            if response.status() == 204 {
                return Ok(serde_json::Value::Null);
            }
            response.into_json().map_err(|e| e.to_string())
        }
        Err(ureq::Error::Status(code, response)) => {
            let raw = response.into_string().unwrap_or_default();
            let message = serde_json::from_str::<serde_json::Value>(&raw)
                .ok()
                .and_then(|value| value.get("error")?.as_str().map(str::to_owned))
                .unwrap_or(raw);
            Err(if message.is_empty() {
                format!("El servidor remoto respondió con estado {code}.")
            } else {
                message
            })
        }
        Err(e) => Err(format!("No se pudo conectar con el servidor remoto: {e}")),
    }
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Ticket> {
    Ok(Ticket {
        id: r.get(0)?,
        title: r.get(1)?,
        description: r.get(2)?,
        kind: r.get(3)?,
        status: r.get(4)?,
        agent: r.get(5)?,
        model: r.get(6)?,
        reasoning: r.get(7)?,
        workspace: r.get(8)?,
        branch: r.get(9)?,
        parent_id: r.get(10)?,
        created_at: r.get(11)?,
        updated_at: r.get(12)?,
    })
}
fn valid(i: &CreateTicket) -> Result<(), String> {
    let mut missing = Vec::new();
    if i.title.trim().is_empty() {
        missing.push("título");
    }
    if i.repository_id.as_deref().unwrap_or("").trim().is_empty() {
        missing.push("repositorio");
    }
    if i.agent.trim().is_empty() {
        missing.push("agente");
    }
    if i.model.trim().is_empty() {
        missing.push("modelo");
    }
    if i.reasoning.trim().is_empty() {
        missing.push("nivel de razonamiento");
    }
    if !missing.is_empty() {
        return Err(format!("Completá: {}.", missing.join(", ")));
    }
    if !matches!(i.kind.as_str(), "task" | "planning") {
        return Err("Tipo de ticket inválido.".into());
    }
    if !matches!(i.agent.as_str(), "codex" | "claude") {
        return Err("Agente no compatible.".into());
    }
    Ok(())
}

fn valid_transition(from: &str, to: &str) -> bool {
    matches!(
        (from, to),
        ("backlog", "todo") | ("review", "backlog" | "done")
    )
}

fn valid_update(i: &UpdateTicket) -> Result<(), String> {
    valid(&CreateTicket {
        title: i.title.clone(),
        description: i.description.clone(),
        kind: i.kind.clone(),
        agent: i.agent.clone(),
        model: i.model.clone(),
        reasoning: i.reasoning.clone(),
        workspace: i.workspace.clone(),
        repository_id: i.repository_id.clone(),
        parent_id: None,
    })
}

fn repository_path(db: &Connection, repository_id: &Option<String>) -> Result<String, String> {
    let id = repository_id
        .as_deref()
        .ok_or_else(|| "Seleccioná un repositorio.".to_string())?;
    db.query_row(
        "SELECT path FROM repositories WHERE id=?1",
        params![id],
        |r| r.get(0),
    )
    .map_err(|_| "El repositorio seleccionado ya no existe.".to_string())
}

fn valid_repository(input: &CreateRepository) -> Result<(), String> {
    if input.name.trim().is_empty() || input.path.trim().is_empty() {
        Err("Completá: nombre y ruta.".into())
    } else {
        Ok(())
    }
}

#[tauri::command]
fn list_repositories(state: tauri::State<'_, AppState>) -> Result<Vec<Repository>, String> {
    let db = state.0.lock().map_err(|_| "Base de datos no disponible")?;
    let mut statement = db.prepare("SELECT id,name,description,path,created_at FROM repositories ORDER BY name COLLATE NOCASE").map_err(|e| e.to_string())?;
    let repositories = statement
        .query_map([], |r| {
            Ok(Repository {
                id: r.get(0)?,
                name: r.get(1)?,
                description: r.get(2)?,
                path: r.get(3)?,
                created_at: r.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(repositories)
}

#[tauri::command]
fn create_repository(
    input: CreateRepository,
    state: tauri::State<'_, AppState>,
) -> Result<Repository, String> {
    valid_repository(&input)?;
    let repository = Repository {
        id: Uuid::new_v4().to_string(),
        name: input.name.trim().into(),
        description: input.description.trim().into(),
        path: input.path.trim().into(),
        created_at: Utc::now().to_rfc3339(),
    };
    let db = state.0.lock().map_err(|_| "Base de datos no disponible")?;
    db.execute(
        "INSERT INTO repositories VALUES(?1,?2,?3,?4,?5)",
        params![
            repository.id,
            repository.name,
            repository.description,
            repository.path,
            repository.created_at
        ],
    )
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            "La ruta ya está registrada.".into()
        } else {
            e.to_string()
        }
    })?;
    Ok(repository)
}

#[tauri::command]
fn update_repository(
    id: String,
    input: CreateRepository,
    state: tauri::State<'_, AppState>,
) -> Result<Repository, String> {
    valid_repository(&input)?;
    let db = state.0.lock().map_err(|_| "Base de datos no disponible")?;
    let created_at: String = db
        .query_row(
            "SELECT created_at FROM repositories WHERE id=?1",
            params![id],
            |r| r.get(0),
        )
        .map_err(|_| "Repositorio no encontrado.".to_string())?;
    let repository = Repository {
        id,
        name: input.name.trim().into(),
        description: input.description.trim().into(),
        path: input.path.trim().into(),
        created_at,
    };
    db.execute(
        "UPDATE repositories SET name=?1,description=?2,path=?3 WHERE id=?4",
        params![
            repository.name,
            repository.description,
            repository.path,
            repository.id
        ],
    )
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            "La ruta ya está registrada.".into()
        } else {
            e.to_string()
        }
    })?;
    db.execute("UPDATE tickets SET workspace=?1,updated_at=?2 WHERE id IN (SELECT ticket_id FROM ticket_repositories WHERE repository_id=?3)", params![repository.path, Utc::now().to_rfc3339(), repository.id]).map_err(|e| e.to_string())?;
    Ok(repository)
}

fn delete_repository_record(db: &Connection, id: &str) -> Result<(), String> {
    let in_use: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM ticket_repositories WHERE repository_id=?1",
            params![id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if in_use > 0 {
        return Err("No podés borrar un repositorio con tickets asociados.".into());
    }
    let deleted = db
        .execute("DELETE FROM repositories WHERE id=?1", params![id])
        .map_err(|e| e.to_string())?;
    if deleted == 0 {
        return Err("Repositorio no encontrado.".into());
    }
    Ok(())
}

#[tauri::command]
fn delete_repository(id: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let db = state.0.lock().map_err(|_| "Base de datos no disponible")?;
    delete_repository_record(&db, &id)
}

#[tauri::command]
fn list_tickets(state: tauri::State<'_, AppState>) -> Result<Vec<Ticket>, String> {
    let db = state.0.lock().map_err(|_| "Base de datos no disponible")?;
    let mut s=db.prepare("SELECT id,title,description,kind,status,agent,model,reasoning,workspace,branch,parent_id,created_at,updated_at FROM tickets WHERE parent_id IS NULL ORDER BY created_at DESC").map_err(|e|e.to_string())?;
    let result = s
        .query_map([], row)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string());
    result
}

#[tauri::command]
fn list_archived_ticket_ids(state: tauri::State<'_, AppState>) -> Result<Vec<String>, String> {
    let db = state.0.lock().map_err(|_| "Base de datos no disponible")?;
    let mut statement = db
        .prepare("SELECT ticket_id FROM ticket_archives ORDER BY archived_at DESC")
        .map_err(|e| e.to_string())?;
    let archived_ticket_ids = statement
        .query_map([], |row| row.get(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(archived_ticket_ids)
}

fn set_ticket_archive(db: &Connection, id: &str, archived: bool) -> Result<(), String> {
    let status: String = db
        .query_row(
            "SELECT status FROM tickets WHERE id=?1",
            params![id],
            |row| row.get(0),
        )
        .map_err(|_| "Ticket no encontrado.".to_string())?;
    if status != "done" {
        return Err("Sólo se pueden archivar tickets en Done.".into());
    }
    if archived {
        db.execute(
            "INSERT OR REPLACE INTO ticket_archives (ticket_id,archived_at) VALUES(?1,?2)",
            params![id, Utc::now().to_rfc3339()],
        )
    } else {
        db.execute(
            "DELETE FROM ticket_archives WHERE ticket_id=?1",
            params![id],
        )
    }
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn archive_ticket(id: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let db = state.0.lock().map_err(|_| "Base de datos no disponible")?;
    set_ticket_archive(&db, &id, true)
}

#[tauri::command]
fn unarchive_ticket(id: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let db = state.0.lock().map_err(|_| "Base de datos no disponible")?;
    set_ticket_archive(&db, &id, false)
}

#[tauri::command]
fn list_subtickets(
    parent_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<Ticket>, String> {
    let db = state.0.lock().map_err(|_| "Base de datos no disponible")?;
    let mut statement = db.prepare("SELECT id,title,description,kind,status,agent,model,reasoning,workspace,branch,parent_id,created_at,updated_at FROM tickets WHERE parent_id=?1 ORDER BY created_at ASC").map_err(|e| e.to_string())?;
    let tickets = statement
        .query_map(params![parent_id], row)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(tickets)
}
#[tauri::command]
fn create_ticket(input: CreateTicket, state: tauri::State<'_, AppState>) -> Result<Ticket, String> {
    let mut input = input;
    let db = state.0.lock().map_err(|_| "Base de datos no disponible")?;
    input.workspace = repository_path(&db, &input.repository_id)?;
    valid(&input)?;
    let now = Utc::now().to_rfc3339();
    let t = Ticket {
        id: Uuid::new_v4().to_string(),
        title: input.title.trim().into(),
        description: input.description.trim().into(),
        kind: input.kind,
        status: "backlog".into(),
        agent: input.agent,
        model: input.model,
        reasoning: input.reasoning,
        workspace: input.workspace.trim().into(),
        branch: None,
        parent_id: input.parent_id,
        created_at: now.clone(),
        updated_at: now,
    };
    db.execute(
        "INSERT INTO tickets VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
        params![
            t.id,
            t.title,
            t.description,
            t.kind,
            t.status,
            t.agent,
            t.model,
            t.reasoning,
            t.workspace,
            t.branch,
            t.parent_id,
            t.created_at,
            t.updated_at
        ],
    )
    .map_err(|e| e.to_string())?;
    db.execute(
        "INSERT INTO ticket_repositories VALUES(?1,?2)",
        params![t.id, input.repository_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(t)
}

#[tauri::command]
fn update_ticket(
    id: String,
    input: UpdateTicket,
    state: tauri::State<'_, AppState>,
) -> Result<Ticket, String> {
    let mut input = input;
    let db = state.0.lock().map_err(|_| "Base de datos no disponible")?;
    input.workspace = repository_path(&db, &input.repository_id)?;
    valid_update(&input)?;
    let ticket = db.query_row("SELECT id,title,description,kind,status,agent,model,reasoning,workspace,branch,parent_id,created_at,updated_at FROM tickets WHERE id=?1", params![id], row).map_err(|_| "Ticket no encontrado.".to_string())?;
    if ticket.status == "running" {
        return Err(
            "No se puede editar la configuración mientras el agente está en ejecución.".into(),
        );
    }
    let updated_at = Utc::now().to_rfc3339();
    db.execute("UPDATE tickets SET title=?1,description=?2,kind=?3,agent=?4,model=?5,reasoning=?6,workspace=?7,updated_at=?8 WHERE id=?9", params![input.title.trim(), input.description.trim(), input.kind.clone(), input.agent.clone(), input.model.trim(), input.reasoning.clone(), input.workspace.trim(), updated_at, ticket.id]).map_err(|e| e.to_string())?;
    db.execute(
        "INSERT OR REPLACE INTO ticket_repositories VALUES(?1,?2)",
        params![ticket.id, input.repository_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(Ticket {
        title: input.title.trim().into(),
        description: input.description.trim().into(),
        kind: input.kind,
        agent: input.agent,
        model: input.model.trim().into(),
        reasoning: input.reasoning,
        workspace: input.workspace.trim().into(),
        updated_at,
        ..ticket
    })
}

#[tauri::command]
fn delete_ticket(id: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let mut db = state.0.lock().map_err(|_| "Base de datos no disponible")?;
    let status: String = db
        .query_row(
            "SELECT status FROM tickets WHERE id=?1",
            params![id],
            |row| row.get(0),
        )
        .map_err(|_| "Ticket no encontrado.".to_string())?;
    if !matches!(status.as_str(), "backlog" | "todo") {
        return Err("Sólo se pueden borrar tickets en Backlog o To do.".into());
    }
    delete_ticket_records(&mut db, &id)?;
    Ok(())
}

fn delete_ticket_records(db: &mut Connection, id: &str) -> Result<(), String> {
    let transaction = db.transaction().map_err(|error| error.to_string())?;
    transaction
        .execute(
            "DELETE FROM execution_queue WHERE ticket_id=?1",
            params![id],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "DELETE FROM ticket_archives WHERE ticket_id=?1",
            params![id],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "DELETE FROM ticket_repositories WHERE ticket_id=?1",
            params![id],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute("DELETE FROM tickets WHERE id=?1", params![id])
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())
}

#[tauri::command]
fn list_queue_states(state: tauri::State<'_, AppState>) -> Result<Vec<QueueState>, String> {
    let db = state.0.lock().map_err(|_| "Base de datos no disponible")?;
    let mut statement = db.prepare("SELECT q.ticket_id,q.retry_required,q.last_error FROM execution_queue q JOIN tickets t ON t.id=q.ticket_id WHERE t.status='todo' ORDER BY q.queued_at ASC").map_err(|e| e.to_string())?;
    let states = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, bool>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .enumerate()
        .map(|(index, item)| {
            item.map(|(ticket_id, retry_required, last_error)| QueueState {
                ticket_id,
                position: (index + 1) as u32,
                retry_required,
                last_error,
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(states)
}

fn reserve_next_queued(db: &mut Connection) -> Result<Option<String>, String> {
    let preferences = preferences(db)?;
    let active: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM executions WHERE status IN ('running','stopping')",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if active >= preferences.max_concurrent_tasks as i64 {
        return Ok(None);
    }
    let ticket = db.query_row("SELECT t.id,t.title,t.description,t.kind,t.status,t.agent,t.model,t.reasoning,t.workspace,t.branch,t.parent_id,t.created_at,t.updated_at FROM execution_queue q JOIN tickets t ON t.id=q.ticket_id WHERE t.status='todo' AND q.retry_required=0 ORDER BY q.queued_at ASC LIMIT 1", [], row).optional().map_err(|e| e.to_string())?;
    let Some(ticket) = ticket else {
        return Ok(None);
    };
    if let Err(error) = validate_execution_policy(db, &ticket) {
        db.execute(
            "UPDATE execution_queue SET retry_required=1,last_error=?1 WHERE ticket_id=?2",
            params![error, ticket.id],
        )
        .map_err(|e| e.to_string())?;
        return Ok(None);
    }
    let transaction = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let reserved = transaction
        .execute(
            "UPDATE tickets SET status='running',updated_at=?1 WHERE id=?2 AND status='todo'",
            params![Utc::now().to_rfc3339(), ticket.id],
        )
        .map_err(|e| e.to_string())?
        == 1;
    transaction.commit().map_err(|e| e.to_string())?;
    Ok(reserved.then_some(ticket.id))
}

fn schedule_queued(app: &tauri::AppHandle) {
    loop {
        let next_id = {
            let state = app.state::<AppState>();
            let mut db = match state.0.lock() {
                Ok(db) => db,
                Err(_) => return,
            };
            match reserve_next_queued(&mut db) {
                Ok(next) => next,
                Err(_) => return,
            }
        };
        let Some(id) = next_id else { return };
        if let Err(error) =
            start_agent_with_context(id.clone(), None, None, app.state::<AppState>(), app.clone())
        {
            if let Ok(db) = app.state::<AppState>().0.lock() {
                let _ = db.execute(
                    "UPDATE tickets SET status='todo',updated_at=?1 WHERE id=?2",
                    params![Utc::now().to_rfc3339(), id],
                );
                let _ = db.execute(
                    "UPDATE execution_queue SET retry_required=1,last_error=?1 WHERE ticket_id=?2",
                    params![error, id],
                );
            }
        }
    }
}

#[tauri::command]
fn move_ticket(
    id: String,
    status: String,
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let db = state.0.lock().map_err(|_| "Base de datos no disponible")?;
    let ticket=db.query_row("SELECT id,title,description,kind,status,agent,model,reasoning,workspace,branch,parent_id,created_at,updated_at FROM tickets WHERE id=?1",params![id],row).map_err(|_|"Ticket no encontrado.".to_string())?;
    if !valid_transition(&ticket.status, &status) {
        return Err("Transición no permitida: usá Backlog → To do o Review → Backlog/Done.".into());
    }
    if status == "todo" {
        if git(&ticket.workspace, &["rev-parse", "--is-inside-work-tree"])? != "true" {
            return Err("La carpeta elegida no es un repositorio Git válido.".into());
        }
        let branch = ticket
            .branch
            .clone()
            .unwrap_or_else(|| branch_name(&ticket));
        if git(&ticket.workspace, &["branch", "--list", &branch])?.is_empty() {
            git(&ticket.workspace, &["checkout", "-b", &branch])?;
        } else {
            git(&ticket.workspace, &["checkout", &branch])?;
        }
        db.execute(
            "UPDATE tickets SET status='todo',branch=?1,updated_at=?2 WHERE id=?3",
            params![branch, Utc::now().to_rfc3339(), ticket.id],
        )
        .map_err(|e| e.to_string())?;
        drop(db);
        let db = state.0.lock().map_err(|_| "Base de datos no disponible")?;
        db.execute("INSERT OR REPLACE INTO execution_queue (ticket_id,queued_at,retry_required,last_error) VALUES(?1,?2,0,NULL)", params![id, Utc::now().to_rfc3339()]).map_err(|e| e.to_string())?;
        drop(db);
        schedule_queued(&app);
        return Ok(());
    }
    let n = db
        .execute(
            "UPDATE tickets SET status=?1,updated_at=?2 WHERE id=?3",
            params![status, Utc::now().to_rfc3339(), ticket.id],
        )
        .map_err(|e| e.to_string())?;
    if n == 0 {
        Err("No se puede mover el ticket.".into())
    } else {
        Ok(())
    }
}

fn git(workspace: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(workspace)
        .args(args)
        .output()
        .map_err(|_| "No se pudo ejecutar Git. Verificá que esté instalado.".to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn inspect_git(workspace: &str) -> Result<GitSnapshot, String> {
    if git(workspace, &["rev-parse", "--is-inside-work-tree"])? != "true" {
        return Err("La carpeta elegida no es un repositorio Git válido.".into());
    }
    Ok(GitSnapshot {
        base_commit: git(workspace, &["rev-parse", "HEAD"]).ok(),
        working_tree_status: git(workspace, &["status", "--porcelain"])?,
    })
}

#[tauri::command]
fn preflight_ticket(id: String, state: tauri::State<'_, AppState>) -> Result<Preflight, String> {
    let db = state.0.lock().map_err(|_| "Base de datos no disponible")?;
    let ticket: Ticket = db.query_row("SELECT id,title,description,kind,status,agent,model,reasoning,workspace,branch,parent_id,created_at,updated_at FROM tickets WHERE id=?1", params![id], row).map_err(|_| "Ticket no encontrado.".to_string())?;
    let snapshot = inspect_git(&ticket.workspace)?;
    let active: i64 = db.query_row("SELECT COUNT(*) FROM executions e JOIN tickets t ON t.id=e.ticket_id WHERE t.workspace=?1 AND e.status IN ('running','stopping')", params![ticket.workspace], |r| r.get(0)).map_err(|e| e.to_string())?;
    Ok(Preflight {
        dirty_worktree: !snapshot.working_tree_status.trim().is_empty(),
        active_execution: active > 0,
    })
}

fn terminal_state(success: bool) -> (&'static str, &'static str) {
    if success {
        ("done", "review")
    } else {
        ("failed", "todo")
    }
}

fn execution_summary(log: &str) -> String {
    let mut summary = None;
    for line in log.lines() {
        let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let Some(item) = event.get("item") else {
            continue;
        };
        if item.get("type").and_then(serde_json::Value::as_str) == Some("agent_message") {
            summary = item
                .get("text")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned);
        }
    }
    summary.unwrap_or_else(|| log.trim().to_string())
}

fn detect_changes(workspace: &str, base_commit: Option<&str>) -> String {
    let committed = base_commit
        .and_then(|base| git(workspace, &["diff", "--stat", &format!("{base}..HEAD")]).ok())
        .unwrap_or_default();
    let working = git(workspace, &["status", "--short"])
        .unwrap_or_else(|error| format!("No se pudo inspeccionar cambios: {error}"));
    [committed, working]
        .into_iter()
        .filter(|section| !section.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn branch_name(ticket: &Ticket) -> String {
    let slug: String = ticket
        .title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    format!(
        "palantir/{}-{}",
        &ticket.id[..8],
        slug.split('-')
            .filter(|p| !p.is_empty())
            .take(4)
            .collect::<Vec<_>>()
            .join("-")
    )
}

fn parse_plan(log: &str, ticket_id: &str) -> Vec<PlanItem> {
    let parsed = log.lines().find_map(|line| {
        let text = serde_json::from_str::<serde_json::Value>(line)
            .ok()
            .and_then(|event| event.get("item")?.get("text")?.as_str().map(str::to_owned))
            .unwrap_or_else(|| line.to_owned());
        text.trim()
            .strip_prefix("PLAN_JSON:")
            .and_then(|json| serde_json::from_str::<Vec<PlanItem>>(json.trim()).ok())
    });
    parsed.filter(|items| !items.is_empty()).unwrap_or_else(|| {
        vec![PlanItem {
            title: format!("Implementar {}", ticket_id),
            description:
                "Subtarea propuesta por el ticket de planificación; revisala antes de ejecutarla."
                    .into(),
        }]
    })
}

#[tauri::command]
fn send_follow_up(
    id: String,
    message: String,
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    if message.trim().is_empty() {
        return Err("Escribí una instrucción de seguimiento.".into());
    }
    let db = state.0.lock().map_err(|_| "Base de datos no disponible")?;
    let ticket: Ticket = db.query_row("SELECT id,title,description,kind,status,agent,model,reasoning,workspace,branch,parent_id,created_at,updated_at FROM tickets WHERE id=?1", params![id], row).map_err(|_| "Ticket no encontrado.".to_string())?;
    if ticket.status != "review" {
        return Err("Los seguimientos sólo se envían desde Review.".into());
    }
    let previous: String = db
        .query_row(
            "SELECT id FROM executions WHERE ticket_id=?1 ORDER BY started_at DESC LIMIT 1",
            params![ticket.id],
            |r| r.get(0),
        )
        .map_err(|_| "El ticket todavía no tiene una ejecución para continuar.".to_string())?;
    db.execute(
        "INSERT INTO review_messages VALUES(?1,?2,?3,'user',?4,?5)",
        params![
            Uuid::new_v4().to_string(),
            ticket.id,
            previous,
            message.trim(),
            Utc::now().to_rfc3339()
        ],
    )
    .map_err(|e| e.to_string())?;
    drop(db);
    start_agent_with_context(id, Some(previous), Some(message.trim().into()), state, app)
}

#[tauri::command]
fn get_plan(id: String, state: tauri::State<'_, AppState>) -> Result<Option<Plan>, String> {
    let db = state.0.lock().map_err(|_| "Base de datos no disponible")?;
    db.query_row(
        "SELECT ticket_id,items,approved_at FROM plans WHERE ticket_id=?1",
        params![id],
        |r| {
            let items: String = r.get(1)?;
            Ok(Plan {
                ticket_id: r.get(0)?,
                items: serde_json::from_str(&items).unwrap_or_default(),
                approved_at: r.get(2)?,
            })
        },
    )
    .optional()
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn approve_plan(id: String, state: tauri::State<'_, AppState>) -> Result<Vec<Ticket>, String> {
    let db = state.0.lock().map_err(|_| "Base de datos no disponible")?;
    let parent: Ticket = db.query_row("SELECT id,title,description,kind,status,agent,model,reasoning,workspace,branch,parent_id,created_at,updated_at FROM tickets WHERE id=?1", params![id], row).map_err(|_| "Ticket no encontrado.".to_string())?;
    if parent.kind != "planning" || parent.status != "review" {
        return Err("Sólo se puede aprobar un plan pendiente en Review.".into());
    }
    let (items_json, approved): (String, Option<String>) = db
        .query_row(
            "SELECT items,approved_at FROM plans WHERE ticket_id=?1",
            params![parent.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| "Todavía no hay un plan estructurado para aprobar.".to_string())?;
    if approved.is_some() {
        return Err("Este plan ya fue aprobado.".into());
    }
    let items: Vec<PlanItem> = serde_json::from_str(&items_json)
        .map_err(|_| "El plan almacenado no es válido.".to_string())?;
    let now = Utc::now().to_rfc3339();
    let mut children = Vec::new();
    for item in items {
        let child = Ticket {
            id: Uuid::new_v4().to_string(),
            title: item.title,
            description: item.description,
            kind: "task".into(),
            status: "backlog".into(),
            agent: parent.agent.clone(),
            model: parent.model.clone(),
            reasoning: parent.reasoning.clone(),
            workspace: parent.workspace.clone(),
            branch: None,
            parent_id: Some(parent.id.clone()),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        db.execute(
            "INSERT INTO tickets VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
            params![
                child.id,
                child.title,
                child.description,
                child.kind,
                child.status,
                child.agent,
                child.model,
                child.reasoning,
                child.workspace,
                child.branch,
                child.parent_id,
                child.created_at,
                child.updated_at
            ],
        )
        .map_err(|e| e.to_string())?;
        children.push(child);
    }
    db.execute(
        "UPDATE plans SET approved_at=?1 WHERE ticket_id=?2",
        params![now, parent.id],
    )
    .map_err(|e| e.to_string())?;
    Ok(children)
}
#[tauri::command]
fn prepare_ticket_branch(id: String, state: tauri::State<'_, AppState>) -> Result<String, String> {
    let db = state.0.lock().map_err(|_| "Base de datos no disponible")?;
    let ticket=db.query_row("SELECT id,title,description,kind,status,agent,model,reasoning,workspace,branch,parent_id,created_at,updated_at FROM tickets WHERE id=?1",params![id],row).map_err(|_|"Ticket no encontrado.".to_string())?;
    if git(&ticket.workspace, &["rev-parse", "--is-inside-work-tree"])? != "true" {
        return Err("La carpeta elegida no es un repositorio Git válido.".into());
    }
    let branch = ticket
        .branch
        .clone()
        .unwrap_or_else(|| branch_name(&ticket));
    let exists = git(&ticket.workspace, &["branch", "--list", &branch])?;
    if exists.is_empty() {
        git(&ticket.workspace, &["checkout", "-b", &branch])?;
    } else {
        git(&ticket.workspace, &["checkout", &branch])?;
    }
    db.execute(
        "UPDATE tickets SET branch=?1,status='todo',updated_at=?2 WHERE id=?3",
        params![branch, Utc::now().to_rfc3339(), ticket.id],
    )
    .map_err(|e| e.to_string())?;
    Ok(branch)
}
#[tauri::command]
fn start_agent(
    id: String,
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let db = state.0.lock().map_err(|_| "Base de datos no disponible")?;
    let status: String = db
        .query_row(
            "SELECT status FROM tickets WHERE id=?1",
            params![id],
            |row| row.get(0),
        )
        .map_err(|_| "Ticket no encontrado.".to_string())?;
    if status != "todo" {
        return Err("Sólo se puede encolar un ticket que esté en To do.".into());
    }
    db.execute("INSERT OR REPLACE INTO execution_queue (ticket_id,queued_at,retry_required,last_error) VALUES(?1,COALESCE((SELECT queued_at FROM execution_queue WHERE ticket_id=?1),?2),0,NULL)", params![id, Utc::now().to_rfc3339()]).map_err(|e| e.to_string())?;
    drop(db);
    schedule_queued(&app);
    Ok(())
}

fn start_agent_with_context(
    id: String,
    parent_execution_id: Option<String>,
    follow_up_message: Option<String>,
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let db = state.0.lock().map_err(|_| "Base de datos no disponible")?;
    let ticket=db.query_row("SELECT id,title,description,kind,status,agent,model,reasoning,workspace,branch,parent_id,created_at,updated_at FROM tickets WHERE id=?1",params![id],row).map_err(|_|"Ticket no encontrado.".to_string())?;
    let preferences = validate_execution_policy(&db, &ticket)?;
    if ticket.branch.is_none() {
        return Err("Mové el ticket a To do para preparar su rama antes de ejecutarlo.".into());
    }
    let active_in_workspace: i64 = db.query_row(
        "SELECT COUNT(*) FROM executions e JOIN tickets t ON t.id=e.ticket_id WHERE t.workspace=?1 AND e.status IN ('running','stopping')",
        params![ticket.workspace], |r| r.get(0)
    ).map_err(|e| e.to_string())?;
    if active_in_workspace > 0 {
        return Err("Ya hay una ejecución activa en esta carpeta. Esperá a que termine para evitar conflictos de worktree o rama.".into());
    }
    let git_snapshot = inspect_git(&ticket.workspace)?;
    let dirty_warning = if git_snapshot.working_tree_status.trim().is_empty() {
        String::new()
    } else {
        "\n\nEl repositorio ya tenía cambios sin confirmar. No los descartes ni los atribuyas al ticket.".into()
    };
    let plan_instruction = if ticket.kind == "planning" {
        "\n\nEste es un ticket de planificación: no implementes cambios. Terminá con una línea PLAN_JSON: seguida de un arreglo JSON de objetos {\"title\": string, \"description\": string} para las subtareas propuestas."
    } else {
        ""
    };
    let follow_context = follow_up_message.as_ref().map(|message| format!("\n\nSeguimiento solicitado desde Review:\n{message}\n\nUsá el trabajo y el informe anterior como contexto.")).unwrap_or_default();
    let prompt=format!("Ticket {}: {}\n\n{}{}{}{}\n\nTrabajá solo en la rama actual. Al finalizar, respondé con estos encabezados: Resumen, Validaciones, Decisiones, Riesgos y pendientes.",ticket.id,ticket.title,ticket.description,dirty_warning,follow_context,plan_instruction);
    let adapter = adapter_for(&ticket.agent)?;
    if !adapter.is_available() {
        return Err(format!(
            "La CLI local de {} no está disponible. Verificá su instalación y autenticación.",
            ticket.agent
        ));
    }
    let previous_session = parent_execution_id.as_ref().and_then(|parent| {
        db.query_row(
            "SELECT session_id FROM executions WHERE id=?1",
            params![parent],
            |r| r.get::<_, Option<String>>(0),
        )
        .ok()
        .flatten()
    });
    let invocation = previous_session
        .as_deref()
        .and_then(|session| adapter.resume(&ticket, session, &prompt))
        .unwrap_or_else(|| adapter.start(&ticket, &prompt));
    let mut command = Command::new(invocation.program);
    command
        .current_dir(&ticket.workspace)
        .args(invocation.args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child = command
        .spawn()
        .map_err(|e| format!("No se pudo iniciar {}: {}", ticket.agent, e))?;
    let execution = Uuid::new_v4().to_string();
    let config=serde_json::json!({"agent":ticket.agent,"model":ticket.model,"reasoning":ticket.reasoning,"workspace":ticket.workspace,"branch":ticket.branch,"pid":child.id(),"git":&git_snapshot,"policy":preferences}).to_string();
    let report_version: i64 = db
        .query_row(
            "SELECT COALESCE(MAX(report_version),0)+1 FROM executions WHERE ticket_id=?1",
            params![ticket.id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    db.execute(
        "INSERT INTO executions (id,ticket_id,session_id,status,effective_config,base_commit,started_at,parent_execution_id,follow_up_message,report_version) VALUES(?1,?2,NULL,'running',?3,?4,?5,?6,?7,?8)",
        params![
            execution,
            ticket.id,
            config,
            git_snapshot.base_commit,
            Utc::now().to_rfc3339(),
            parent_execution_id,
            follow_up_message,
            report_version
        ],
    )
    .map_err(|e| e.to_string())?;
    db.execute(
        "UPDATE tickets SET status='running',updated_at=?1 WHERE id=?2",
        params![Utc::now().to_rfc3339(), ticket.id],
    )
    .map_err(|e| e.to_string())?;
    let path = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("palantir.sqlite3");
    let ticket_id = ticket.id.clone();
    let agent = ticket.agent.clone();
    let ticket_kind = ticket.kind.clone();
    let workspace = ticket.workspace.clone();
    let base_commit = git_snapshot.base_commit.clone();
    let scheduler_app = app.clone();
    std::thread::spawn(move || {
        let result = child.wait_with_output();
        let db = match Connection::open(path) {
            Ok(db) => db,
            Err(_) => return,
        };
        let (status, summary, log, ticket_status) = match result {
            Ok(output) => {
                let log = format!(
                    "{}\n{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
                let (status, ticket_status) = terminal_state(output.status.success());
                (status, execution_summary(&log), log, ticket_status)
            }
            Err(error) => {
                let message = error.to_string();
                ("failed", message.clone(), message, "todo")
            }
        };
        let session_id = adapter_for(&agent)
            .ok()
            .and_then(|adapter| adapter.session_id_from_output(&log));
        let changes_detected = detect_changes(&workspace, base_commit.as_deref());
        let _ = db.execute(
            "UPDATE executions SET session_id=?1,status=?2,finished_at=?3,summary=?4 WHERE id=?5",
            params![
                session_id,
                status,
                Utc::now().to_rfc3339(),
                summary,
                execution
            ],
        );
        if ticket_kind == "planning" && status == "done" {
            let items = parse_plan(&log, &ticket_id);
            let _ = db.execute(
                "INSERT OR REPLACE INTO plans (ticket_id,items,approved_at,created_at) VALUES(?1,?2,NULL,?3)",
                params![ticket_id, serde_json::to_string(&items).unwrap_or_else(|_| "[]".into()), Utc::now().to_rfc3339()],
            );
        }
        let _ = db.execute(
            "INSERT OR REPLACE INTO execution_reports VALUES(?1,?2,?3,?4)",
            params![
                Uuid::new_v4().to_string(),
                execution,
                changes_detected,
                Utc::now().to_rfc3339()
            ],
        );
        let _ = db.execute(
            "INSERT INTO execution_logs VALUES(?1,?2,?3,?4)",
            params![
                Uuid::new_v4().to_string(),
                execution,
                log,
                Utc::now().to_rfc3339()
            ],
        );
        let _ = db.execute(
            "UPDATE tickets SET status=?1,updated_at=?2 WHERE id=?3",
            params![ticket_status, Utc::now().to_rfc3339(), ticket_id],
        );
        if status == "done" {
            let _ = db.execute(
                "DELETE FROM execution_queue WHERE ticket_id=?1",
                params![ticket_id],
            );
        } else {
            let _ = db.execute("INSERT OR REPLACE INTO execution_queue (ticket_id,queued_at,retry_required,last_error) VALUES(?1,?2,1,?3)", params![ticket_id, Utc::now().to_rfc3339(), summary]);
        }
        schedule_queued(&scheduler_app);
    });
    Ok(())
}

#[tauri::command]
fn retry_ticket(
    id: String,
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let db = state.0.lock().map_err(|_| "Base de datos no disponible")?;
    let status: String = db
        .query_row(
            "SELECT status FROM tickets WHERE id=?1",
            params![id],
            |row| row.get(0),
        )
        .map_err(|_| "Ticket no encontrado.".to_string())?;
    if status != "todo" {
        return Err(
            "Sólo se puede reintentar un ticket que volvió a To do después de un fallo.".into(),
        );
    }
    db.execute(
        "UPDATE execution_queue SET retry_required=0,last_error=NULL WHERE ticket_id=?1",
        params![id],
    )
    .map_err(|e| e.to_string())?;
    drop(db);
    schedule_queued(&app);
    Ok(())
}

#[tauri::command]
fn stop_execution(id: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let db = state.0.lock().map_err(|_| "Base de datos no disponible")?;
    let config: String = db
        .query_row(
            "SELECT effective_config FROM executions WHERE id=?1 AND status='running'",
            params![id],
            |row| row.get(0),
        )
        .map_err(|_| "No hay una ejecución activa con ese identificador.".to_string())?;
    let pid = serde_json::from_str::<serde_json::Value>(&config)
        .ok()
        .and_then(|value| value.get("pid")?.as_u64())
        .ok_or_else(|| "La ejecución no tiene un PID recuperable.".to_string())?;
    let result = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status()
        .map_err(|error| format!("No se pudo solicitar la detención: {error}"))?;
    if !result.success() {
        return Err("El proceso ya no está disponible para detenerse.".into());
    }
    db.execute(
        "UPDATE executions SET status='stopping' WHERE id=?1",
        params![id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Session {
    id: String,
    ticket_id: String,
    ticket_title: String,
    agent: String,
    status: String,
    started_at: String,
    finished_at: Option<String>,
    summary: Option<String>,
    effective_config: String,
    base_commit: Option<String>,
    changes_detected: Option<String>,
}
#[tauri::command]
fn list_sessions(state: tauri::State<'_, AppState>) -> Result<Vec<Session>, String> {
    let db = state.0.lock().map_err(|_| "Base de datos no disponible")?;
    let mut s=db.prepare("SELECT e.id,e.ticket_id,t.title,t.agent,e.status,e.started_at,e.finished_at,e.summary,e.effective_config,e.base_commit,r.changes_detected FROM executions e JOIN tickets t ON t.id=e.ticket_id LEFT JOIN execution_reports r ON r.execution_id=e.id ORDER BY e.started_at DESC").map_err(|e|e.to_string())?;
    let rows = s
        .query_map([], |r| {
            Ok(Session {
                id: r.get(0)?,
                ticket_id: r.get(1)?,
                ticket_title: r.get(2)?,
                agent: r.get(3)?,
                status: r.get(4)?,
                started_at: r.get(5)?,
                finished_at: r.get(6)?,
                summary: r.get(7)?,
                effective_config: r.get(8)?,
                base_commit: r.get(9)?,
                changes_detected: r.get(10)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_log_retention(max_entries: u32, state: tauri::State<'_, AppState>) -> Result<(), String> {
    if !(10..=100_000).contains(&max_entries) {
        return Err("La retención debe estar entre 10 y 100000 registros.".into());
    }
    let db = state.0.lock().map_err(|_| "Base de datos no disponible")?;
    db.execute("DELETE FROM execution_logs WHERE id NOT IN (SELECT id FROM execution_logs ORDER BY created_at DESC LIMIT ?1)", params![max_entries]).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir: PathBuf = app.path().app_data_dir().map_err(|e| e.to_string())?;
            fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let db = Connection::open(dir.join("palantir.sqlite3")).map_err(|e| e.to_string())?;
            migrate(&db).map_err(|e| e.to_string())?;
            db.execute(
                "UPDATE executions SET status='interrupted', finished_at=?1 WHERE status='running'",
                params![Utc::now().to_rfc3339()],
            )
            .map_err(|e| e.to_string())?;
            // Keep a bounded local archive by default; users can tighten or expand it via the command.
            db.execute("DELETE FROM execution_logs WHERE id NOT IN (SELECT id FROM execution_logs ORDER BY created_at DESC LIMIT 2000)", [])
                .map_err(|e| e.to_string())?;
            db.execute(
                "UPDATE tickets SET status='todo', updated_at=?1 WHERE status='running'",
                params![Utc::now().to_rfc3339()],
            )
            .map_err(|e| e.to_string())?;
            db.execute("INSERT OR REPLACE INTO execution_queue (ticket_id,queued_at,retry_required,last_error) SELECT id,updated_at,1,'La aplicación se reinició durante la ejecución; reintentá manualmente.' FROM tickets WHERE status='todo' AND id IN (SELECT ticket_id FROM executions WHERE status='interrupted')", []).map_err(|e| e.to_string())?;
            app.manage(AppState(Mutex::new(db)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_tickets,
            get_preferences,
            update_preferences,
            list_queue_states,
            list_archived_ticket_ids,
            list_repositories,
            create_repository,
            update_repository,
            delete_repository,
            list_subtickets,
            create_ticket,
            update_ticket,
            delete_ticket,
            archive_ticket,
            unarchive_ticket,
            move_ticket,
            prepare_ticket_branch,
            preflight_ticket,
            start_agent,
            retry_ticket,
            stop_execution,
            list_sessions,
            send_follow_up,
            get_plan,
            approve_plan,
            set_log_retention,
            test_remote_connection,
            remote_request
        ])
        .run(tauri::generate_context!())
        .expect("error while running Palantir")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ticket(agent: &str) -> Ticket {
        Ticket {
            id: "12345678-test".into(),
            title: "Test".into(),
            description: String::new(),
            kind: "task".into(),
            status: "review".into(),
            agent: agent.into(),
            model: "model".into(),
            reasoning: "high".into(),
            workspace: "/tmp".into(),
            branch: Some("palantir/test".into()),
            parent_id: None,
            created_at: "now".into(),
            updated_at: "now".into(),
        }
    }
    #[test]
    fn migration_keeps_snapshots() {
        let db = Connection::open_in_memory().unwrap();
        migrate(&db).unwrap();
        db.execute("INSERT INTO executions (id,ticket_id,session_id,status,effective_config,base_commit,started_at) VALUES ('e','t',NULL,'done','{\"model\":\"gpt\"}',NULL,'now')",[]).unwrap();
        let v: String = db
            .query_row("SELECT effective_config FROM executions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, "{\"model\":\"gpt\"}")
    }

    #[test]
    fn migration_creates_default_preferences_and_orders_existing_todo_tickets() {
        let db = Connection::open_in_memory().unwrap();
        migrate(&db).unwrap();
        let defaults = preferences(&db).unwrap();
        assert_eq!(defaults.max_concurrent_tasks, 1);
        assert_eq!(defaults.execution_profile, "local");
        assert!(validate_preferences(&Preferences {
            max_concurrent_tasks: 0,
            ..defaults.clone()
        })
        .is_err());
        for (id, updated_at) in [("later", "2026-01-02"), ("first", "2026-01-01")] {
            db.execute("INSERT INTO tickets VALUES(?1,?2,'','task','todo','codex','gpt-5.6-terra','high','/tmp',NULL,NULL,'now',?3)", params![id, id, updated_at]).unwrap();
        }
        db.execute("INSERT INTO execution_queue (ticket_id,queued_at) SELECT id,updated_at FROM tickets WHERE status='todo'", []).unwrap();
        let first: String = db
            .query_row(
                "SELECT ticket_id FROM execution_queue ORDER BY queued_at ASC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(first, "first");
    }

    #[test]
    fn policy_rejects_remote_and_disallowed_ticket_configuration() {
        let db = Connection::open_in_memory().unwrap();
        migrate(&db).unwrap();
        let ticket = Ticket {
            model: "not-allowed".into(),
            ..test_ticket("codex")
        };
        assert!(validate_execution_policy(&db, &ticket).is_err());
        let remote = Preferences {
            execution_profile: "remote".into(),
            ..default_preferences()
        };
        db.execute(
            "UPDATE application_preferences SET value=?1 WHERE id=1",
            params![serde_json::to_string(&remote).unwrap()],
        )
        .unwrap();
        assert!(validate_execution_policy(&db, &test_ticket("codex")).is_err());
    }

    #[test]
    fn scheduler_reserves_fifo_slots_and_skips_manual_retries() {
        let mut db = Connection::open_in_memory().unwrap();
        migrate(&db).unwrap();
        for (id, queued_at, retry_required) in [
            ("first", "2026-01-01", 0),
            ("second", "2026-01-02", 0),
            ("retry", "2026-01-03", 1),
        ] {
            db.execute("INSERT INTO tickets VALUES(?1,?1,'','task','todo','codex','gpt-5.6-terra','high','/tmp',NULL,NULL,?2,?2)", params![id, queued_at]).unwrap();
            db.execute("INSERT INTO execution_queue (ticket_id,queued_at,retry_required,last_error) VALUES(?1,?2,?3,CASE WHEN ?3=1 THEN 'falló' ELSE NULL END)", params![id, queued_at, retry_required]).unwrap();
        }
        assert_eq!(
            reserve_next_queued(&mut db).unwrap().as_deref(),
            Some("first")
        );
        db.execute("INSERT INTO executions (id,ticket_id,session_id,status,effective_config,started_at) VALUES('running','first',NULL,'running','{}','now')", []).unwrap();
        assert_eq!(
            reserve_next_queued(&mut db).unwrap(),
            None,
            "el límite uno conserva la espera"
        );
        db.execute("UPDATE executions SET status='done' WHERE id='running'", [])
            .unwrap();
        assert_eq!(
            reserve_next_queued(&mut db).unwrap().as_deref(),
            Some("second")
        );
        db.execute("UPDATE tickets SET status='todo' WHERE id='retry'", [])
            .unwrap();
        assert_eq!(
            reserve_next_queued(&mut db).unwrap(),
            None,
            "un fallo no se reintenta solo"
        );
        db.execute(
            "UPDATE execution_queue SET retry_required=0,last_error=NULL WHERE ticket_id='retry'",
            [],
        )
        .unwrap();
        assert_eq!(
            reserve_next_queued(&mut db).unwrap().as_deref(),
            Some("retry")
        );
    }

    #[test]
    fn validates_ticket_kind_and_execution_configuration() {
        let valid_ticket = CreateTicket {
            title: "Planificar migración".into(),
            description: String::new(),
            kind: "planning".into(),
            agent: "codex".into(),
            model: "gpt-5.6".into(),
            reasoning: "high".into(),
            workspace: "/tmp/repo".into(),
            repository_id: Some("repository".into()),
            parent_id: None,
        };
        assert!(valid(&valid_ticket).is_ok());

        let claude_ticket = CreateTicket {
            agent: "claude".into(),
            model: "claude-sonnet-5".into(),
            ..valid_ticket
        };
        assert!(valid(&claude_ticket).is_ok());

        let unsupported_agent = CreateTicket {
            agent: "gemini".into(),
            ..claude_ticket
        };
        assert!(valid(&unsupported_agent).is_err());

        let missing_agent = CreateTicket {
            agent: String::new(),
            ..unsupported_agent
        };
        assert_eq!(valid(&missing_agent), Err("Completá: agente.".into()));
    }

    #[test]
    fn policy_allows_claude_tickets_under_default_preferences() {
        let db = Connection::open_in_memory().unwrap();
        migrate(&db).unwrap();
        let ticket = Ticket {
            model: "claude-sonnet-5".into(),
            ..test_ticket("claude")
        };
        assert!(validate_execution_policy(&db, &ticket).is_ok());
        assert!(adapter_for("claude").is_ok());
    }

    #[test]
    fn only_exposes_manual_kanban_transitions() {
        assert!(valid_transition("backlog", "todo"));
        assert!(valid_transition("review", "backlog"));
        assert!(valid_transition("review", "done"));
        assert!(!valid_transition("todo", "running"));
        assert!(!valid_transition("running", "review"));
        assert!(!valid_transition("done", "backlog"));
    }

    #[test]
    fn adapters_capture_sessions_and_build_resume_commands() {
        let codex = CodexAdapter;
        assert_eq!(
            codex
                .session_id_from_output("{\"type\":\"thread.started\",\"thread_id\":\"thread-1\"}"),
            Some("thread-1".into())
        );
        assert!(codex
            .resume(&test_ticket("codex"), "thread-1", "seguí")
            .unwrap()
            .args
            .contains(&"thread-1".into()));

        let claude = ClaudeAdapter;
        assert_eq!(
            claude.session_id_from_output("{\"session_id\":\"session-1\"}"),
            Some("session-1".into())
        );
        assert!(claude
            .resume(&test_ticket("claude"), "session-1", "seguí")
            .unwrap()
            .args
            .contains(&"session-1".into()));
    }

    #[test]
    fn planning_output_produces_structured_subtasks() {
        let plan = parse_plan(
            "Resumen\nPLAN_JSON: [{\"title\":\"Diseñar\",\"description\":\"Definir contrato\"}]",
            "ticket-1",
        );
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].title, "Diseñar");
        assert_eq!(parse_plan("sin JSON", "ticket-1").len(), 1);
    }

    #[test]
    fn execution_history_links_follow_ups_and_versions() {
        let db = Connection::open_in_memory().unwrap();
        migrate(&db).unwrap();
        db.execute("INSERT INTO executions (id,ticket_id,status,effective_config,started_at,report_version) VALUES ('first','ticket','done','{}','now',1)", []).unwrap();
        db.execute("INSERT INTO executions (id,ticket_id,status,effective_config,started_at,parent_execution_id,follow_up_message,report_version) VALUES ('second','ticket','done','{}','later','first','corregí las pruebas',2)", []).unwrap();
        let linked: (String, String, i64) = db.query_row("SELECT parent_execution_id,follow_up_message,report_version FROM executions WHERE id='second'", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
        assert_eq!(linked, ("first".into(), "corregí las pruebas".into(), 2));
    }

    #[test]
    fn subkanban_children_are_isolated_and_progress_is_countable() {
        let db = Connection::open_in_memory().unwrap();
        migrate(&db).unwrap();
        db.execute("INSERT INTO tickets VALUES ('parent','Plan','','planning','review','codex','model','high','/tmp',NULL,NULL,'now','now')", []).unwrap();
        db.execute("INSERT INTO tickets VALUES ('child-1','Uno','','task','done','codex','model','high','/tmp',NULL,'parent','now','now')", []).unwrap();
        db.execute("INSERT INTO tickets VALUES ('child-2','Dos','','task','backlog','codex','model','high','/tmp',NULL,'parent','now','now')", []).unwrap();
        let main_board: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM tickets WHERE parent_id IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let progress: (i64, i64) = db
            .query_row(
                "SELECT COUNT(*), SUM(status='done') FROM tickets WHERE parent_id='parent'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(main_board, 1);
        assert_eq!(progress, (2, 1));
    }

    #[test]
    fn migration_catalogs_historical_workspaces_and_validates_repositories() {
        let db = Connection::open_in_memory().unwrap();
        migrate(&db).unwrap();
        db.execute("INSERT INTO tickets VALUES ('ticket','Ticket','','task','backlog','codex','model','high','/tmp/repository',NULL,NULL,'now','now')", []).unwrap();
        migrate(&db).unwrap();
        let mapped_path: String = db.query_row("SELECT r.path FROM repositories r JOIN ticket_repositories tr ON tr.repository_id=r.id WHERE tr.ticket_id='ticket'", [], |r| r.get(0)).unwrap();
        assert_eq!(mapped_path, "/tmp/repository");
        assert_eq!(
            valid_repository(&CreateRepository {
                name: String::new(),
                description: String::new(),
                path: String::new()
            }),
            Err("Completá: nombre y ruta.".into())
        );
        assert!(valid_repository(&CreateRepository {
            name: "Repo".into(),
            description: String::new(),
            path: "/tmp/repository".into()
        })
        .is_ok());
    }

    #[test]
    fn repository_reference_resolves_the_ticket_workspace() {
        let db = Connection::open_in_memory().unwrap();
        migrate(&db).unwrap();
        db.execute(
            "INSERT INTO repositories VALUES ('repo','Repo','', '/tmp/repository','now')",
            [],
        )
        .unwrap();
        assert_eq!(
            repository_path(&db, &Some("repo".into())).unwrap(),
            "/tmp/repository"
        );
        assert!(repository_path(&db, &None).is_err());
        assert!(db
            .execute(
                "INSERT INTO repositories VALUES ('other','Otro','', '/tmp/repository','now')",
                []
            )
            .is_err());
    }

    #[test]
    fn deleting_a_ticket_removes_its_repository_reference() {
        let mut db = Connection::open_in_memory().unwrap();
        migrate(&db).unwrap();
        db.execute(
            "INSERT INTO repositories VALUES ('repo','Repo','', '/tmp/repository','now')",
            [],
        )
        .unwrap();
        db.execute("INSERT INTO tickets VALUES ('ticket','Ticket','','task','backlog','codex','model','high','/tmp/repository',NULL,NULL,'now','now')", []).unwrap();
        db.execute(
            "INSERT INTO ticket_repositories VALUES ('ticket','repo')",
            [],
        )
        .unwrap();
        delete_ticket_records(&mut db, "ticket").unwrap();
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM tickets WHERE id='ticket'", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap(),
            0
        );
        assert_eq!(
            db.query_row(
                "SELECT COUNT(*) FROM ticket_repositories WHERE ticket_id='ticket'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }

    #[test]
    fn deletes_unused_repositories_but_blocks_those_with_tickets() {
        let db = Connection::open_in_memory().unwrap();
        migrate(&db).unwrap();
        db.execute(
            "INSERT INTO repositories VALUES ('free','Free','', '/tmp/free','now')",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO repositories VALUES ('used','Used','', '/tmp/used','now')",
            [],
        )
        .unwrap();
        db.execute("INSERT INTO tickets VALUES ('ticket','Ticket','','task','backlog','codex','model','high','/tmp/used',NULL,NULL,'now','now')", []).unwrap();
        db.execute(
            "INSERT INTO ticket_repositories VALUES ('ticket','used')",
            [],
        )
        .unwrap();

        assert!(delete_repository_record(&db, "used").is_err());
        assert_eq!(
            db.query_row(
                "SELECT COUNT(*) FROM repositories WHERE id='used'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );

        assert!(delete_repository_record(&db, "free").is_ok());
        assert_eq!(
            db.query_row(
                "SELECT COUNT(*) FROM repositories WHERE id='free'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );

        assert!(delete_repository_record(&db, "missing").is_err());
    }

    #[test]
    fn archives_only_done_tickets_and_can_restore_them() {
        let db = Connection::open_in_memory().unwrap();
        migrate(&db).unwrap();
        db.execute("INSERT INTO tickets VALUES ('done','Done','','task','done','codex','model','high','/tmp',NULL,NULL,'now','now')", []).unwrap();
        db.execute("INSERT INTO tickets VALUES ('todo','Todo','','task','todo','codex','model','high','/tmp',NULL,NULL,'now','now')", []).unwrap();
        assert!(set_ticket_archive(&db, "done", true).is_ok());
        assert_eq!(
            db.query_row(
                "SELECT COUNT(*) FROM ticket_archives WHERE ticket_id='done'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            set_ticket_archive(&db, "todo", true),
            Err("Sólo se pueden archivar tickets en Done.".into())
        );
        set_ticket_archive(&db, "done", false).unwrap();
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM ticket_archives", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn git_validation_branch_and_terminal_execution_states() {
        let invalid_workspace =
            std::env::temp_dir().join(format!("palantir-not-a-repo-{}", Uuid::new_v4()));
        fs::create_dir(&invalid_workspace).unwrap();
        assert!(inspect_git(invalid_workspace.to_str().unwrap()).is_err());
        fs::remove_dir(&invalid_workspace).unwrap();

        let workspace = std::env::temp_dir().join(format!("palantir-git-test-{}", Uuid::new_v4()));
        Command::new("git")
            .args(["init", "--initial-branch=main", workspace.to_str().unwrap()])
            .status()
            .unwrap();
        let ticket = test_ticket("codex");
        let branch = branch_name(&ticket);
        assert!(git(workspace.to_str().unwrap(), &["checkout", "-b", &branch]).is_ok());
        assert_eq!(
            git(workspace.to_str().unwrap(), &["branch", "--show-current"]).unwrap(),
            branch
        );
        fs::remove_dir_all(&workspace).unwrap();

        assert_eq!(terminal_state(true), ("done", "review"));
        assert_eq!(terminal_state(false), ("failed", "todo"));
    }
}
