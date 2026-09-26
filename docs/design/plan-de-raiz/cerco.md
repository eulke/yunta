# El cerco — M25

Lo que una sesión puede escribir, dicho una sola vez, juzgado por una sola
función, construido por cada adapter con el mecanismo que su CLI tiene, y
reportado en el log con la cobertura real. Reemplaza cuatro formas de decir lo
mismo: `edit_constraints` (globs como `String`), `edit_hooks: bool`,
`artifact_dir` leído como permiso, y los marcadores `blocked:<path>` del mock.
El post-check por diff (`scope_check`, se conserva; su archivo gana una
función pura) sigue siendo la única garantía; el cerco es lo que evita que una
escritura fuera de scope llegue a existir, y lo dice cuando no pudo evitarla.

Decisión: D172 (revisa D13 y D167). Ítem: 3-08. Defectos: AD-D2, AD-D7,
AD-D20, AD-D24, DO-D2; deuda A-13.

---

## 1. Vocabulario

| término | qué es | dónde vive |
|---|---|---|
| **cerco** (`Fence`) | los globs que una sesión puede escribir bajo el worktree (los declarados más las ampliaciones ya concedidas) y las raíces absolutas fuera de él | `yunta_core::fence` |
| **nivel** (`FenceLevel`) | lo que un adapter sabe construir: nada, un juicio por llamada de herramienta, o un sandbox de filesystem | `Capabilities::fence` |
| **cobertura** (`Coverage`) | lo que la sesión concreta cercó, calculado de lo que el adapter construyó: exacta, ensanchada a raíces, o solo herramientas | `agent_session_opened.fence` |
| **canal** | por dónde una sesión escribe: las herramientas de archivo del CLI, o todo lo demás (shell, MCP, tareas delegadas del CLI). Es prosa: el tipo `Channel` del código es el de los gates | `Fenced` y `Coverage::of` |
| **cercado** (`Fenced`) | lo que un canal tiene: cerco exacto, o raíces | `Coverage::of` |
| **juez** | la función pura que decide si un path está dentro del cerco | `Fence::judge` |
| **hook** (`FenceHook`) | el comando que un CLI ejecuta antes de escribir y que invoca al juez | `core::fence`, construido por el CLI |
| **codec** | la traducción entre el juez y el hook de un CLI: qué llega por stdin, qué se responde | `FenceCodec` por adapter |
| **rechazo** (`write_refused`) | el hecho de que una escritura fue rechazada antes de ocurrir | kind del dominio `session` |

En prosa: cerco, nivel, cobertura, canal, cercado, juez, hook, codec,
rechazo. En YAML, JSON y código: `fence`, `FenceLevel`, `Coverage`, `Fenced`,
`Fence::judge`, `FenceHook`, `FenceCodec`, `write_refused`.

---

## 2. Tipos

```rust
// crates/core/src/capabilities.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Capabilities {
    pub resume_session: bool,
    pub fence: FenceLevel,             // reemplaza `edit_hooks: bool`; un log viejo con `edit_hooks` lee `None`
    pub permission_profiles: bool,
    pub custom_agents: bool,
    pub usage_reporting: bool,
    pub skills: bool,
    pub run_tools: bool,
    pub network_isolation: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FenceLevel { #[default] None, ToolCalls, Filesystem }      // "none" | "tool_calls" | "filesystem"
pub enum Capability { ResumeSession, Fence, PermissionProfiles, CustomAgents, UsageReporting, Skills, RunTools, NetworkIsolation }
impl Capability { pub fn as_str(&self) -> &'static str /* Fence => "fence" */ }
impl Capabilities { pub fn declares(&self, cap: Capability) -> bool /* Fence => self.fence != FenceLevel::None */ }

// crates/core/src/fence.rs — nuevo; el único juez del workspace
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fence { pub allowed: Vec<ScopeGlob>, pub roots: Vec<PathBuf>, pub advice: Advice }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Advice { RequestExpansion, ReportFinding }   // qué le dice el rechazo al modelo: pedir ampliación (la tool está montada) o reportar un finding
impl Fence {
    pub fn everything(roots: Vec<PathBuf>, advice: Advice) -> Self;          // allowed = ["**"]
    pub fn read_only(roots: Vec<PathBuf>, advice: Advice) -> Self;           // allowed = []
    pub fn for_session(profile: PermissionProfile, scope: Option<&[ScopeGlob]>, granted: &[ScopeGlob], artifact_dir: Option<&Path>, advice: Advice) -> Self;
        // ReadOnly → read_only; scope Some → allowed = scope ∪ granted; None → everything; roots = artifact_dir.into_iter().collect()
    pub fn judge(&self, worktree: &Path, target: &Path) -> Verdict;   // pura: no toca disco
    pub fn to_env(&self, worktree: &Path) -> (&'static str, String);  // (ENV_VAR, json de FenceEnv { fence, worktree })
    pub fn from_env(value: &str) -> Result<(Self, PathBuf), FenceEnvError>;
}
pub const ENV_VAR: &str = "YUNTA_FENCE";
pub enum FenceEnvError { Json(#[source] serde_json::Error) }
pub enum Verdict { Allowed, Refused(Refusal) }
pub struct Refusal { pub target: PathBuf, pub worktree: PathBuf, pub allowed: Vec<ScopeGlob>, pub roots: Vec<PathBuf>, pub advice: Advice }
impl Display for Refusal;                      // la única prosa del rechazo; empieza por REFUSAL_MARKER
pub const REFUSAL_MARKER: &str = "yunta: write refused: ";
pub fn refused_target(text: &str) -> Option<PathBuf>;   // reconoce un Display de Refusal; único parser del marcador
pub fn lexical_absolute(base: &Path, path: &Path) -> PathBuf;   // resuelve `.`/`..` sin disco; relativo → contra base

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fenced { Exact, Roots(Vec<PathBuf>) }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "coverage", rename_all = "snake_case")]
pub enum Coverage { Exact, WidenedToRoots { roots: Vec<PathBuf> }, ToolsOnly }
// wire, dentro de `agent_session_opened`: "fence": {"coverage": "exact"} · {"coverage": "widened_to_roots", "roots": [...]} · {"coverage": "tools_only"}
impl Coverage {
    pub fn of(tools: Fenced, others: Option<Fenced>) -> Coverage;
    // (Exact, Some(Exact)) → Exact · (_, None) → ToolsOnly · resto → WidenedToRoots { roots: unión ordenada, sin repetidos }
}

pub const SUBCOMMAND: &str = "fence";          // el nombre del subcomando vive acá; el CLI lo registra, los adapters lo embeben
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FenceHook { bin: PathBuf }
impl FenceHook {
    pub fn new(bin: PathBuf) -> Self;                          // el CLI: `std::env::current_exe()` una vez, en `Context`
    pub fn command(&self, adapter: &AdapterId) -> Vec<String>; // ["<bin>", "fence", "<adapter-id>"]
}
```

**Reglas del juez** (`Fence::judge`, en este orden; la primera que decide, decide):

1. `target` se normaliza con `lexical_absolute(worktree, target)`: un path
   relativo se resuelve contra el worktree; `.` y `..` se colapsan sin leer el
   disco. Un CLI que entrega paths relativos los entrega relativos a su cwd, que
   es el worktree.
2. Bajo alguna `roots[i]` → `Allowed`.
3. Bajo `worktree`: si el path relativo es `.git` o empieza por `.git/` →
   `Refused` (el registro del run nunca es trabajo de una tarea; `.git` a
   secas es el archivo que un `git worktree` deja); si
   `scope_globset(allowed)` lo matchea → `Allowed`; si no → `Refused`.
4. Fuera del worktree y de toda raíz → `Refused`.

`allowed` vacío deja toda escritura bajo el worktree en `Refused`: ese es el
perfil `read_only`, y las raíces siguen escribibles porque son el registro,
no el trabajo.

**El texto del rechazo** (`Refusal`'s `Display`), una sola vez, para el
modelo y para el parser. Las listas vacías se imprimen como `none`:

```
yunta: write refused: <target> is outside this session's scope.
Allowed under <worktree>: <globs separados por ", " | none>; also writable: <roots separados por ", " | none>.
<consejo>
```

Con `Advice::RequestExpansion`: `Ask for more scope with the run tool
yunta_request_scope_expansion; a granted expansion applies from the next
attempt. Do not write here.` Con `Advice::ReportFinding`: `Report the need as
a finding; do not write here.` `refused_target` lee el path entre
`REFUSAL_MARKER` y ` is outside`; un texto sin el marcador devuelve `None`.

**Las ampliaciones concedidas entran al cerco.** `allowed` es el scope
declarado más lo ya concedido en intentos anteriores (`already_granted_paths`
de `task_cycle/attempt.rs:32`), lo mismo que el post-check evalúa (Contrato
§6.2). Una ampliación pedida durante el intento se evalúa después de la
sesión, como hoy (`attempt.rs:94`); si se concede, el intento siguiente la
tiene en su cerco. El flujo pedir→escribir en el mismo intento no existe con
un cerco: la escritura se rechaza, el pedido queda registrado, y el reintento
la hace.

**La cobertura es la del canal más débil.** Un CLI puede cercar sus
herramientas de archivo por glob y su shell solo por sandbox de raíces, o no
cercarlo. `Coverage::of(tools, others)` toma lo que el adapter construyó en
cada canal y devuelve lo más débil; `Exact` solo cuando los dos canales son
exactos. Con un solo valor el engine decide lo único que decide con él: una
escritura que llega al diff bajo `Exact` es un `engine_finding` (§5).

---

## 3. Puerto y eventos

```rust
// crates/core/src/port/mod.rs (M01)
pub struct SessionRequest {
    pub prompt: String, pub cwd: PathBuf, pub model: Option<ModelName>, pub agent: Option<AgentName>,
    pub permissions: PermissionProfile,
    pub env: HashMap<String, Secret<String>>,
    pub fence: Fence,                       // siempre; reemplaza `edit_constraints: Option<Vec<Glob>>`; la única fuente de permisos de escritura
    pub fence_hook: Option<FenceHook>,      // None solo en un arnés sin binario (Bench); un adapter que lo necesita y no lo tiene falla la sesión
    pub budget: Budget,
    pub adapter_settings: serde_json::Map<String, serde_json::Value>,
    pub skills: Vec<PathBuf>,
    pub run_tools_endpoint: Option<RunToolsEndpoint>,
    pub artifact_dir: Option<PathBuf>,      // dónde van los archivos declarados (prompt y mount); ningún adapter lo lee como permiso
    pub scratch_dir: PathBuf,               // siempre; deja de ser Option
}
pub enum AgentEvent {
    SessionOpened { session_id: SessionId, model: Option<ModelName>, fence: Option<Coverage> },   // None cuando el nivel es None
    WriteRefused { target: ToolTarget },
    RunToolsMounted { count: usize }, ToolUse { target: ToolTarget /* M11 */ }, Usage { … }, Note { … }, Completed { … }, Failed { … },
}
pub enum AdapterError {
    /* como hoy */
    FenceUnbuildable(Unbuildable),          // antes de spawn; una capacidad ausente falla
}
pub enum Unbuildable { HookUnavailable, SealedRoots(Vec<PathBuf>) }   // Display en el borde: "this adapter cannot run its fence hook: no yunta binary was handed to the session" / "this adapter cannot keep <roots> writable under a read-only profile; hand the files over through the run tools or raise the profile to `edit`"
pub trait Adapter: Send + Sync {
    /* como hoy */
    fn fence_codec(&self) -> Option<&dyn FenceCodec> { None }   // Some solo si `capabilities().fence == ToolCalls`
}
pub trait FenceCodec: Send + Sync {
    fn decode(&self, stdin: &[u8]) -> Result<Option<PathBuf>, CodecError>;   // None: la llamada no escribe un path (se permite)
    fn encode(&self, verdict: &Verdict) -> HookReply;
}
pub struct HookReply { pub stdout: Vec<u8>, pub stderr: Vec<u8>, pub exit: i32 }
pub enum CodecError { Json(#[source] serde_json::Error), MissingField(&'static str) }
pub type Glob = …;   // borrado

// crates/core/src/events/session/kinds.rs (M02)
pub enum SessionEvent { Opened(AgentSessionOpenedPayload), Message(AgentMessagePayload), Degraded(CapabilityDegradedPayload), WriteRefused(WriteRefusedPayload) }
// KINDS = ["agent_session_opened", "agent_message", "capability_degraded", "write_refused"]; is_audit: write_refused = false (mueve NodeLedger, D175)
// crates/core/src/events/session/payloads.rs (M03)
pub struct AgentSessionOpenedPayload { pub session_id: SessionId, pub agent: Option<AgentName>, pub model: Option<ModelName>, pub capabilities: Capabilities, #[serde(default, skip_serializing_if = "Option::is_none")] pub fence: Option<Coverage> }
impl AgentSessionOpenedPayload { pub fn new(session_id: SessionId, agent: Option<AgentName>, model: Option<ModelName>, capabilities: Capabilities, fence: Option<Coverage>) -> Self; }
// el nivel viaja una vez, en `capabilities.fence`; `fence` es solo la cobertura
pub struct WriteRefusedPayload { pub session_id: SessionId, pub target: ToolTarget }
impl WriteRefusedPayload { pub fn new(session_id: SessionId, target: ToolTarget) -> Self; }
// crates/core/src/events/node/ledger.rs (M04, D175): NodeLedger es el dueño de las sesiones, porque el intento las acota
// OpenSession.fence: Option<Coverage>; NodeRecord.refused: Vec<RefusedWrite { session_id, target }>
impl NodeLedger { pub fn coverage_of(&self, node: &NodeId, session: &SessionId) -> Option<&Coverage>; pub fn refused_by(&self, node: &NodeId, session: &SessionId) -> Vec<&ToolTarget>; }
// crates/core/src/port/policy.rs (M09): la fila
(Capability::Fence, Absence::DegradeWith(Policy::PostCheckOnly)),   // una vez por run
```

`EventPayload::KINDS` pasa a 37 y `crates/core/schemas/events.json` se
regenera con `cargo xtask schema`: sin tag (README §1) el reemplazo es en el
lugar, con `schema_version` 1 (D141); el invariante "ni un byte" es del ítem
2-01, no una congelación. Los tests de M02 que derivan de `KINDS`
(`kind_names_match_the_spec_exactly`, `…every_kind`) pasan solos; spec-events
§0 (conteo) y §5.5 (`fence`) cambian, y gana §5.x `write_refused`.
`Capabilities` serializa `fence` como string y lee por `#[serde(default)]`,
así que un log con `edit_hooks` sigue leyéndose (`fence = None`); los
fixtures y el corpus que escriben `edit_hooks` se reescriben.

---

## 4. El comando `yunta fence <adapter-id>`

Oculto en `--help`; su nombre es `fence::SUBCOMMAND`. Es el hook: el CLI del
agente lo ejecuta antes de cada escritura con la llamada en stdin y
`YUNTA_FENCE` en el entorno.

```rust
// crates/cli/src/commands/fence.rs
pub fn run(codec: Option<&dyn FenceCodec>, fence_var: Option<&str>, stdin: &[u8]) -> HookReply;
// 1. codec None → HookReply { exit: 2, stderr: "yunta: fence misconfigured: adapter `<id>` has no fence codec" }
// 2. fence_var None / Fence::from_env error → exit 2, stderr "yunta: fence misconfigured: <causa>"
// 3. codec.decode(stdin) → error → exit 2 "yunta: fence misconfigured: <causa>"; None → codec.encode(&Verdict::Allowed)
// 4. codec.encode(&fence.judge(&worktree, &target))
```

Un fallo nuestro rechaza; nunca permite. `main.rs` resuelve el codec en el
registro de adapters de 1-04 (`registry.iter().find(|a| a.id() == id)`,
construido sin `probe()`) y lee `YUNTA_FENCE` en el borde, por
`config/env.rs` (`Env::fence_var()`; el ratchet `env_read_outside_boundary`
de M22 no cambia). El cerco viaja por `YUNTA_FENCE` en el entorno del hijo
(no es secreto: son globs y paths), puesto por el adapter en `spawn` con
`Fence::to_env`. `FenceHook` lo construye `Context` una sola vez con
`std::env::current_exe()` (hoy `commands/mod.rs:120` lo repite para
`--detach`; pasa a consumir el mismo valor) y llega al engine por
`RunEnv.fence_hook: Option<FenceHook>`. `Bench` pasa `None`: el mock juzga en
proceso y no lo necesita; `yunta test` y `drive` reciben el de `Context`.

---

## 5. El engine

- **`open_session` (M08)** construye el cerco en un solo lugar:
  `require(Capability::Fence)` (degrada `PostCheckOnly` una vez por run si el
  nivel es `None`); `advice = RequestExpansion` si la sesión monta
  `yunta_request_scope_expansion` (una sesión de tarea con run tools,
  Contrato §6.4), `ReportFinding` si no; `fence = Fence::for_session(profile,
  scope, granted, artifact_dir, advice)` con `scope` el de la tarea
  (`Task.scope`) o el del nodo (`Node.scope`), ya `Vec<ScopeGlob>` (M12), y
  `granted` las ampliaciones ya concedidas a esa tarea (`GrantLedger`);
  `fence_hook = ctx.env.fence_hook.clone()`; `scratch_dir` lo crea
  `SessionSlot` siempre.
- **Cobertura en memoria.** `dispatch_session` (`task_cycle/session.rs:150`)
  emite `agent_session_opened` con `fence` desde `AgentEvent::SessionOpened`
  (`session.rs:290-302`, gana el campo) y lo devuelve en
  `DispatchOutcome.fence: Option<Coverage>`: la cache de una invocación de lo
  que el log ya tiene.
- **Post-check.** `scope_check` no cambia. Después de cada llamada
  (`node_close.rs:212`, `task_cycle/attempt.rs:112`, `loop_exec/integrate.rs:278`)
  corre `fence_breach(coverage: Option<&Coverage>, result: &ScopeCheckResult) -> Option<Breach>`
  (pura, `engine/src/scope.rs`): `Some(Breach { paths: result.violations.clone() })`
  solo con `Coverage::Exact` y `violations` no vacías. `attempt.rs` no tiene
  `RunCtx`: devuelve el `Breach` dentro de su resultado y lo registra el
  llamador que sí lo tiene, el mismo que registra `scope_checked`. El
  registro es el `engine_finding` que ya existe (`ctx.rs:190`) con
  `node = Some(nodo)`, `id = "engine-fence-breach"` (el esquema `engine-*` de
  `exec.rs:236,280`), `severity = FindingSeverity::Major` (la tarea ya falla
  por la violación; el finding pide revisar el adapter, no bloquea más que
  esa falla), `title = "the fence declared exact let a write through"`,
  `location = <primer path>`, `detail = "fence exact on <adapter>; <n> paths reached the diff outside it: <paths separados por ", ">"`,
  además de la falla que ya existe. Bajo `WidenedToRoots` o `ToolsOnly` una
  violación es lo que siempre fue: la tarea falla con la lista.
- **Nada más decide con el cerco.** `decide()` (M07) no lo mira; los
  `write_refused` son crónica y conteo, no estado de nodo.
- **Superficie.** Hasta 5-05, `cli/src/surface/lines.rs` gana la línea de
  `write_refused` (`<nodo> — write refused: <display>`) y el sufijo de
  `agent_session_opened` (`· fence exact` / `· fence widened to <n> roots` /
  `· fence on tool calls`; nada sin `fence`). En 5-05 la crónica los dispone
  desde `NodeLedger::coverage_of` y `NodeRecord.refused`; las filas ya están
  en `cronica.md`, y son de 5-05, no de este ítem.
- **`check` (M09).** Sin cambio: `Fence` degrada, no falla en `check`.

---

## 6. Adapters builtin

Regla común: ningún adapter lee `artifact_dir` para permisos; `--add-dir`,
`writable_roots` y todo equivalente salen de `fence.roots`. La configuración
de sesión va bajo `scratch_dir`; nada bajo `cwd`.

### claude-code — `FenceLevel::ToolCalls`

- **Instala.** `--add-dir <root>` por cada `fence.roots`;
  `--settings '<json>'` inline con
  `{"hooks":{"PreToolUse":[{"matcher":"Edit|Write|MultiEdit|NotebookEdit","hooks":[{"type":"command","command":"<fence_hook.command(id) unido por espacios, cada parte entre comillas>","timeout":10}]}]}}`;
  `YUNTA_FENCE` en el entorno del hijo. Sin `fence_hook` →
  `AdapterError::FenceUnbuildable(HookUnavailable)` antes de spawn. Los
  `10` segundos de timeout los fija D172.
- **Perfiles.** `edit`: herramientas de lectura y escritura, sin `Bash`;
  `full`: todas; `read_only`: las de lectura, y `Write`/`Edit` solo cuando
  hay `artifact_dir` (los archivos declarados se escriben ahí y el cerco
  rechaza todo lo demás bajo el worktree). Cierra AD-D7 sin perder los
  archivos declarados; 3-07 toma esta forma.
- **Codec** (`adapters/src/claude_code/fence.rs`): stdin es el JSON del
  hook; `tool_input.file_path`, o `tool_input.notebook_path` para
  `NotebookEdit`; sin ninguno → `MissingField`. `Refused` → `exit 2`,
  `stderr = Refusal`'s `Display`; `Allowed` → `exit 0`, vacío.
- **Cobertura.** `tools = Exact`; `others`: `edit` y `read_only` no exponen
  `Bash` → `Some(Exact)`; `full` → `None`. Es decir: `edit`/`read_only`
  reportan `Exact`, `full` reporta `ToolsOnly`.
- **Parser** (`claude_code/parse.rs`): `ClaudeLine` gana el brazo
  `User(UserLine)` (hoy `parse.rs:22-33` descarta `type: "user"`); un
  `message.content[]` de `type: "tool_result"` con `is_error: true` cuyo
  `content` `refused_target` reconoce →
  `WriteRefused { target: ToolTarget::of_path(path relativo al worktree) }`.
  El parser recibe el worktree al construirse (`ClaudeParser::new(cwd)`). Qué
  parte del stderr del hook llega a `content` es lo que la verificación en
  vivo (A-17) confirma.
- **Límite declarado.** Si el hook no responde en el timeout, Claude Code
  deja pasar la llamada; el post-check la atrapa y `fence_breach` la nombra.

### codex — `FenceLevel::Filesystem`

- **Instala.** `--sandbox workspace-write` y
  `-c 'sandbox_workspace_write.writable_roots=[<fence.roots>]'` (por
  `ConfigOverride`, se conserva). Perfil `read_only` → `--sandbox read-only`;
  si además `fence.roots` no está vacío, el adapter falla antes de spawn con
  `AdapterError::FenceUnbuildable(SealedRoots(roots))`: el sandbox no puede
  dejar el worktree de solo lectura y las raíces escribibles a la vez, y una
  capacidad ausente falla en vez de gastar la sesión (cierra AD-D20; M09
  toma esta forma en vez de una degradación).
- **Cobertura.** `Coverage::of(Fenced::Roots(cwd ∪ roots), Some(Fenced::Roots(cwd ∪ roots)))`
  = `WidenedToRoots { roots: [cwd, …fence.roots] }`: el sandbox es por
  directorio en los dos canales. `.git` lo protege el sandbox mismo.
- **Parser** (`codex/parse.rs`): el ítem de proceso terminado con la marca de
  sandbox denegado → `WriteRefused { target: ToolTarget::opaque(command) }`.
- **Límite declarado.** La política partida del sandbox (`/repo=write`,
  `/repo/a=none`) no se usa: no expresa globs y el post-check ya cubre lo que
  ella cubriría.

### mock — nivel del fixture

`capabilities: { fence: none | tool_calls | filesystem }` (twin
`FixtureCapabilities.fence: FenceLevel`, M09) y `fence_coverage: exact |
widened | tools_only` (opcional; default `exact` para `tool_calls`, `widened`
para `filesystem`; `widened` reporta `roots = [cwd] ∪ fence.roots`). Cada
`MockEffect { path, content }` de `effects:` (`fixture.rs:227-230`) pasa por
`Fence::judge` con el cerco de la request en `apply_effects`
(`mock/mod.rs:146-167`): `Refused` → el mock emite `WriteRefused` y no
escribe; con nivel `filesystem` el juicio usa `Fence::everything(roots, …)`
(el sandbox no sabe de globs); con nivel `none` escribe siempre.
`is_blocked` (`mock/mod.rs:134-139`), `blocked_markers` y el
`target_digest: "blocked:<path>"` se borran.

---

## 7. Adapters futuros: la muestra del mercado

Relevado el 2026-09-13 sobre los ocho CLIs más usados después de Claude Code
y Codex (evidencia en `auditoria/09-mercado-de-clis.md`). La tabla dice qué
construiría cada adapter y qué reportaría; no está construido y no es deuda:
es la prueba de que los tipos de §2 alcanzan para cualquiera de ellos sin
una variante nueva.

| CLI | nivel | cómo llega el juez | cobertura `edit` / `full` | rechazo en el stream | inyección por sesión | raíces |
|---|---|---|---|---|---|---|
| Gemini CLI | ToolCalls | hook `BeforeTool` = `yunta fence gemini` en `settings.json` bajo `GEMINI_CLI_HOME=<scratch>`; `--policy` como segunda línea | Exact (`auto_edit` ya niega shell) / ToolsOnly | `tool_result.error.type = policy_violation` + marcador | `GEMINI_CLI_HOME`, `--policy`, `--include-directories` | `--include-directories` (≤5 con Seatbelt) |
| Copilot CLI | ToolCalls | hook `preToolUse` bajo `COPILOT_HOME=<scratch>`; `--allow-tool='write(<glob>)'` y `--excluded-tools=bash,…` como segunda línea | Exact / ToolsOnly (o `WidenedToRoots` con `--sandbox`) | `tool.execution_complete.error.code = denied` + marcador | flags; `COPILOT_HOME` | `--add-dir` |
| Cursor CLI | ToolCalls | hook `preToolUse` en `hooks.json` bajo `CURSOR_CONFIG_DIR=<scratch>` (si no lo reubica: bajo `cwd`, declarado en `staged_paths`); `Write(<glob>)` como segunda línea | Exact / ToolsOnly | sin campo documentado: marcador en la prosa del hook | `CURSOR_CONFIG_DIR` | `Write(/abs/**)` absoluto |
| OpenCode | ToolCalls | extensión `tool.execute.before` que ejecuta `yunta fence opencode` y lanza su stderr, cargada por `OPENCODE_CONFIG_CONTENT`; `OPENCODE_PERMISSION` como segunda línea | Exact (bash deny) / ToolsOnly | `tool_use.part.state.status = error` + marcador | env inline | `external_directory: {"<root>/*":"allow"}` |
| Aider | None | ninguno (`--yes-always` acepta cualquier path; sin salida estructurada) | — (post-check) | — | `--config <scratch>/aider.yml`, `AIDER_*` | — |
| Goose | ToolCalls | extensión `PreToolUse` con `on_failure: block` bajo `GOOSE_PATH_ROOT=<scratch>` (si reubica `.agents/plugins`; si no, bajo `cwd` declarado en `staged_paths`) | Exact (hook también sobre `shell`) / ToolsOnly | `toolResponse.is_error` + prefijo fijo + marcador | `GOOSE_PATH_ROOT`, `GOOSE_MODE=auto` | ninguna (no confina) |
| Amp | ToolCalls | `amp.permissions` con `action: delegate` a `yunta fence amp` (exit ≥2 rechaza) en `--settings-file <scratch>/settings.json`; `Bash` en `reject` | Exact / ToolsOnly | `result.permission_denials[]` + marcador | `--settings-file`, `--mcp-config` inline | paths absolutos en `matches.path` |
| Kimi Code | ToolCalls | `[[hooks]] PreToolUse` en `KIMI_CODE_HOME=<scratch>/config.toml`; `Write(!{<globs>})` como segunda línea; o ACP con el harness como juez | Exact / ToolsOnly | prosa en la línea `tool` del JSONL + marcador | `KIMI_CODE_HOME`, `--add-dir` | `--add-dir` |

Lo que la muestra confirma y las cinco reglas que fija para todo adapter:

1. **Tres niveles alcanzan.** Ocho de nueve son `ToolCalls`; Codex es
   `Filesystem`; Aider es `None`. Un transporte distinto (hook, extensión,
   programa delegado, ACP) es el mismo nivel: cada llamada de escritura se
   juzga antes de ocurrir.
2. **El juez es uno.** Los siete `ToolCalls` ejecutan `yunta fence <id>`
   por su hook, extensión o programa delegado; el adapter escribe solo el
   codec. Las reglas declarativas del CLI (`write(<glob>)`, `Write(!{…})`,
   `permission.edit`) son una segunda línea, nunca la fuente de la cobertura:
   su semántica de glob es ajena y no es el juez.
3. **El rechazo se reconoce por el marcador**, y por el campo estructurado
   donde existe. Cuatro CLIs tienen campo; cuatro solo prosa. El parser de
   cada adapter emite `WriteRefused` por cualquiera de los dos; el marcador
   es la fuente del path.
4. **La configuración vive en `scratch_dir`.** Siete CLIs reubican su home de
   config con una variable; el adapter la apunta a `scratch_dir` y nada cae
   bajo `cwd`. Lo que un CLI solo lee desde el árbol del proyecto se planta
   bajo `cwd` y se declara en `staged_paths` (se conserva), como los skills.
5. **La cobertura se calcula, no se declara.** Ningún CLI cerca el shell por
   path sin sandbox; `edit` sin shell es `Exact`, `full` es `ToolsOnly`, un
   sandbox de raíces es `WidenedToRoots`. `Coverage::of` lo fija en core.

Límites que la muestra hace explícitos y spec-adapter §6 declara: los hooks
de Claude Code, Copilot, Kimi, Goose, Cursor y Gemini son fail-open ante
timeout (la llamada pasa; el post-check la atrapa; `fence_breach` la nombra);
el shell es ciego a paths en todos; Gemini con Seatbelt admite cinco
directorios extra; en Cursor no está documentado si la lista `allow` es
exclusiva bajo `-p --force`; Aider no tiene salida estructurada ni exit code
significativo.

---

## 8. Archivos

- **nuevo:** `crates/core/src/fence.rs`, `crates/core/tests/fence.rs`,
  `crates/cli/src/commands/fence.rs`, `crates/cli/tests/fence_cmd.rs`,
  `crates/adapters/src/claude_code/fence.rs` (codec + JSON del `--settings`),
  `crates/adapters/src/codex/fence.rs` (`sandbox_args(&Fence, PermissionProfile) -> Result<Vec<String>, Unbuildable>`, `coverage(&Fence, cwd) -> Coverage`),
  `crates/adapters/tests/fence.rs`, `docs/design/adr/D172-el-cerco.md`,
  `docs/design/plan-de-raiz/auditoria/09-mercado-de-clis.md`.
- **modifica:** `crates/core/src/capabilities.rs` (`fence: FenceLevel`,
  `Capability::Fence`, `as_str`, `declares`, derives); `crates/core/src/lib.rs`
  (exporta `fence`); `crates/core/src/port/mod.rs` (`SessionRequest`,
  `AgentEvent`, `AdapterError::FenceUnbuildable`, `Adapter::fence_codec`,
  `FenceCodec`, `HookReply`, `CodecError`); `crates/core/src/port/policy.rs`
  (fila `Fence`); `crates/core/src/events/session/{kinds,payloads,ledger,happening}.rs`;
  `crates/core/src/events/wire.rs`; `crates/core/schemas/events.json`
  (regenerado); `crates/engine/src/run/mod.rs` (`RunEnv.fence_hook`);
  `crates/engine/src/run/session_plan.rs` (`Fence::for_session`, `advice`, `fence_hook`, `scratch_dir`);
  `crates/engine/src/task_cycle/session.rs:150,290-302` (`DispatchOutcome.fence`, `SessionOpened.fence`, `WriteRefused` → `write_refused` con el `session_id` de la sesión abierta);
  `crates/engine/src/scope.rs` (`fence_breach`); `crates/engine/src/run/node_close.rs:212`,
  `crates/engine/src/task_cycle/attempt.rs:112` y su llamador, `crates/engine/src/run/loop_exec/integrate.rs:278`
  (llaman `fence_breach`; registran por `engine_finding`);
  `crates/cli/src/surface/lines.rs` (línea de `write_refused`, sufijo de `fence`);
  `crates/cli/src/context.rs` y `crates/cli/src/commands/mod.rs:120`
  (`FenceHook` una vez); `crates/cli/src/main.rs` y `crates/cli/src/config/env.rs`
  (subcomando `fence::SUBCOMMAND`, `Env::fence_var()`); `crates/cli/src/commands/{drive,test}.rs`
  (pasan el `fence_hook` de `Context` al `RunEnv`); `crates/testkit/src/bench.rs`
  (`fence_hook: None`); `crates/adapters/src/claude_code/{mod,permissions,parse}.rs`;
  `crates/adapters/src/codex/{mod,permissions,parse}.rs`;
  `crates/adapters/src/mock/{mod,fixture}.rs`; los fixtures YAML y el corpus
  que nombran `edit_hooks`; `docs/design/spec-adapter.md` (§2 `Capabilities`
  y `SessionRequest`, §3 `WriteRefused`, O5 → el cerco, §5 fila `fence`,
  §6 verdadero por adapter con nivel, cobertura y límite);
  `docs/design/contrato-del-run.md` (§6 "en caliente" → el cerco con nivel y
  cobertura; §6.1 el cerco como peldaño más fino; §6.2 la ampliación rige
  desde el intento siguiente); `docs/design/spec-events.md` (§0 conteo, §5.5
  `fence`, §5.x `write_refused`, `Capabilities` con `fence`);
  `docs/design/glosario.md` (cerco, nivel, cobertura, canal, cercado, juez,
  hook, codec, rechazo); `docs/adapters.md` (nivel por adapter);
  `docs/design/status.md` (verificación en vivo de los dos hooks pendiente,
  como A-17); `docs/design/deuda-consciente.md` (A-13 cerrado por D172 al
  cerrar 3-08); `docs/design/adrs.md` (D13 revisada por D172; índice);
  `docs/design/adr/D167-build-or-register.md` (`revised_by: [D172]`, nota).
- **borra:** `edit_constraints`, `Glob`, `edit_hooks`, `Capability::EditHooks`,
  la prosa de `edit_hooks` en `Policy`, `mock::is_blocked`, `blocked_markers`,
  el `target_digest: "blocked:<path>"`, `spec-adapter` O5 viejo.

---

## 9. Tests

| test | archivo | prueba |
|---|---|---|
| `a_write_inside_an_allowed_glob_is_allowed` | core/tests/fence.rs | regla 3 |
| `a_write_outside_every_glob_is_refused_naming_the_target` | core/tests/fence.rs | regla 3; `Refusal.target` es el path normalizado |
| `a_write_under_a_root_is_allowed_whatever_the_globs_say` | core/tests/fence.rs | regla 2 antes que 3 |
| `a_write_into_dot_git_is_refused` | core/tests/fence.rs | regla 3, `.git` y `.git/` |
| `a_relative_target_is_judged_against_the_worktree` | core/tests/fence.rs | regla 1 |
| `a_dot_dot_escape_is_refused_without_touching_disk` | core/tests/fence.rs | reglas 1 y 4; el path no existe |
| `an_empty_allowed_set_refuses_every_worktree_write_and_keeps_the_roots` | core/tests/fence.rs | `read_only` |
| `a_granted_expansion_is_inside_the_fence` | core/tests/fence.rs | `for_session` con `granted` |
| `a_refusal_message_round_trips_its_target_and_says_its_advice` | core/tests/fence.rs | `Display` → `refused_target`; los dos `Advice` |
| `text_without_the_marker_is_not_a_refusal` | core/tests/fence.rs | `refused_target` = `None` |
| `coverage_is_the_weakest_channel` | core/tests/fence.rs | la tabla de `Coverage::of`, las seis combinaciones |
| `a_fence_round_trips_through_the_environment` | core/tests/fence.rs | `to_env` → `from_env` |
| `a_fence_hook_names_the_subcommand_and_the_adapter` | core/tests/fence.rs | `FenceHook::command` |
| `write_refused_is_a_session_kind_that_moves_the_ledger` | core/tests/events.rs | KINDS, `is_audit = false`, `NodeRecord.refused` |
| `an_old_log_without_a_fence_level_reads_as_none` | core/tests/events.rs | `Capabilities` con `edit_hooks` |
| `the_fence_command_refuses_with_exit_two_and_the_reason_on_stderr` | cli/tests/fence_cmd.rs | codec claude-code, stdin real, binario real |
| `the_fence_command_allows_with_exit_zero_and_nothing_on_stdout` | cli/tests/fence_cmd.rs | ídem |
| `a_misconfigured_fence_refuses_rather_than_allows` | cli/tests/fence_cmd.rs | sin `YUNTA_FENCE` → exit 2 |
| `a_write_refused_line_names_its_target` | cli/src/surface tests | `lines.rs` |
| `a_claude_session_installs_the_fence_by_settings_and_add_dir_and_nothing_under_cwd` | adapters/tests/claude_code.rs | args desde `fence.roots` y `fence_hook`; `staged_paths` |
| `a_claude_session_without_a_hook_fails_before_spawning` | adapters/tests/claude_code.rs | `HookUnavailable` |
| `a_refused_write_in_the_stream_becomes_write_refused` | adapters/tests/claude_code.rs | brazo `User` del parser |
| `an_edit_profile_reports_exact_coverage_and_full_reports_tools_only` | adapters/tests/claude_code.rs | `SessionOpened.fence` |
| `a_read_only_claude_session_keeps_write_only_for_its_declared_files` | adapters/tests/claude_code.rs | perfiles |
| `a_codex_session_reports_widened_coverage_with_its_roots` | adapters/tests/codex.rs | `WidenedToRoots { [cwd, …roots] }` |
| `a_sandbox_denial_becomes_write_refused` | adapters/tests/codex.rs | parser |
| `a_read_only_codex_session_with_declared_files_fails_before_spawning` | adapters/tests/codex.rs | `SealedRoots` |
| `a_fixture_effect_outside_the_fence_is_refused_and_recorded` | adapters/tests/mock.rs | mock por `Fence::judge` |
| `every_capability_round_trips_through_a_fixture` | adapters/tests/mock.rs | M09, ahora con `fence` |
| `a_run_on_an_adapter_without_a_fence_says_so_once` | engine/tests/degradation.rs | reemplaza `…without_edit_hooks…` |
| `a_write_that_escapes_an_exact_fence_is_an_engine_finding` | engine/tests/degradation.rs | `fence_breach` en un nodo |
| `a_task_write_that_escapes_an_exact_fence_is_an_engine_finding` | engine/tests/degradation.rs | `fence_breach` en una tarea |
| `a_write_inside_widened_roots_is_a_scope_violation_and_nothing_more` | engine/tests/degradation.rs | sin finding |
| `every_session_carries_its_fence_and_read_only_allows_nothing_under_the_worktree` | engine/tests/run_sessions.rs | `open_session` |
| `a_task_session_advises_expansion_and_a_prompt_session_advises_a_finding` | engine/tests/run_sessions.rs | `Advice` |
| `write_refused_carries_the_session_that_refused_it` | engine/tests/run_sessions.rs | `session_id` |
| `docs_sync`: `Capability::ALL` ↔ spec-adapter §2 (ya en M23) cubre `fence`; `KINDS` ↔ spec-events (ya en M23) cubre `write_refused` | xtask | M23 |

Rojo primero: `a_write_outside_every_glob_is_refused_naming_the_target` es
el primer test del ítem y falla porque `yunta_core::fence` no existe.

---

## 10. Cierra

AD-D2, AD-D7, AD-D20, AD-D24, DO-D2, A-13.
