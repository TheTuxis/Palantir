// Headless Palantir server: the same kind of local Git + CLI-agent execution
// the desktop app does, but reachable over HTTP so a desktop client's
// "remote" execution profile can point at a machine other than its own.
//
// This is an independent instance with its own SQLite database, ticket
// board and execution queue (not a stateless worker) — run it on whichever
// machine has your repositories and the `codex`/`claude`/`opencode` CLIs installed.
//
// Not yet ported from the desktop app: planning tickets/sub-kanban,
// archiving, follow-up/resume, retry and stop. Those remain desktop-only
// for now; this covers the core loop: repositories, tickets, the queue and
// running an agent to completion.

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    env,
    io::Cursor,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
};
use tiny_http::{Header, Method, Request, Response, Server};
use uuid::Uuid;

// ---------------------------------------------------------------- domain --

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Ticket {
    id: String,
    title: String,
    description: String,
    status: String,
    agent: String,
    model: String,
    reasoning: String,
    workspace: String,
    branch: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateTicket {
    title: String,
    description: String,
    agent: String,
    model: String,
    reasoning: String,
    repository_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateTicket {
    title: String,
    description: String,
    agent: String,
    model: String,
    reasoning: String,
    repository_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MoveTicket {
    status: String,
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
    base_commit: Option<String>,
    changes_detected: Option<String>,
    session_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct QueueState {
    ticket_id: String,
    position: u32,
    retry_required: bool,
    last_error: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
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

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Preferences {
    max_concurrent_tasks: u32,
    enabled_agents: Vec<String>,
    allowed_models: Vec<String>,
    allowed_reasoning: Vec<String>,
}

fn default_preferences() -> Preferences {
    Preferences {
        max_concurrent_tasks: 1,
        enabled_agents: vec!["codex".into(), "claude".into(), "opencode".into()],
        allowed_models: vec![
            "gpt-5.6-sol".into(),
            "gpt-5.6-terra".into(),
            "gpt-5.6-luna".into(),
            "claude-opus-5".into(),
            "claude-sonnet-5".into(),
            "claude-haiku-4-5-20251001".into(),
            "openai/gpt-5.6-sol".into(),
            "openai/gpt-5.6-terra".into(),
            "openai/gpt-5.6-luna".into(),
        ],
        allowed_reasoning: vec![
            "low".into(),
            "medium".into(),
            "high".into(),
            "xhigh".into(),
            "max".into(),
        ],
    }
}

// -------------------------------------------------------- agent adapters --
// Ported from the desktop app's AgentAdapter/CodexAdapter/ClaudeAdapter:
// kept as a direct copy (rather than a shared dependency) so this crate can
// evolve independently without risking the tested desktop code.

struct AgentInvocation {
    program: &'static str,
    args: Vec<String>,
}

trait AgentAdapter {
    fn start(&self, ticket: &Ticket, prompt: &str) -> AgentInvocation;
    fn session_id_from_output(&self, _output: &str) -> Option<String> {
        None
    }
}

struct CodexAdapter;
impl AgentAdapter for CodexAdapter {
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
    fn session_id_from_output(&self, output: &str) -> Option<String> {
        serde_json::from_str::<serde_json::Value>(output)
            .ok()?
            .get("session_id")?
            .as_str()
            .map(str::to_owned)
    }
}

struct OpenCodeAdapter;
impl AgentAdapter for OpenCodeAdapter {
    fn start(&self, ticket: &Ticket, prompt: &str) -> AgentInvocation {
        AgentInvocation {
            program: "opencode",
            args: vec![
                "run".into(),
                "--model".into(),
                ticket.model.clone(),
                "--variant".into(),
                ticket.reasoning.clone(),
                "--auto".into(),
                "--format".into(),
                "json".into(),
                prompt.into(),
            ],
        }
    }
    fn session_id_from_output(&self, output: &str) -> Option<String> {
        output.lines().find_map(|line| {
            let event: serde_json::Value = serde_json::from_str(line).ok()?;
            event.get("sessionID")?.as_str().map(str::to_owned)
        })
    }
}

fn adapter_for(agent: &str) -> Result<Box<dyn AgentAdapter>, String> {
    match agent {
        "codex" => Ok(Box::new(CodexAdapter)),
        "claude" => Ok(Box::new(ClaudeAdapter)),
        "opencode" => Ok(Box::new(OpenCodeAdapter)),
        _ => Err("Agente no compatible.".into()),
    }
}

// ------------------------------------------------------------- git/exec --

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
        // opencode: `{"type":"text","part":{"text":"..."}}`.
        if event.get("type").and_then(serde_json::Value::as_str) == Some("text") {
            if let Some(text) = event.pointer("/part/text").and_then(serde_json::Value::as_str) {
                summary = Some(text.to_owned());
            }
            continue;
        }
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

// ------------------------------------------------------------------ db --

fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(
        "PRAGMA foreign_keys=ON;
        CREATE TABLE IF NOT EXISTS repositories (id TEXT PRIMARY KEY,name TEXT NOT NULL,description TEXT NOT NULL DEFAULT '',path TEXT NOT NULL UNIQUE,created_at TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS tickets (id TEXT PRIMARY KEY,title TEXT NOT NULL,description TEXT NOT NULL,status TEXT NOT NULL,agent TEXT NOT NULL,model TEXT NOT NULL,reasoning TEXT NOT NULL,workspace TEXT NOT NULL,branch TEXT,repository_id TEXT,created_at TEXT NOT NULL,updated_at TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS executions (id TEXT PRIMARY KEY,ticket_id TEXT NOT NULL,session_id TEXT,status TEXT NOT NULL,base_commit TEXT,started_at TEXT NOT NULL,finished_at TEXT,summary TEXT,changes_detected TEXT);
        CREATE TABLE IF NOT EXISTS execution_queue (ticket_id TEXT PRIMARY KEY,queued_at TEXT NOT NULL,retry_required INTEGER NOT NULL DEFAULT 0,last_error TEXT);
        CREATE TABLE IF NOT EXISTS preferences (id INTEGER PRIMARY KEY CHECK(id=1), value TEXT NOT NULL);",
    )?;
    let preferences = serde_json::to_string(&default_preferences()).unwrap();
    db.execute(
        "INSERT OR IGNORE INTO preferences (id,value) VALUES(1,?1)",
        params![preferences],
    )?;
    Ok(())
}

fn preferences(db: &Connection) -> Result<Preferences, String> {
    let value: String = db
        .query_row("SELECT value FROM preferences WHERE id=1", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&value).map_err(|_| "Preferencias inválidas.".into())
}

fn ticket_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Ticket> {
    Ok(Ticket {
        id: r.get(0)?,
        title: r.get(1)?,
        description: r.get(2)?,
        status: r.get(3)?,
        agent: r.get(4)?,
        model: r.get(5)?,
        reasoning: r.get(6)?,
        workspace: r.get(7)?,
        branch: r.get(8)?,
        created_at: r.get(9)?,
        updated_at: r.get(10)?,
    })
}

const TICKET_COLUMNS: &str =
    "id,title,description,status,agent,model,reasoning,workspace,branch,created_at,updated_at";

fn repository_path(db: &Connection, repository_id: &str) -> Result<String, String> {
    db.query_row(
        "SELECT path FROM repositories WHERE id=?1",
        params![repository_id],
        |r| r.get(0),
    )
    .map_err(|_| "El repositorio seleccionado ya no existe.".to_string())
}

// ------------------------------------------------------------- app state --

struct AppState {
    db: Mutex<Connection>,
    db_path: PathBuf,
    token: String,
}

fn reserve_next_queued(db: &mut Connection) -> Result<Option<String>, String> {
    let prefs = preferences(db)?;
    let running: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM tickets WHERE status='running'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if running as u32 >= prefs.max_concurrent_tasks {
        return Ok(None);
    }
    let next: Option<(String, bool)> = db
        .query_row(
            "SELECT ticket_id,retry_required FROM execution_queue ORDER BY queued_at ASC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get::<_, i64>(1)? == 1)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let Some((ticket_id, retry_required)) = next else {
        return Ok(None);
    };
    if retry_required {
        return Ok(None);
    }
    db.execute(
        "DELETE FROM execution_queue WHERE ticket_id=?1",
        params![ticket_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(Some(ticket_id))
}

fn schedule_queued(state: &Arc<AppState>) {
    loop {
        let next_id = {
            let mut db = match state.db.lock() {
                Ok(db) => db,
                Err(_) => return,
            };
            match reserve_next_queued(&mut db) {
                Ok(next) => next,
                Err(_) => return,
            }
        };
        let Some(id) = next_id else { return };
        if let Err(error) = start_agent(&id, state) {
            if let Ok(db) = state.db.lock() {
                let _ = db.execute(
                    "UPDATE tickets SET status='todo',updated_at=?1 WHERE id=?2",
                    params![Utc::now().to_rfc3339(), id],
                );
                let _ = db.execute(
                    "INSERT INTO execution_queue (ticket_id,queued_at,retry_required,last_error) VALUES(?1,?2,1,?3)",
                    params![id, Utc::now().to_rfc3339(), error],
                );
            }
        }
    }
}

fn start_agent(ticket_id: &str, state: &Arc<AppState>) -> Result<(), String> {
    let db = state.db.lock().map_err(|_| "Base de datos no disponible")?;
    let ticket: Ticket = db
        .query_row(
            &format!("SELECT {TICKET_COLUMNS} FROM tickets WHERE id=?1"),
            params![ticket_id],
            ticket_row,
        )
        .map_err(|_| "Ticket no encontrado.".to_string())?;
    let base_commit = git(&ticket.workspace, &["rev-parse", "HEAD"]).ok();
    let adapter = adapter_for(&ticket.agent)?;
    let prompt = format!("{}\n\n{}", ticket.title, ticket.description);
    let invocation = adapter.start(&ticket, &prompt);
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
    db.execute(
        "INSERT INTO executions (id,ticket_id,session_id,status,base_commit,started_at) VALUES(?1,?2,NULL,'running',?3,?4)",
        params![execution, ticket.id, base_commit, Utc::now().to_rfc3339()],
    )
    .map_err(|e| e.to_string())?;
    db.execute(
        "UPDATE tickets SET status='running',updated_at=?1 WHERE id=?2",
        params![Utc::now().to_rfc3339(), ticket.id],
    )
    .map_err(|e| e.to_string())?;
    drop(db);

    let db_path = state.db_path.clone();
    let agent = ticket.agent.clone();
    let workspace = ticket.workspace.clone();
    let state_for_thread = state.clone();
    let ticket_id = ticket.id.clone();
    std::thread::spawn(move || {
        let result = child.wait_with_output();
        let db = match Connection::open(&db_path) {
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
            "UPDATE executions SET session_id=?1,status=?2,finished_at=?3,summary=?4,changes_detected=?5 WHERE id=?6",
            params![session_id, status, Utc::now().to_rfc3339(), summary, changes_detected, execution],
        );
        let _ = db.execute(
            "UPDATE tickets SET status=?1,updated_at=?2 WHERE id=?3",
            params![ticket_status, Utc::now().to_rfc3339(), ticket_id],
        );
        schedule_queued(&state_for_thread);
    });
    Ok(())
}

// ------------------------------------------------------------------ http --

fn json_response(status: u16, body: &impl Serialize) -> Response<Cursor<Vec<u8>>> {
    let data = serde_json::to_vec(body).unwrap_or_else(|_| b"null".to_vec());
    let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
    Response::from_data(data)
        .with_status_code(status)
        .with_header(header)
}

fn error_response(status: u16, message: impl Into<String>) -> Response<Cursor<Vec<u8>>> {
    json_response(status, &serde_json::json!({ "error": message.into() }))
}

fn read_body<T: for<'de> Deserialize<'de>>(request: &mut Request) -> Result<T, String> {
    let mut raw = String::new();
    request
        .as_reader()
        .read_to_string(&mut raw)
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&raw).map_err(|e| format!("Cuerpo inválido: {e}"))
}

fn authorized(request: &Request, token: &str) -> bool {
    request.headers().iter().any(|header| {
        header.field.equiv("Authorization") && header.value.as_str() == format!("Bearer {token}")
    })
}

fn handle(mut request: Request, state: Arc<AppState>) {
    let method = request.method().clone();
    let url = request.url().to_string();
    let segments: Vec<&str> = url.trim_matches('/').split('/').filter(|s| !s.is_empty()).collect();

    if segments == ["health"] {
        let _ = request.respond(json_response(200, &serde_json::json!({ "status": "ok" })));
        return;
    }
    if !authorized(&request, &state.token) {
        let _ = request.respond(error_response(401, "Token inválido o ausente."));
        return;
    }

    let response = match (&method, segments.as_slice()) {
        (Method::Get, ["tickets"]) => list_tickets(&state),
        (Method::Post, ["tickets"]) => create_ticket(&mut request, &state),
        (Method::Put, ["tickets", id]) => update_ticket(&mut request, &state, id),
        (Method::Delete, ["tickets", id]) => delete_ticket(&state, id),
        (Method::Post, ["tickets", id, "move"]) => move_ticket(&mut request, &state, id),
        (Method::Get, ["queue"]) => list_queue(&state),
        (Method::Get, ["sessions"]) => list_sessions(&state),
        (Method::Get, ["repositories"]) => list_repositories(&state),
        (Method::Post, ["repositories"]) => create_repository(&mut request, &state),
        (Method::Put, ["repositories", id]) => update_repository(&mut request, &state, id),
        (Method::Delete, ["repositories", id]) => delete_repository(&state, id),
        (Method::Get, ["preferences"]) => get_preferences(&state),
        (Method::Put, ["preferences"]) => update_preferences(&mut request, &state),
        _ => Err((404, "No encontrado.".to_string())),
    };
    let _ = request.respond(match response {
        Ok(body) => body,
        Err((status, message)) => error_response(status, message),
    });
}

type HandlerResult = Result<Response<Cursor<Vec<u8>>>, (u16, String)>;

fn list_tickets(state: &Arc<AppState>) -> HandlerResult {
    let db = state.db.lock().map_err(|_| (500, "Base de datos no disponible".into()))?;
    let mut statement = db
        .prepare(&format!("SELECT {TICKET_COLUMNS} FROM tickets ORDER BY created_at DESC"))
        .map_err(|e| (500, e.to_string()))?;
    let tickets = statement
        .query_map([], ticket_row)
        .map_err(|e| (500, e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| (500, e.to_string()))?;
    Ok(json_response(200, &tickets))
}

fn valid_ticket_input(title: &str, agent: &str, model: &str, reasoning: &str) -> Result<(), String> {
    if title.trim().is_empty() {
        return Err("Completá: título.".into());
    }
    if !matches!(agent, "codex" | "claude" | "opencode") {
        return Err("Agente no compatible.".into());
    }
    if model.trim().is_empty() || reasoning.trim().is_empty() {
        return Err("Completá: modelo y nivel de razonamiento.".into());
    }
    Ok(())
}

fn create_ticket(request: &mut Request, state: &Arc<AppState>) -> HandlerResult {
    let input: CreateTicket = read_body(request).map_err(|e| (400, e))?;
    valid_ticket_input(&input.title, &input.agent, &input.model, &input.reasoning)
        .map_err(|e| (422, e))?;
    let db = state.db.lock().map_err(|_| (500, "Base de datos no disponible".into()))?;
    let workspace = repository_path(&db, &input.repository_id).map_err(|e| (422, e))?;
    let now = Utc::now().to_rfc3339();
    let ticket = Ticket {
        id: Uuid::new_v4().to_string(),
        title: input.title.trim().into(),
        description: input.description.trim().into(),
        status: "backlog".into(),
        agent: input.agent,
        model: input.model,
        reasoning: input.reasoning,
        workspace,
        branch: None,
        created_at: now.clone(),
        updated_at: now,
    };
    db.execute(
        "INSERT INTO tickets (id,title,description,status,agent,model,reasoning,workspace,branch,repository_id,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,NULL,?9,?10,?11)",
        params![ticket.id, ticket.title, ticket.description, ticket.status, ticket.agent, ticket.model, ticket.reasoning, ticket.workspace, input.repository_id, ticket.created_at, ticket.updated_at],
    )
    .map_err(|e| (500, e.to_string()))?;
    Ok(json_response(201, &ticket))
}

fn update_ticket(request: &mut Request, state: &Arc<AppState>, id: &str) -> HandlerResult {
    let input: UpdateTicket = read_body(request).map_err(|e| (400, e))?;
    valid_ticket_input(&input.title, &input.agent, &input.model, &input.reasoning)
        .map_err(|e| (422, e))?;
    let db = state.db.lock().map_err(|_| (500, "Base de datos no disponible".into()))?;
    let workspace = repository_path(&db, &input.repository_id).map_err(|e| (422, e))?;
    let now = Utc::now().to_rfc3339();
    let n = db
        .execute(
            "UPDATE tickets SET title=?1,description=?2,agent=?3,model=?4,reasoning=?5,workspace=?6,repository_id=?7,updated_at=?8 WHERE id=?9",
            params![input.title.trim(), input.description.trim(), input.agent, input.model, input.reasoning, workspace, input.repository_id, now, id],
        )
        .map_err(|e| (500, e.to_string()))?;
    if n == 0 {
        return Err((404, "Ticket no encontrado.".into()));
    }
    let ticket: Ticket = db
        .query_row(&format!("SELECT {TICKET_COLUMNS} FROM tickets WHERE id=?1"), params![id], ticket_row)
        .map_err(|e| (500, e.to_string()))?;
    Ok(json_response(200, &ticket))
}

fn delete_ticket(state: &Arc<AppState>, id: &str) -> HandlerResult {
    let db = state.db.lock().map_err(|_| (500, "Base de datos no disponible".into()))?;
    db.execute("DELETE FROM execution_queue WHERE ticket_id=?1", params![id])
        .map_err(|e| (500, e.to_string()))?;
    db.execute("DELETE FROM executions WHERE ticket_id=?1", params![id])
        .map_err(|e| (500, e.to_string()))?;
    let n = db
        .execute("DELETE FROM tickets WHERE id=?1", params![id])
        .map_err(|e| (500, e.to_string()))?;
    if n == 0 {
        return Err((404, "Ticket no encontrado.".into()));
    }
    Ok(json_response(200, &serde_json::json!({ "deleted": true })))
}

fn valid_transition(from: &str, to: &str) -> bool {
    matches!((from, to), ("backlog", "todo") | ("review", "backlog" | "done"))
}

fn move_ticket(request: &mut Request, state: &Arc<AppState>, id: &str) -> HandlerResult {
    let input: MoveTicket = read_body(request).map_err(|e| (400, e))?;
    let db = state.db.lock().map_err(|_| (500, "Base de datos no disponible".into()))?;
    let ticket: Ticket = db
        .query_row(&format!("SELECT {TICKET_COLUMNS} FROM tickets WHERE id=?1"), params![id], ticket_row)
        .map_err(|_| (404, "Ticket no encontrado.".to_string()))?;
    if !valid_transition(&ticket.status, &input.status) {
        return Err((422, "Transición no permitida.".into()));
    }
    if input.status == "todo" {
        if git(&ticket.workspace, &["rev-parse", "--is-inside-work-tree"]).map_err(|e| (422, e))? != "true" {
            return Err((422, "La carpeta configurada no es un repositorio Git válido.".into()));
        }
        let branch = ticket.branch.clone().unwrap_or_else(|| branch_name(&ticket));
        if git(&ticket.workspace, &["branch", "--list", &branch]).map_err(|e| (500, e))?.is_empty() {
            git(&ticket.workspace, &["checkout", "-b", &branch]).map_err(|e| (500, e))?;
        } else {
            git(&ticket.workspace, &["checkout", &branch]).map_err(|e| (500, e))?;
        }
        db.execute(
            "UPDATE tickets SET status='todo',branch=?1,updated_at=?2 WHERE id=?3",
            params![branch, Utc::now().to_rfc3339(), ticket.id],
        )
        .map_err(|e| (500, e.to_string()))?;
        db.execute(
            "INSERT OR REPLACE INTO execution_queue (ticket_id,queued_at,retry_required,last_error) VALUES(?1,?2,0,NULL)",
            params![id, Utc::now().to_rfc3339()],
        )
        .map_err(|e| (500, e.to_string()))?;
        drop(db);
        schedule_queued(state);
        return Ok(json_response(200, &serde_json::json!({ "moved": true })));
    }
    db.execute(
        "UPDATE tickets SET status=?1,updated_at=?2 WHERE id=?3",
        params![input.status, Utc::now().to_rfc3339(), ticket.id],
    )
    .map_err(|e| (500, e.to_string()))?;
    Ok(json_response(200, &serde_json::json!({ "moved": true })))
}

fn list_queue(state: &Arc<AppState>) -> HandlerResult {
    let db = state.db.lock().map_err(|_| (500, "Base de datos no disponible".into()))?;
    let mut statement = db
        .prepare("SELECT ticket_id,retry_required,last_error FROM execution_queue ORDER BY queued_at ASC")
        .map_err(|e| (500, e.to_string()))?;
    let rows = statement
        .query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? == 1, r.get::<_, Option<String>>(2)?))
        })
        .map_err(|e| (500, e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| (500, e.to_string()))?;
    let queue: Vec<QueueState> = rows
        .into_iter()
        .enumerate()
        .map(|(index, (ticket_id, retry_required, last_error))| QueueState {
            ticket_id,
            position: index as u32 + 1,
            retry_required,
            last_error,
        })
        .collect();
    Ok(json_response(200, &queue))
}

fn list_sessions(state: &Arc<AppState>) -> HandlerResult {
    let db = state.db.lock().map_err(|_| (500, "Base de datos no disponible".into()))?;
    let mut statement = db
        .prepare(
            "SELECT e.id,e.ticket_id,t.title,t.agent,e.status,e.started_at,e.finished_at,e.summary,e.base_commit,e.changes_detected,e.session_id \
             FROM executions e JOIN tickets t ON t.id=e.ticket_id ORDER BY e.started_at DESC",
        )
        .map_err(|e| (500, e.to_string()))?;
    let sessions = statement
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
                base_commit: r.get(8)?,
                changes_detected: r.get(9)?,
                session_id: r.get(10)?,
            })
        })
        .map_err(|e| (500, e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| (500, e.to_string()))?;
    Ok(json_response(200, &sessions))
}

fn list_repositories(state: &Arc<AppState>) -> HandlerResult {
    let db = state.db.lock().map_err(|_| (500, "Base de datos no disponible".into()))?;
    let mut statement = db
        .prepare("SELECT id,name,description,path,created_at FROM repositories ORDER BY name COLLATE NOCASE")
        .map_err(|e| (500, e.to_string()))?;
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
        .map_err(|e| (500, e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| (500, e.to_string()))?;
    Ok(json_response(200, &repositories))
}

fn valid_repository(input: &CreateRepository) -> Result<(), String> {
    if input.name.trim().is_empty() || input.path.trim().is_empty() {
        Err("Completá: nombre y ruta.".into())
    } else {
        Ok(())
    }
}

fn create_repository(request: &mut Request, state: &Arc<AppState>) -> HandlerResult {
    let input: CreateRepository = read_body(request).map_err(|e| (400, e))?;
    valid_repository(&input).map_err(|e| (422, e))?;
    let repository = Repository {
        id: Uuid::new_v4().to_string(),
        name: input.name.trim().into(),
        description: input.description.trim().into(),
        path: input.path.trim().into(),
        created_at: Utc::now().to_rfc3339(),
    };
    let db = state.db.lock().map_err(|_| (500, "Base de datos no disponible".into()))?;
    db.execute(
        "INSERT INTO repositories VALUES(?1,?2,?3,?4,?5)",
        params![repository.id, repository.name, repository.description, repository.path, repository.created_at],
    )
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            (409, "La ruta ya está registrada.".to_string())
        } else {
            (500, e.to_string())
        }
    })?;
    Ok(json_response(201, &repository))
}

fn update_repository(request: &mut Request, state: &Arc<AppState>, id: &str) -> HandlerResult {
    let input: CreateRepository = read_body(request).map_err(|e| (400, e))?;
    valid_repository(&input).map_err(|e| (422, e))?;
    let db = state.db.lock().map_err(|_| (500, "Base de datos no disponible".into()))?;
    let created_at: String = db
        .query_row("SELECT created_at FROM repositories WHERE id=?1", params![id], |r| r.get(0))
        .map_err(|_| (404, "Repositorio no encontrado.".to_string()))?;
    let repository = Repository {
        id: id.to_string(),
        name: input.name.trim().into(),
        description: input.description.trim().into(),
        path: input.path.trim().into(),
        created_at,
    };
    db.execute(
        "UPDATE repositories SET name=?1,description=?2,path=?3 WHERE id=?4",
        params![repository.name, repository.description, repository.path, repository.id],
    )
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            (409, "La ruta ya está registrada.".to_string())
        } else {
            (500, e.to_string())
        }
    })?;
    db.execute(
        "UPDATE tickets SET workspace=?1,updated_at=?2 WHERE repository_id=?3",
        params![repository.path, Utc::now().to_rfc3339(), repository.id],
    )
    .map_err(|e| (500, e.to_string()))?;
    Ok(json_response(200, &repository))
}

fn delete_repository(state: &Arc<AppState>, id: &str) -> HandlerResult {
    let db = state.db.lock().map_err(|_| (500, "Base de datos no disponible".into()))?;
    let in_use: i64 = db
        .query_row("SELECT COUNT(*) FROM tickets WHERE repository_id=?1", params![id], |r| r.get(0))
        .map_err(|e| (500, e.to_string()))?;
    if in_use > 0 {
        return Err((409, "No podés borrar un repositorio con tickets asociados.".into()));
    }
    let deleted = db
        .execute("DELETE FROM repositories WHERE id=?1", params![id])
        .map_err(|e| (500, e.to_string()))?;
    if deleted == 0 {
        return Err((404, "Repositorio no encontrado.".into()));
    }
    Ok(json_response(200, &serde_json::json!({ "deleted": true })))
}

fn get_preferences(state: &Arc<AppState>) -> HandlerResult {
    let db = state.db.lock().map_err(|_| (500, "Base de datos no disponible".into()))?;
    let prefs = preferences(&db).map_err(|e| (500, e))?;
    Ok(json_response(200, &prefs))
}

fn update_preferences(request: &mut Request, state: &Arc<AppState>) -> HandlerResult {
    let input: Preferences = read_body(request).map_err(|e| (400, e))?;
    if input.max_concurrent_tasks == 0 {
        return Err((422, "El límite de concurrencia debe ser mayor que cero.".into()));
    }
    if input.enabled_agents.iter().any(|agent| !matches!(agent.as_str(), "codex" | "claude" | "opencode")) {
        return Err((422, "Los agentes disponibles son Codex, Claude y OpenCode.".into()));
    }
    let db = state.db.lock().map_err(|_| (500, "Base de datos no disponible".into()))?;
    db.execute(
        "UPDATE preferences SET value=?1 WHERE id=1",
        params![serde_json::to_string(&input).map_err(|e| (500, e.to_string()))?],
    )
    .map_err(|e| (500, e.to_string()))?;
    drop(db);
    schedule_queued(state);
    Ok(json_response(200, &input))
}

// ------------------------------------------------------------------ main --

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut port: u16 = 8787;
    let mut db_path = PathBuf::from("palantir-server.sqlite3");
    let mut token = env::var("PALANTIR_TOKEN").ok();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--port" => {
                port = args.get(i + 1).and_then(|v| v.parse().ok()).unwrap_or(port);
                i += 1;
            }
            "--db" => {
                if let Some(value) = args.get(i + 1) {
                    db_path = PathBuf::from(value);
                }
                i += 1;
            }
            "--token" => {
                token = args.get(i + 1).cloned();
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    let token = token.unwrap_or_else(|| {
        let generated = Uuid::new_v4().to_string();
        eprintln!("No se pasó --token ni PALANTIR_TOKEN: generando uno para esta sesión.");
        eprintln!("Usalo como \"Authorization: Bearer {generated}\" desde el cliente.");
        generated
    });

    let db = Connection::open(&db_path).expect("no se pudo abrir la base de datos");
    migrate(&db).expect("no se pudo migrar la base de datos");
    // Recover from a crash mid-execution, same as the desktop app does on startup.
    let _ = db.execute("UPDATE tickets SET status='todo',updated_at=?1 WHERE status='running'", params![Utc::now().to_rfc3339()]);
    let _ = db.execute(
        "INSERT OR REPLACE INTO execution_queue (ticket_id,queued_at,retry_required,last_error) SELECT id,updated_at,1,'El servidor se reinició durante la ejecución; reintentá manualmente.' FROM tickets WHERE status='todo'",
        [],
    );

    let state = Arc::new(AppState {
        db: Mutex::new(db),
        db_path,
        token,
    });

    let server = Server::http(("0.0.0.0", port)).expect("no se pudo iniciar el servidor HTTP");
    println!("Palantir server escuchando en http://0.0.0.0:{port}");
    for request in server.incoming_requests() {
        let state = state.clone();
        std::thread::spawn(move || handle(request, state));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ticket() -> Ticket {
        Ticket {
            id: "12345678-test".into(),
            title: "Arreglar el bug".into(),
            description: String::new(),
            status: "todo".into(),
            agent: "codex".into(),
            model: "gpt-5.6-terra".into(),
            reasoning: "high".into(),
            workspace: "/tmp".into(),
            branch: None,
            created_at: "now".into(),
            updated_at: "now".into(),
        }
    }

    #[test]
    fn migration_creates_default_preferences() {
        let db = Connection::open_in_memory().unwrap();
        migrate(&db).unwrap();
        let prefs = preferences(&db).unwrap();
        assert_eq!(prefs.max_concurrent_tasks, 1);
        assert!(prefs.enabled_agents.contains(&"codex".to_string()));
        assert!(prefs.enabled_agents.contains(&"claude".to_string()));
    }

    #[test]
    fn branch_name_slugifies_the_title() {
        let branch = branch_name(&test_ticket());
        assert!(branch.starts_with("palantir/12345678-"));
        assert!(branch.contains("arreglar-el-bug"));
    }

    #[test]
    fn only_exposes_manual_kanban_transitions() {
        assert!(valid_transition("backlog", "todo"));
        assert!(valid_transition("review", "backlog"));
        assert!(valid_transition("review", "done"));
        assert!(!valid_transition("todo", "running"));
        assert!(!valid_transition("running", "review"));
    }

    #[test]
    fn adapters_reject_unsupported_agents() {
        assert!(adapter_for("codex").is_ok());
        assert!(adapter_for("claude").is_ok());
        assert!(adapter_for("gemini").is_err());
    }

    #[test]
    fn execution_summary_extracts_the_last_agent_message() {
        let log = "{\"item\":{\"type\":\"agent_message\",\"text\":\"listo\"}}\nnoise";
        assert_eq!(execution_summary(log), "listo");
        assert_eq!(execution_summary("plain text"), "plain text");
        let opencode_log = "{\"type\":\"step_start\"}\n{\"type\":\"text\",\"part\":{\"text\":\"hecho\"}}\n{\"type\":\"step_finish\"}";
        assert_eq!(execution_summary(opencode_log), "hecho");
    }

    #[test]
    fn repository_and_ticket_lifecycle_persists_correctly() {
        let db = Connection::open_in_memory().unwrap();
        migrate(&db).unwrap();
        db.execute(
            "INSERT INTO repositories VALUES ('repo','Repo','', '/tmp/repo','now')",
            [],
        )
        .unwrap();
        assert_eq!(repository_path(&db, "repo").unwrap(), "/tmp/repo");
        assert!(repository_path(&db, "missing").is_err());

        db.execute(
            "INSERT INTO tickets (id,title,description,status,agent,model,reasoning,workspace,branch,repository_id,created_at,updated_at) VALUES('t1','Test','','backlog','codex','model','high','/tmp/repo',NULL,'repo','now','now')",
            [],
        )
        .unwrap();
        let ticket: Ticket = db
            .query_row(&format!("SELECT {TICKET_COLUMNS} FROM tickets WHERE id='t1'"), [], ticket_row)
            .unwrap();
        assert_eq!(ticket.status, "backlog");
    }
}
