import { FormEvent, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./App.css";
import "./palantir-theme.css";
import "./app-functions.css";
import "./ticket-modal.css";
import "./modern-desktop.css";
import "./ticket-detail-modal.css";
import "./compact-desktop.css";
import "./repositories.css";

type Status = "backlog" | "todo" | "running" | "review" | "done";
type Ticket = {
  id: string; title: string; description: string; kind: "task" | "planning"; status: Status;
  agent: "codex" | "claude"; model: string; reasoning: string; workspace: string; branch?: string;
  archived?: boolean;
};
type Session = { id: string; ticketId: string; ticketTitle: string; agent: string; status: string; startedAt: string; finishedAt?: string; summary?: string; effectiveConfig: string; baseCommit?: string; changesDetected?: string };
type Plan = { ticketId: string; items: { title: string; description: string }[]; approvedAt?: string };
type Preflight = { dirtyWorktree: boolean; activeExecution: boolean };
type Repository = { id: string; name: string; description: string; path: string; createdAt: string };
type Preferences = { maxConcurrentTasks: number; enabledAgents: string[]; allowedModels: string[]; allowedReasoning: string[]; executionProfile: "local" | "remote"; remoteEndpoint: string; remoteToken: string };
type QueueState = { ticketId: string; position: number; retryRequired: boolean; lastError?: string };
type Draft = Omit<Ticket, "id" | "status" | "branch"> & { repositoryId: string };

const columns: [Status, string][] = [["backlog", "Backlog"], ["todo", "To do"], ["running", "In Progress"], ["review", "Review"], ["done", "Done"]];
const codexModels = [
  ["gpt-5.6-sol", "GPT-5.6 Sol · máxima capacidad"],
  ["gpt-5.6-terra", "GPT-5.6 Terra · equilibrio"],
  ["gpt-5.6-luna", "GPT-5.6 Luna · rápida y eficiente"],
] as const;
const claudeModels = [
  ["claude-opus-5", "Claude Opus 5 · máxima capacidad"],
  ["claude-sonnet-5", "Claude Sonnet 5 · equilibrio"],
  ["claude-haiku-4-5-20251001", "Claude Haiku 4.5 · rápida y eficiente"],
] as const;
const modelsByAgent: Record<Ticket["agent"], readonly (readonly [string, string])[]> = { codex: codexModels, claude: claudeModels };
const agentLabels: Record<Ticket["agent"], string> = { codex: "Codex", claude: "Claude" };
const allModels = [...codexModels, ...claudeModels];
const modelLabel = (model: string) => allModels.find(([id]) => id === model)?.[1] ?? model;
const statusName = (status: Status) => columns.find(([value]) => value === status)?.[1] ?? status;
const emptyDraft: Draft = { title: "", description: "", kind: "task", agent: "codex", model: "gpt-5.6-terra", reasoning: "high", workspace: "", repositoryId: "" };
const reviewSections = (text?: string) => {
  const source = text?.trim() || "Sin informe todavía.";
  const labels = ["Resumen", "Validaciones", "Decisiones", "Riesgos y pendientes"];
  const sections = new Map(labels.map((label) => [label, ""]));
  let current = "Resumen";
  for (const line of source.split("\n")) {
    const found = labels.find((label) => line.toLowerCase().includes(label.toLowerCase()));
    if (found) { current = found; continue; }
    sections.set(current, `${sections.get(current)}${sections.get(current) ? "\n" : ""}${line}`.trim());
  }
  return labels.map((label) => [label, sections.get(label) || (label === "Resumen" ? source : "No informado.")] as const);
};
const NavIcon = ({ kind }: { kind: "board" | "sessions" | "repos" | "settings" }) => <svg className="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{kind === "board" && <><rect x="3" y="4" width="18" height="16" rx="2" /><path d="M8 4v16M8 9h13M8 14h13" /></>}{kind === "sessions" && <><path d="M20 15a4 4 0 0 1-4 4H9l-5 3V8a4 4 0 0 1 4-4h8a4 4 0 0 1 4 4Z" /><path d="M8 10h8M8 14h5" /></>}{kind === "repos" && <><path d="M4 7.5h6l1.5 2H20v9.5H4Z" /><path d="M4 7.5V5h6l1.5 2.5" /></>}{kind === "settings" && <><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.7 1.7 0 0 0 .34 1.88l.06.06-2.1 2.1-.06-.06a1.7 1.7 0 0 0-1.88-.34 1.7 1.7 0 0 0-1.03 1.56v.1h-3v-.1A1.7 1.7 0 0 0 10.7 18.6a1.7 1.7 0 0 0-1.88.34l-.06.06-2.1-2.1.06-.06A1.7 1.7 0 0 0 7.06 15 1.7 1.7 0 0 0 5.5 14H5.4v-3h.1A1.7 1.7 0 0 0 7.06 10a1.7 1.7 0 0 0-.34-1.88l-.06-.06 2.1-2.1.06.06A1.7 1.7 0 0 0 10.7 6.36 1.7 1.7 0 0 0 11.73 4.8v-.1h3v.1a1.7 1.7 0 0 0 1.03 1.56 1.7 1.7 0 0 0 1.88-.34l.06-.06 2.1 2.1-.06.06A1.7 1.7 0 0 0 19.4 10c.14.6.69 1 1.3 1h.1v3h-.1c-.61 0-1.16.4-1.3 1Z" /></>}</svg>;
// Official marks (CC0, simple-icons): Claude's own icon; Codex has no dedicated mark, so it uses OpenAI's.
const AgentIcon = ({ agent }: { agent: Ticket["agent"] }) => agent === "claude"
  ? <svg className="agent-icon" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="m4.7144 15.9555 4.7174-2.6471.079-.2307-.079-.1275h-.2307l-.7893-.0486-2.6956-.0729-2.3375-.0971-2.2646-.1214-.5707-.1215-.5343-.7042.0546-.3522.4797-.3218.686.0608 1.5179.1032 2.2767.1578 1.6514.0972 2.4468.255h.3886l.0546-.1579-.1336-.0971-.1032-.0972L6.973 9.8356l-2.55-1.6879-1.3356-.9714-.7225-.4918-.3643-.4614-.1578-1.0078.6557-.7225.8803.0607.2246.0607.8925.686 1.9064 1.4754 2.4893 1.8336.3643.3035.1457-.1032.0182-.0728-.164-.2733-1.3539-2.4467-1.445-2.4893-.6435-1.032-.17-.6194c-.0607-.255-.1032-.4674-.1032-.7285L6.287.1335 6.6997 0l.9957.1336.419.3642.6192 1.4147 1.0018 2.2282 1.5543 3.0296.4553.8985.2429.8318.091.255h.1579v-.1457l.1275-1.706.2368-2.0947.2307-2.6957.0789-.7589.3764-.9107.7468-.4918.5828.2793.4797.686-.0668.4433-.2853 1.8517-.5586 2.9021-.3643 1.9429h.2125l.2429-.2429.9835-1.3053 1.6514-2.0643.7286-.8196.85-.9046.5464-.4311h1.0321l.759 1.1293-.34 1.1657-1.0625 1.3478-.8804 1.1414-1.2628 1.7-.7893 1.36.0729.1093.1882-.0183 2.8535-.607 1.5421-.2794 1.8396-.3157.8318.3886.091.3946-.3278.8075-1.967.4857-2.3072.4614-3.4364.8136-.0425.0304.0486.0607 1.5482.1457.6618.0364h1.621l3.0175.2247.7892.522.4736.6376-.079.4857-1.2142.6193-1.6393-.3886-3.825-.9107-1.3113-.3279h-.1822v.1093l1.0929 1.0686 2.0035 1.8092 2.5075 2.3314.1275.5768-.3218.4554-.34-.0486-2.2039-1.6575-.85-.7468-1.9246-1.621h-.1275v.17l.4432.6496 2.3436 3.5214.1214 1.0807-.17.3521-.6071.2125-.6679-.1214-1.3721-1.9246L14.38 17.959l-1.1414-1.9428-.1397.079-.674 7.2552-.3156.3703-.7286.2793-.6071-.4614-.3218-.7468.3218-1.4753.3886-1.9246.3157-1.53.2853-1.9004.17-.6314-.0121-.0425-.1397.0182-1.4328 1.9672-2.1796 2.9446-1.7243 1.8456-.4128.164-.7164-.3704.0667-.6618.4008-.5889 2.386-3.0357 1.4389-1.882.929-1.0868-.0062-.1579h-.0546l-6.3385 4.1164-1.1293.1457-.4857-.4554.0608-.7467.2307-.2429 1.9064-1.3114Z" /></svg>
  : <svg className="agent-icon" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M22.2819 9.8211a5.9847 5.9847 0 0 0-.5157-4.9108 6.0462 6.0462 0 0 0-6.5098-2.9A6.0651 6.0651 0 0 0 4.9807 4.1818a5.9847 5.9847 0 0 0-3.9977 2.9 6.0462 6.0462 0 0 0 .7427 7.0966 5.98 5.98 0 0 0 .511 4.9107 6.051 6.051 0 0 0 6.5146 2.9001A5.9847 5.9847 0 0 0 13.2599 24a6.0557 6.0557 0 0 0 5.7718-4.2058 5.9894 5.9894 0 0 0 3.9977-2.9001 6.0557 6.0557 0 0 0-.7475-7.0729zm-9.022 12.6081a4.4755 4.4755 0 0 1-2.8764-1.0408l.1419-.0804 4.7783-2.7582a.7948.7948 0 0 0 .3927-.6813v-6.7369l2.02 1.1686a.071.071 0 0 1 .038.052v5.5826a4.504 4.504 0 0 1-4.4945 4.4944zm-9.6607-4.1254a4.4708 4.4708 0 0 1-.5346-3.0137l.142.0852 4.783 2.7582a.7712.7712 0 0 0 .7806 0l5.8428-3.3685v2.3324a.0804.0804 0 0 1-.0332.0615L9.74 19.9502a4.4992 4.4992 0 0 1-6.1408-1.6464zM2.3408 7.8956a4.485 4.485 0 0 1 2.3655-1.9728V11.6a.7664.7664 0 0 0 .3879.6765l5.8144 3.3543-2.0201 1.1685a.0757.0757 0 0 1-.071 0l-4.8303-2.7865A4.504 4.504 0 0 1 2.3408 7.872zm16.5963 3.8558L13.1038 8.364 15.1192 7.2a.0757.0757 0 0 1 .071 0l4.8303 2.7913a4.4944 4.4944 0 0 1-.6765 8.1042v-5.6772a.79.79 0 0 0-.407-.667zm2.0107-3.0231l-.142-.0852-4.7735-2.7818a.7759.7759 0 0 0-.7854 0L9.409 9.2297V6.8974a.0662.0662 0 0 1 .0284-.0615l4.8303-2.7866a4.4992 4.4992 0 0 1 6.6802 4.66zM8.3065 12.863l-2.02-1.1638a.0804.0804 0 0 1-.038-.0567V6.0742a4.4992 4.4992 0 0 1 7.3757-3.4537l-.142.0805L8.704 5.459a.7948.7948 0 0 0-.3927.6813zm1.0976-2.3654l2.602-1.4998 2.6069 1.4998v2.9994l-2.5974 1.4997-2.6067-1.4997Z" /></svg>;

// Remote profile: proxied through the Rust `remote_request` command rather than a
// frontend `fetch`, since the Tauri webview enforces CORS like a browser and
// palantir-server doesn't send CORS headers — a direct fetch would just fail.
const isRemoteProfile = (prefs: Preferences) => prefs.executionProfile === "remote";
const remoteRequest = <T,>(prefs: Preferences, method: string, path: string, body?: unknown): Promise<T> =>
  invoke<T>("remote_request", { request: { endpoint: prefs.remoteEndpoint, token: prefs.remoteToken, method, path, body: body ?? null } });
// palantir-server has no planning/sub-kanban or archiving concept yet: normalize its
// tickets/sessions into the local shape so the rest of the UI doesn't need to know.
const toLocalTicket = (ticket: Omit<Ticket, "kind" | "archived">): Ticket => ({ ...ticket, kind: "task", archived: false });
const toLocalSession = (session: Omit<Session, "effectiveConfig">): Session => ({ ...session, effectiveConfig: "" });
const REMOTE_UNSUPPORTED = "Todavía no está disponible en modo remoto: por ahora sólo el tablero, repos, cola y sesiones.";

const apiListTickets = (prefs: Preferences) => isRemoteProfile(prefs)
  ? remoteRequest<Omit<Ticket, "kind" | "archived">[]>(prefs, "GET", "/tickets").then((rows) => rows.map(toLocalTicket))
  : invoke<Ticket[]>("list_tickets");
const apiListSessions = (prefs: Preferences) => isRemoteProfile(prefs)
  ? remoteRequest<Omit<Session, "effectiveConfig">[]>(prefs, "GET", "/sessions").then((rows) => rows.map(toLocalSession))
  : invoke<Session[]>("list_sessions");
const apiListRepositories = (prefs: Preferences) => isRemoteProfile(prefs)
  ? remoteRequest<Repository[]>(prefs, "GET", "/repositories")
  : invoke<Repository[]>("list_repositories");
const apiListQueueStates = (prefs: Preferences) => isRemoteProfile(prefs)
  ? remoteRequest<QueueState[]>(prefs, "GET", "/queue")
  : invoke<QueueState[]>("list_queue_states");
const apiCreateTicket = (prefs: Preferences, input: Draft) => isRemoteProfile(prefs)
  ? remoteRequest<Omit<Ticket, "kind" | "archived">>(prefs, "POST", "/tickets", input).then(toLocalTicket)
  : invoke<Ticket>("create_ticket", { input });
const apiUpdateTicket = (prefs: Preferences, id: string, input: Draft) => isRemoteProfile(prefs)
  ? remoteRequest<Omit<Ticket, "kind" | "archived">>(prefs, "PUT", `/tickets/${id}`, input).then(toLocalTicket)
  : invoke<Ticket>("update_ticket", { id, input });
const apiDeleteTicket = (prefs: Preferences, id: string) => isRemoteProfile(prefs)
  ? remoteRequest(prefs, "DELETE", `/tickets/${id}`)
  : invoke("delete_ticket", { id });
const apiMoveTicket = (prefs: Preferences, id: string, status: Status) => isRemoteProfile(prefs)
  ? remoteRequest(prefs, "POST", `/tickets/${id}/move`, { status })
  : invoke("move_ticket", { id, status });
const apiCreateRepository = (prefs: Preferences, input: { name: string; description: string; path: string }) => isRemoteProfile(prefs)
  ? remoteRequest<Repository>(prefs, "POST", "/repositories", input)
  : invoke<Repository>("create_repository", { input });
const apiUpdateRepository = (prefs: Preferences, id: string, input: { name: string; description: string; path: string }) => isRemoteProfile(prefs)
  ? remoteRequest<Repository>(prefs, "PUT", `/repositories/${id}`, input)
  : invoke<Repository>("update_repository", { id, input });
const apiDeleteRepository = (prefs: Preferences, id: string) => isRemoteProfile(prefs)
  ? remoteRequest(prefs, "DELETE", `/repositories/${id}`)
  : invoke("delete_repository", { id });

export default function App() {
  const [tickets, setTickets] = useState<Ticket[]>([]);
  const [sessions, setSessions] = useState<Session[]>([]);
  const [repositories, setRepositories] = useState<Repository[]>([]);
  const [repositoryDraft, setRepositoryDraft] = useState({ name: "", description: "", path: "" });
  const [editingRepositoryId, setEditingRepositoryId] = useState<string | null>(null);
  const [repositoryModalOpen, setRepositoryModalOpen] = useState(false);
  const [selected, setSelected] = useState<Ticket | null>(null);
  const [modalOpen, setModalOpen] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [draft, setDraft] = useState<Draft>(emptyDraft);
  const [notice, setNotice] = useState("");
  const [actionError, setActionError] = useState("");
  const [followUp, setFollowUp] = useState("");
  const [plan, setPlan] = useState<Plan | null>(null);
  const [subtickets, setSubtickets] = useState<Ticket[]>([]);
  const [section, setSection] = useState<"board" | "sessions" | "repos">("board");
  const [showArchivedDone, setShowArchivedDone] = useState(false);
  const [showArchivedSessions, setShowArchivedSessions] = useState(false);
  const [preferences, setPreferences] = useState<Preferences>({ maxConcurrentTasks: 1, enabledAgents: ["codex", "claude"], allowedModels: allModels.map(([model]) => model), allowedReasoning: ["low", "medium", "high", "xhigh", "max"], executionProfile: "local", remoteEndpoint: "", remoteToken: "" });
  const [remoteTestStatus, setRemoteTestStatus] = useState("");
  const [queueStates, setQueueStates] = useState<QueueState[]>([]);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const settingsOpenRef = useRef(settingsOpen);
  settingsOpenRef.current = settingsOpen;
  const preferencesRef = useRef(preferences);
  preferencesRef.current = preferences;
  const [railExpanded, setRailExpanded] = useState(true);
  const load = async () => {
    try {
      const prefs = preferencesRef.current;
      const remote = isRemoteProfile(prefs);
      const [rows, executionRows, repositoryRows, storedPreferences, queue] = await Promise.all([apiListTickets(prefs), apiListSessions(prefs), apiListRepositories(prefs), invoke<Preferences>("get_preferences"), apiListQueueStates(prefs)]);
      let ticketsWithArchiveState = rows;
      if (!remote) {
        const archivedTicketIds = await invoke<string[]>("list_archived_ticket_ids");
        const archived = new Set(archivedTicketIds);
        ticketsWithArchiveState = rows.map((ticket) => ({ ...ticket, archived: archived.has(ticket.id) }));
      }
      setTickets(ticketsWithArchiveState);
      setSessions(executionRows);
      setRepositories(repositoryRows);
      if (!settingsOpenRef.current) setPreferences(storedPreferences);
      setQueueStates(queue);
      setSelected((current) => current ? ticketsWithArchiveState.find((ticket) => ticket.id === current.id) ?? null : null);
    } catch (error) { setNotice(preferencesRef.current.executionProfile === "remote" ? `No se pudo leer el servidor remoto: ${String(error)}` : "Abrí Palantir con Tauri para usar el almacenamiento local."); }
  };
  useEffect(() => { void load(); const timer = window.setInterval(() => void load(), 2000); return () => window.clearInterval(timer); }, []);
  useEffect(() => {
    if (!selected || selected.kind !== "planning") { setPlan(null); setSubtickets([]); return; }
    void Promise.all([invoke<Plan | null>("get_plan", { id: selected.id }), invoke<Ticket[]>("list_subtickets", { parentId: selected.id })]).then(([storedPlan, children]) => { setPlan(storedPlan); setSubtickets(children); }).catch((error) => setActionError(String(error)));
  }, [selected?.id, selected?.kind]);
  const field = (key: keyof Draft) => (event: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement>) => setDraft({ ...draft, [key]: event.target.value });
  const selectAgent = (agent: Ticket["agent"]) => setDraft({ ...draft, agent, model: modelsByAgent[agent][0][0] });
  const toggleEnabledAgent = (agent: Ticket["agent"]) => setPreferences((current) => ({
    ...current,
    enabledAgents: current.enabledAgents.includes(agent) ? current.enabledAgents.filter((value) => value !== agent) : [...current.enabledAgents, agent],
  }));
  const selectSection = (target: "board" | "sessions" | "repos") => {
    if (section === target) setRailExpanded((current) => !current);
    else setSection(target);
  };
  const create = async (event: FormEvent) => {
    event.preventDefault();
    try {
      const ticket = editingId
        ? await apiUpdateTicket(preferences, editingId, draft)
        : await apiCreateTicket(preferences, draft);
      setTickets((current) => editingId ? current.map((item) => item.id === ticket.id ? ticket : item) : [ticket, ...current]);
      setSelected(ticket); setDraft(emptyDraft); setEditingId(null); setModalOpen(false); setNotice(editingId ? "Configuración del ticket actualizada." : "Ticket creado en Backlog.");
    } catch (error) { setNotice(String(error)); }
  };
  const createRepository = async (event: FormEvent) => {
    event.preventDefault();
    const wasEditing = Boolean(editingRepositoryId);
    try { await (editingRepositoryId ? apiUpdateRepository(preferences, editingRepositoryId, repositoryDraft) : apiCreateRepository(preferences, repositoryDraft)); setRepositoryDraft({ name: "", description: "", path: "" }); setEditingRepositoryId(null); setRepositoryModalOpen(false); await load(); setNotice(wasEditing ? "Repositorio actualizado." : "Repositorio registrado."); }
    catch (error) { setNotice(String(error)); }
  };
  const openRepositoryModal = (repository?: Repository) => {
    setEditingRepositoryId(repository?.id ?? null);
    setRepositoryDraft(repository ? { name: repository.name, description: repository.description, path: repository.path } : { name: "", description: "", path: "" });
    setRepositoryModalOpen(true);
  };
  const closeRepositoryModal = () => {
    setRepositoryModalOpen(false);
    setEditingRepositoryId(null);
    setRepositoryDraft({ name: "", description: "", path: "" });
  };
  const deleteRepository = async (repository: Repository, ticketCount: number) => {
    if (ticketCount > 0) { setNotice(`No podés borrar "${repository.name}": tiene ${ticketCount === 1 ? "un ticket asociado" : `${ticketCount} tickets asociados`}.`); return; }
    if (!window.confirm(`¿Borrar el repositorio "${repository.name}"? Esta acción no se puede deshacer.`)) return;
    try { await apiDeleteRepository(preferences, repository.id); await load(); setNotice("Repositorio borrado."); }
    catch (error) { setNotice(String(error)); }
  };
  const move = async (status: Status) => {
    if (!selected) return;
    try {
      if (status === "todo" && !isRemoteProfile(preferences)) {
        const check = await invoke<Preflight>("preflight_ticket", { id: selected.id });
        if (check.dirtyWorktree) setNotice("Advertencia: el repositorio ya tiene cambios sin confirmar; se preservarán durante la ejecución.");
      }
      await apiMoveTicket(preferences, selected.id, status); await load();
      setNotice(status === "todo" ? "Rama preparada: el ticket quedó en la cola de ejecución." : `Ticket movido a ${statusName(status)}.`);
    } catch (error) { setNotice(String(error)); }
  };
  const retry = async () => {
    if (!selected) return;
    if (isRemoteProfile(preferences)) { setNotice(REMOTE_UNSUPPORTED); return; }
    try { await invoke("retry_ticket", { id: selected.id }); await load(); setNotice("Reintentando el ticket en la misma rama…"); }
    catch (error) { setNotice(String(error)); }
  };
  const stop = async () => {
    if (!lastSession) return;
    if (isRemoteProfile(preferences)) { setNotice(REMOTE_UNSUPPORTED); return; }
    try { await invoke("stop_execution", { id: lastSession.id }); await load(); setNotice("Solicitando la detención del agente…"); }
    catch (error) { setNotice(String(error)); }
  };
  const sendFollowUp = async () => {
    if (!selected) return;
    if (isRemoteProfile(preferences)) { setActionError(REMOTE_UNSUPPORTED); return; }
    try { await invoke("send_follow_up", { id: selected.id, message: followUp }); setFollowUp(""); await load(); setNotice("Seguimiento enviado; el agente retoma el contexto en la misma rama."); }
    catch (error) { setActionError(String(error)); }
  };
  const approvePlan = async () => {
    if (!selected) return;
    if (isRemoteProfile(preferences)) { setActionError(REMOTE_UNSUPPORTED); return; }
    try { await invoke("approve_plan", { id: selected.id }); await load(); const children = await invoke<Ticket[]>("list_subtickets", { parentId: selected.id }); setSubtickets(children); setPlan((current) => current ? { ...current, approvedAt: new Date().toISOString() } : current); setNotice("Plan aprobado: se creó el sub-kanban."); }
    catch (error) { setActionError(String(error)); }
  };
  const remove = async () => {
    if (!selected) return;
    setActionError("");
    try { await apiDeleteTicket(preferences, selected.id); setSelected(null); await load(); setNotice("Ticket borrado."); }
    catch (error) { setActionError(String(error)); }
  };
  const archive = async () => {
    if (!selected || selected.status !== "done") return;
    if (isRemoteProfile(preferences)) { setActionError(REMOTE_UNSUPPORTED); return; }
    try { await invoke(selected.archived ? "unarchive_ticket" : "archive_ticket", { id: selected.id }); setSelected(null); await load(); setNotice(selected.archived ? "Ticket restaurado en Done." : "Ticket archivado."); }
    catch (error) { setActionError(String(error)); }
  };
  const savePreferences = async (event: FormEvent) => {
    event.preventDefault();
    try { const saved = await invoke<Preferences>("update_preferences", { input: preferences }); setPreferences(saved); setSettingsOpen(false); await load(); setNotice("Configuración guardada. La cola usa el nuevo límite desde ahora."); }
    catch (error) { setNotice(String(error)); }
  };
  const testRemoteConnection = async () => {
    setRemoteTestStatus("Probando…");
    try { const result = await invoke<string>("test_remote_connection", { endpoint: preferences.remoteEndpoint, token: preferences.remoteToken }); setRemoteTestStatus(result); }
    catch (error) { setRemoteTestStatus(String(error)); }
  };
  const availableActions = (ticket: Ticket): [Status, string][] => {
    if (ticket.status === "backlog") return [["todo", "Enviar a To do"]];
    if (ticket.status === "review") return [["backlog", "Volver a Backlog"], ["done", "Marcar Done"]];
    return [];
  };
  const enabledAgents = (preferences.enabledAgents.length ? preferences.enabledAgents : ["codex"]) as Ticket["agent"][];
  const agentModelIds = modelsByAgent[draft.agent].map(([model]) => model);
  const allowedModels = agentModelIds.filter((model) => preferences.allowedModels.includes(model));
  const modelChoices = allowedModels.length ? allowedModels : agentModelIds;
  const modelOptions = modelChoices.some((model) => model === draft.model) ? modelChoices.map((model) => [model, modelLabel(model)] as const) : [[draft.model, `${draft.model} · modelo guardado`], ...modelChoices.map((model) => [model, modelLabel(model)] as const)];
  const lastSession = selected ? sessions.find((session) => session.ticketId === selected.id) : undefined;
  const repositoryFor = (ticket: Ticket) => repositories.find((repository) => repository.path === ticket.workspace);
  const pageTitle = section === "repos" ? "Repositorios" : section === "sessions" ? "Sesiones" : "Trabajo en curso";
  const contextLabel = section === "board" ? (isRemoteProfile(preferences) ? "Tablero remoto" : "Tablero local") : section === "sessions" ? "Historial de ejecuciones" : "Catálogo de workspaces";
  const archivedTicketIds = new Set(tickets.filter((ticket) => ticket.archived).map((ticket) => ticket.id));
  const visibleSessions = sessions.filter((session) => showArchivedSessions || !archivedTicketIds.has(session.ticketId));
  const contextCount = section === "sessions" ? `${visibleSessions.length} sesiones` : section === "repos" ? `${repositories.length} repositorios` : `${tickets.length} tickets guardados`;
  const ticketsForColumn = (status: Status) => tickets.filter((ticket) => ticket.status === status && (status !== "done" || showArchivedDone || !ticket.archived));
  const queueFor = (ticket: Ticket) => queueStates.find((item) => item.ticketId === ticket.id);
  return <main className="shell">
    <div className="window-drag-region" data-tauri-drag-region onMouseDown={(event) => { if (event.buttons === 1) void getCurrentWindow().startDragging(); }} />
    <div className="shell-body">
    <aside className={`rail ${railExpanded ? "expanded" : ""}`}><b className="brand-mark" aria-label="Palantir"><svg viewBox="0 0 32 32" aria-hidden="true"><path d="M16 2.5 27 9v14L16 29.5 5 23V9z"/><path d="M16 8.5c4.1 0 7.5 3.3 7.5 7.5s-3.4 7.5-7.5 7.5S8.5 20.1 8.5 16 11.9 8.5 16 8.5Z"/><circle cx="16" cy="16" r="2.4"/></svg><span>Palantir</span></b><button className={section === "board" ? "active" : ""} onClick={() => selectSection("board")} aria-label="Tablero" title="Tablero" aria-pressed={section === "board"}><NavIcon kind="board" /><small>Tablero</small></button><button className={section === "sessions" ? "active" : ""} onClick={() => selectSection("sessions")} aria-label="Sesiones" title="Sesiones" aria-pressed={section === "sessions"}><NavIcon kind="sessions" /><small>Sesiones</small></button><button className={section === "repos" ? "active" : ""} onClick={() => selectSection("repos")} aria-label="Repositorios" title="Repositorios" aria-pressed={section === "repos"}><NavIcon kind="repos" /><small>Repos</small></button><button className="settings-nav" onClick={() => setSettingsOpen(true)} aria-label="Configuración" title="Configuración"><NavIcon kind="settings" /><small>Configuración</small></button></aside>
    <section className="workspace">
      <header><div><p>PALANTIR · {isRemoteProfile(preferences) ? "REMOTE AGENT CONTROL" : "LOCAL AGENT CONTROL"}</p><h1>{pageTitle}</h1></div>{section === "repos" ? <button className="primary" onClick={() => openRepositoryModal()}>+ Nuevo repositorio</button> : <button className="primary" onClick={() => { setEditingId(null); setDraft(emptyDraft); setModalOpen(true); }}>+ Nuevo ticket</button>}</header>
      {notice && <div className="notice" role="status">{notice}</div>}
      {section === "board" && <div className="board">{columns.map(([status, title]) => <section className="column" key={status}>
        <h2>{title}{status === "done" && <button className={`archive-filter ${showArchivedDone ? "active" : ""}`} onClick={() => setShowArchivedDone((current) => !current)} aria-pressed={showArchivedDone}>{showArchivedDone ? "Ocultar archivadas" : "Ver archivadas"}</button>}<small>{ticketsForColumn(status).length}</small></h2>
        <div className="column-body">{ticketsForColumn(status).map((ticket) => <button className={`card ${selected?.id === ticket.id ? "selected" : ""} ${ticket.archived ? "archived" : ""}`} key={ticket.id} onClick={() => setSelected(ticket)}>
          <div><span>{ticket.id.slice(0, 8)}</span><strong>{ticket.agent}</strong></div>{ticket.kind === "planning" && <em>PLAN</em>}{ticket.archived && <em className="archived-label">ARCHIVADO</em>}{status === "todo" && queueFor(ticket) && <em className={queueFor(ticket)?.retryRequired ? "queue-blocked" : "queue-label"}>{queueFor(ticket)?.retryRequired ? "REINTENTO MANUAL" : `EN COLA · #${queueFor(ticket)?.position}`}</em>}<h3>{ticket.title}</h3><p>{queueFor(ticket)?.lastError || ticket.description || "Sin descripción"}</p><footer>{ticket.model} · {ticket.reasoning}</footer>
        </button>)}</div>
      </section>)}</div>}
      {section === "sessions" && <section className="sessions-view"><div className="sessions-toolbar"><span>Las sesiones de tickets archivados se ocultan para mantener el historial operativo a la vista.</span><button className={`archive-filter ${showArchivedSessions ? "active" : ""}`} onClick={() => setShowArchivedSessions((current) => !current)} aria-pressed={showArchivedSessions}>{showArchivedSessions ? "Ocultar archivadas" : "Ver archivadas"}</button></div><div className="content-list">{visibleSessions.length ? visibleSessions.map((session) => <article className={archivedTicketIds.has(session.ticketId) ? "archived-session" : ""} key={session.id}><small>{archivedTicketIds.has(session.ticketId) ? "ARCHIVADA · " : ""}{session.status} · {session.agent}</small><h2>{session.ticketTitle}</h2><time>{new Date(session.startedAt).toLocaleString()}</time><pre>{session.summary ?? "Sin salida todavía"}</pre></article>) : <p>{sessions.length ? "No hay sesiones activas. Usá “Ver archivadas” para consultar el historial." : "No hay sesiones registradas todavía."}</p>}</div></section>}
      {section === "repos" && <section className="repo-catalog">{repositories.length ? repositories.map((repository) => {
        const ticketCount = tickets.filter((ticket) => ticket.workspace === repository.path).length;
        return <article className="repo-card" key={repository.id}><div className="repo-card-top"><span className="repo-glyph" aria-hidden="true">⌘</span><div><small>WORKSPACE LOCAL</small><h2>{repository.name}</h2></div><div className="repo-card-actions"><button className="repo-edit" onClick={() => openRepositoryModal(repository)}>Editar<span aria-hidden="true">↗</span></button><button className={`repo-delete ${ticketCount > 0 ? "blocked" : ""}`} onClick={() => void deleteRepository(repository, ticketCount)} title={ticketCount > 0 ? "Tiene tickets asociados: no se puede borrar." : "Borrar repositorio"}>Borrar</button></div></div><p>{repository.description || "Sin descripción. Agregá una nota para reconocer este workspace más rápido."}</p><code title={repository.path}>{repository.path}</code><footer><span><b>{ticketCount}</b> {ticketCount === 1 ? "ticket asociado" : "tickets asociados"}</span><span>Disponible para nuevos tickets</span></footer></article>;
      }) : <div className="repo-empty"><span className="repo-glyph" aria-hidden="true">⌘</span><h2>Todavía no hay repositorios</h2><p>Guardá una carpeta local para usarla como contexto de tus tickets.</p><button className="primary" onClick={() => openRepositoryModal()}>Registrar primer repositorio</button></div>}</section>}
    </section>
    </div>
    <footer className="statusbar">
      <span className="statusbar-item">{contextLabel}</span>
      <span className="statusbar-item">{contextCount}</span>
      {queueStates.length > 0 && <span className="statusbar-item">{queueStates.length} en cola</span>}
      <span className="statusbar-item statusbar-right">{preferences.executionProfile === "remote" ? "Remoto" : "Local"}</span>
    </footer>
    {selected && <div className="modal-backdrop detail-backdrop" onMouseDown={() => setSelected(null)}><aside className="review" role="dialog" aria-modal="true" aria-label={`Detalle de ${selected.title}`} onMouseDown={(event) => event.stopPropagation()}>
      <button className="x" onClick={() => { setSelected(null); setActionError(""); }} aria-label="Cerrar detalle">×</button><p>{selected.kind === "planning" ? "TICKET DE PLANIFICACIÓN" : "TAREA SIMPLE"}</p><h2>{selected.title}</h2>
      <div className="ticket-actions">{selected.status !== "running" && <button onClick={() => { setDraft({ title: selected.title, description: selected.description, kind: selected.kind, agent: selected.agent, model: selected.model, reasoning: selected.reasoning, workspace: selected.workspace, repositoryId: repositoryFor(selected)?.id ?? "" }); setEditingId(selected.id); setSelected(null); setModalOpen(true); }}>Editar</button>}{["backlog", "todo"].includes(selected.status) && <button className="danger" onClick={() => void remove()}>Borrar</button>}{selected.status === "todo" && queueFor(selected)?.retryRequired && <button className="secondary" onClick={() => void retry()}>Reintentar</button>}{selected.status === "running" && lastSession?.status === "running" && <button onClick={() => void stop()}>Detener agente</button>}{selected.status === "done" && <button className={selected.archived ? "secondary" : "archive-action"} onClick={() => void archive()}>{selected.archived ? "Restaurar de archivo" : "Archivar ticket"}</button>}{availableActions(selected).map(([status, label]) => <button key={status} onClick={() => void move(status)}>{label}</button>)}</div>
      {actionError && <p className="action-error" role="alert">{actionError}</p>}
      <article className="execution-detail"><h3>Descripción</h3><p>{selected.description || "Sin descripción."}</p><h3>Configuración</h3><dl><dt>Estado</dt><dd>{statusName(selected.status)}</dd><dt>Agente</dt><dd>{selected.agent}</dd><dt>Modelo</dt><dd>{selected.model}</dd><dt>Razonamiento</dt><dd>{selected.reasoning}</dd><dt>Repositorio</dt><dd>{repositoryFor(selected)?.name ?? "Repositorio histórico"}</dd><dt>Carpeta</dt><dd>{selected.workspace}</dd><dt>Rama</dt><dd>{selected.branch ?? "Se crea al pasar a To do"}</dd><dt>Commit base</dt><dd>{lastSession?.baseCommit ?? "Sin commit base"}</dd></dl><h3>Informe de ejecución · {lastSession?.status ?? "sin ejecutar"}</h3>{reviewSections(lastSession?.summary).map(([label, content]) => <section className="report-section" key={label}><h4>{label}</h4><p>{content}</p></section>)}<h3>Cambios detectados</h3><pre>{lastSession?.changesDetected || "No se detectaron cambios."}</pre></article>
      {selected.status === "review" && <section className="follow-up"><h3>Seguimiento</h3><textarea value={followUp} onChange={(event) => setFollowUp(event.target.value)} placeholder="Pedí una corrección, una validación o una mejora…" /><button className="primary" onClick={() => void sendFollowUp()}>Enviar a {agentLabels[selected.agent]}</button></section>}
      {selected.kind === "planning" && <section className="follow-up"><h3>Plan propuesto</h3>{plan ? <>{plan.items.map((item, index) => <p key={`${item.title}-${index}`}><strong>{index + 1}. {item.title}</strong><br />{item.description}</p>)}{plan.approvedAt ? <><h4>Sub-kanban · {subtickets.filter((ticket) => ticket.status === "done").length}/{subtickets.length} completadas</h4>{subtickets.map((ticket) => <button className="secondary" key={ticket.id} onClick={() => setSelected(ticket)}>{statusName(ticket.status)} · {ticket.title}</button>)}</> : selected.status === "review" && <button className="primary" onClick={() => void approvePlan()}>Aprobar y crear sub-kanban</button>}</> : <p>El plan aparecerá aquí cuando finalice la ejecución.</p>}</section>}
      {selected.status === "todo" && <p className="action-hint">{queueFor(selected)?.retryRequired ? "La última ejecución falló. Revisá el informe y reintentá manualmente." : `Está esperando en la cola${queueFor(selected) ? ` · posición ${queueFor(selected)?.position}` : ""}.`}</p>}{selected.status === "running" && <p className="action-hint">El agente está trabajando. Al finalizar, este ticket pasa a Review automáticamente.</p>}{selected.status === "done" && <p className="action-hint">La rama y el historial permanecen disponibles. Palantir no hace merge ni push.</p>}
    </aside></div>}
    {modalOpen && <div className="modal-backdrop" onMouseDown={() => setModalOpen(false)}><form className="ticket-modal" onSubmit={create} onMouseDown={(event) => event.stopPropagation()}>
      <button type="button" className="x" onClick={() => { setModalOpen(false); setEditingId(null); }} aria-label="Cerrar formulario">×</button><p>{editingId ? "EDITAR TICKET" : "NUEVO TICKET"}</p><h2>{editingId ? "Actualizar configuración" : "Definí el trabajo"}</h2>
      <label>Título<input autoFocus value={draft.title} onChange={field("title")} required placeholder="Qué querés resolver" /></label><label>Descripción<textarea value={draft.description} onChange={field("description")} placeholder="Contexto, resultado esperado y restricciones" /></label>
      <div className="form-grid"><label>Tipo<select value={draft.kind} onChange={field("kind")}><option value="task">Tarea simple</option>{!isRemoteProfile(preferences) && <option value="planning">Planificar sub-kanban</option>}</select></label><label>Agente<div className="agent-picker" role="radiogroup" aria-label="Agente">{enabledAgents.map((agent) => <button type="button" key={agent} className={`agent-option ${draft.agent === agent ? "active" : ""}`} role="radio" aria-checked={draft.agent === agent} onClick={() => selectAgent(agent)}><AgentIcon agent={agent} />{agentLabels[agent]}</button>)}</div></label><label>Modelo<select value={draft.model} onChange={field("model")}>{modelOptions.map(([model, label]) => <option key={model} value={model}>{label}</option>)}</select></label><label>Razonamiento<select value={draft.reasoning} onChange={field("reasoning")}>{preferences.allowedReasoning.map((reasoning) => <option key={reasoning}>{reasoning}</option>)}</select></label></div>
      <label>Repositorio<select value={draft.repositoryId} onChange={field("repositoryId")} required><option value="">Seleccioná un repositorio</option>{repositories.map((repository) => <option key={repository.id} value={repository.id}>{repository.name} · {repository.path}</option>)}</select></label>{!repositories.length && <p className="action-error">Primero registrá un repositorio en la sección Repos.</p>}<footer><span>{editingId ? "Los cambios se aplican al ticket; la configuración de ejecuciones previas se conserva." : "Se creará en Backlog; moverlo a To do prepara Git y lo agrega a la cola."}</span><button className="primary">{editingId ? "Guardar cambios" : "Crear en Backlog"}</button></footer>
    </form></div>}
    {settingsOpen && <div className="modal-backdrop" onMouseDown={() => setSettingsOpen(false)}><form className="ticket-modal settings-modal" onSubmit={savePreferences} onMouseDown={(event) => event.stopPropagation()}><button type="button" className="x" onClick={() => setSettingsOpen(false)} aria-label="Cerrar configuración">×</button><p>CONFIGURACIÓN</p><h2>Ejecución y perfiles</h2><label>Límite de tareas concurrentes<input type="number" min="1" value={preferences.maxConcurrentTasks} onChange={(event) => setPreferences({ ...preferences, maxConcurrentTasks: Math.max(1, Number(event.target.value)) })} /></label><p className="settings-copy">Los tickets en To do se ejecutan en orden de cola hasta este límite.</p><div className="form-grid"><label>Agentes habilitados<div className="agent-picker" role="group" aria-label="Agentes habilitados">{(Object.keys(agentLabels) as Ticket["agent"][]).map((agent) => <button type="button" key={agent} className={`agent-option ${preferences.enabledAgents.includes(agent) ? "active" : ""}`} aria-pressed={preferences.enabledAgents.includes(agent)} onClick={() => toggleEnabledAgent(agent)}><AgentIcon agent={agent} />{agentLabels[agent]}</button>)}</div></label><label>Perfil<select value={preferences.executionProfile} onChange={(event) => setPreferences({ ...preferences, executionProfile: event.target.value as Preferences["executionProfile"] })}><option value="local">Local</option><option value="remote">Remoto</option></select></label></div><label>Modelos permitidos<select multiple value={preferences.allowedModels} onChange={(event) => setPreferences({ ...preferences, allowedModels: Array.from(event.target.selectedOptions, (option) => option.value) })}>{allModels.map(([model, label]) => <option key={model} value={model}>{label}</option>)}</select></label><label>Razonamiento permitido<select multiple value={preferences.allowedReasoning} onChange={(event) => setPreferences({ ...preferences, allowedReasoning: Array.from(event.target.selectedOptions, (option) => option.value) })}>{["low", "medium", "high", "xhigh", "max"].map((value) => <option key={value}>{value}</option>)}</select></label>{preferences.executionProfile === "remote" && <><label>Endpoint remoto<input value={preferences.remoteEndpoint} onChange={(event) => setPreferences({ ...preferences, remoteEndpoint: event.target.value })} placeholder="http://100.x.x.x:8787" /></label><label>Token del servidor remoto<input type="password" value={preferences.remoteToken} onChange={(event) => setPreferences({ ...preferences, remoteToken: event.target.value })} placeholder="Bearer token de palantir-server" /></label><div className="remote-test"><button type="button" className="secondary" onClick={() => void testRemoteConnection()}>Probar conexión</button>{remoteTestStatus && <span>{remoteTestStatus}</span>}</div><p className="remote-notice">Con este perfil activo, el tablero, repos, cola y sesiones operan contra esta instancia de <code>palantir-server</code> en vez de tu base local. Todavía no soporta planificación/sub-kanban, archivado, reintentar, detener ni seguimiento — esas acciones no están disponibles mientras el perfil sea Remoto.</p></>}<footer><span>La política se valida también en el backend.</span><button className="primary">Guardar configuración</button></footer></form></div>}
    {repositoryModalOpen && <div className="modal-backdrop" onMouseDown={closeRepositoryModal}><form className="ticket-modal repository-modal" onSubmit={createRepository} onMouseDown={(event) => event.stopPropagation()}>
      <button type="button" className="x" onClick={closeRepositoryModal} aria-label="Cerrar formulario">×</button><p>{editingRepositoryId ? "EDITAR REPOSITORIO" : "NUEVO REPOSITORIO"}</p><h2>{editingRepositoryId ? "Actualizar workspace" : "Registrar un workspace"}</h2><p className="repository-modal-copy">Este repositorio aparecerá como opción al crear tickets. La ruta debe ser única.</p>
      <label>Nombre<input autoFocus value={repositoryDraft.name} onChange={(event) => setRepositoryDraft({ ...repositoryDraft, name: event.target.value })} required placeholder="Ej. Palantir" /></label><label>Descripción<textarea value={repositoryDraft.description} onChange={(event) => setRepositoryDraft({ ...repositoryDraft, description: event.target.value })} placeholder="Qué contiene o para qué lo usás" /></label><label>Ruta local<input value={repositoryDraft.path} onChange={(event) => setRepositoryDraft({ ...repositoryDraft, path: event.target.value })} required placeholder="/Users/tu-usuario/proyecto" /></label><footer><span>{editingRepositoryId ? "Los tickets existentes conservarán esta referencia." : "Después podés seleccionar este repositorio en cada ticket."}</span><div><button className="secondary" type="button" onClick={closeRepositoryModal}>Cancelar</button><button className="primary">{editingRepositoryId ? "Guardar cambios" : "Guardar repositorio"}</button></div></footer>
    </form></div>}
  </main>;
}
