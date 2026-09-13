# La cerca — M25

Lo que una sesión puede escribir, dicho una sola vez, juzgado por una sola
función, construido por cada adapter con el mecanismo que su CLI tiene, y
reportado en el log con la cobertura real. Reemplaza cuatro formas de decir lo
mismo: `edit_constraints` (globs como `String`), `edit_hooks: bool`,
`artifact_dir` como permiso implícito, y los marcadores `blocked:<path>` del
mock. El post-check por diff (`engine/src/scope.rs`, se conserva) sigue siendo
la única garantía; la cerca es lo que evita que una escritura fuera de scope
llegue a existir, y lo dice cuando no pudo evitarla.

Decisión: D172 (revisa D13 y D167). Ítem: 3-08. Defectos: AD-D2, AD-D7 (con
3-07), AD-D20, AD-D24, DO-D2; deuda A-13.

---

## 1. Vocabulario

| término | qué es | dónde vive |
|---|---|---|
| **cerca** (`Fence`) | los globs que una sesión puede escribir bajo el worktree más las raíces absolutas fuera de él | `yunta_core::fence` |
| **nivel** (`FenceLevel`) | lo que un adapter sabe construir: nada, un juicio por llamada de herramienta, o un sandbox de filesystem | `Capabilities::fence` |
| **cobertura** (`Coverage`) | lo que la sesión concreta cercó, según lo que el adapter construyó: exacta, ensanchada a raíces, o solo herramientas | `FenceReport` en `agent_session_opened` |
| **canal** | por dónde una sesión escribe: las herramientas de archivo del CLI, o todo lo demás (shell, MCP, subagentes) | regla de `Coverage::of` |
| **juez** | la función pura que decide si un path está dentro de la cerca | `Fence::judge` |
| **codec** | la traducción entre el juez y el mecanismo de un CLI: qué llega por stdin, qué se responde | `FenceCodec` por adapter |
| **rechazo** (`write_refused`) | el hecho de que una escritura fue refusada antes de ocurrir | kind del dominio `session` |

En prosa: cerca, nivel, cobertura, canal, juez, codec, rechazo. En YAML, JSON
y código: `fence`, `FenceLevel`, `Coverage`, `Fence::judge`, `FenceCodec`,
`write_refused`.

---

## 2. Tipos

```rust
// crates/core/src/capabilities.rs
pub struct Capabilities {
    pub resume_session: bool,
    pub fence: FenceLevel,             // reemplaza `edit_hooks: bool`
    pub permission_profiles: bool,
    pub custom_agents: bool,
    pub usage_reporting: bool,
    pub skills: bool,
    pub run_tools: bool,
    pub network_isolation: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FenceLevel { None, ToolCalls, Filesystem }      // "none" | "tool_calls" | "filesystem"
pub enum Capability { ResumeSession, Fence, PermissionProfiles, CustomAgents, UsageReporting, Skills, RunTools, NetworkIsolation }
impl Capability { pub fn as_str(&self) -> &'static str /* Fence => "fence" */ }
impl Capabilities { pub fn declares(&self, cap: Capability) -> bool /* Fence => self.fence != FenceLevel::None */ }

// crates/core/src/fence.rs — nuevo; el único juez del workspace
pub struct Fence { pub allowed: Vec<ScopeGlob>, pub roots: Vec<PathBuf> }
impl Fence {
    pub fn everything(roots: Vec<PathBuf>) -> Self;                 // allowed = ["**"]
    pub fn read_only(roots: Vec<PathBuf>) -> Self;                  // allowed = []
    pub fn for_session(profile: PermissionProfile, scope: Option<&[ScopeGlob]>, artifact_dir: Option<&Path>) -> Self;
        // ReadOnly → read_only(roots); scope Some → allowed = scope; None → everything; roots = artifact_dir.into_iter().collect()
    pub fn judge(&self, worktree: &Path, target: &Path) -> Verdict;   // pura: no toca disco
    pub fn to_env(&self, worktree: &Path) -> (&'static str, String);  // (ENV_VAR, json)
    pub fn from_env(get: &dyn Fn(&str) -> Option<String>) -> Result<(Self, PathBuf), FenceEnvError>;
}
pub const ENV_VAR: &str = "YUNTA_FENCE";
pub enum Verdict { Allowed, Refused(Refusal) }
pub struct Refusal { pub target: PathBuf, pub worktree: PathBuf, pub allowed: Vec<ScopeGlob>, pub roots: Vec<PathBuf> }
impl Display for Refusal;                      // la única prosa del rechazo; empieza por REFUSAL_MARKER
pub const REFUSAL_MARKER: &str = "yunta: write refused: ";
pub fn refused_target(text: &str) -> Option<PathBuf>;   // reconoce un Display de Refusal; único parser del marcador
pub fn lexical_absolute(base: &Path, path: &Path) -> PathBuf;   // resuelve `.`/`..` sin disco; relativo → contra base

pub enum Fenced { Exact, Roots(Vec<PathBuf>) }
pub struct FenceReport { pub level: FenceLevel, pub coverage: Coverage }
#[serde(rename_all = "snake_case", tag = "coverage")]
pub enum Coverage { Exact, WidenedToRoots { roots: Vec<PathBuf> }, ToolsOnly }
impl Coverage {
    pub fn of(tools: Fenced, others: Option<Fenced>) -> Coverage;
    // (Exact, Some(Exact)) → Exact · (_, None) → ToolsOnly · resto → WidenedToRoots { roots: unión ordenada, sin repetidos }
}
```

**Reglas del juez** (`Fence::judge`, en este orden; la primera que decide, decide):

1. `target` se normaliza con `lexical_absolute(worktree, target)`: un path
   relativo se resuelve contra el worktree; `.` y `..` se colapsan sin leer el
   disco. Un CLI que entrega paths relativos los entrega relativos a su cwd, que
   es el worktree.
2. Bajo alguna `roots[i]` → `Allowed`.
3. Bajo `worktree`: si el path relativo empieza por `.git/` → `Refused` (el
   registro del run nunca es trabajo de una tarea); si `scope_globset(allowed)`
   lo matchea → `Allowed`; si no → `Refused`.
4. Fuera del worktree y de toda raíz → `Refused`.

`allowed` vacío deja toda escritura bajo el worktree en `Refused`: ese es el
perfil `read_only`, y las raíces siguen escribibles porque son el registro,
no el trabajo.

**El texto del rechazo** (`Refusal`'s `Display`), una sola vez, para el
modelo y para el parser:

```
yunta: write refused: <target> is outside this session's scope.
Allowed under <worktree>: <globs separados por ", "> (none = read-only); also writable: <roots separados por ", "> (none).
Ask for more scope with the run tool `yunta_request_scope_expansion`, or report the need as a finding; do not write here.
```

`refused_target` lee el path entre `REFUSAL_MARKER` y ` is outside`; un
texto sin el marcador devuelve `None`.

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
    pub fence: Fence,                       // siempre; reemplaza `edit_constraints: Option<Vec<Glob>>`
    pub budget: Budget,
    pub adapter_settings: serde_json::Map<String, serde_json::Value>,
    pub skills: Vec<PathBuf>,
    pub run_tools_endpoint: Option<RunToolsEndpoint>,
    pub artifact_dir: Option<PathBuf>,      // el lugar de los archivos declarados; `fence.roots` lo repite como permiso, construido en el mismo sitio
    pub scratch_dir: PathBuf,               // siempre; deja de ser Option
    pub yunta_bin: PathBuf,                 // el binario que responde `yunta fence <adapter>`
}
pub enum AgentEvent {
    SessionOpened { session_id: SessionId, model: Option<ModelName>, fence: Option<FenceReport> },   // None cuando el nivel es None
    WriteRefused { target: ToolTarget },
    RunToolsMounted { count: usize }, ToolUse { target: ToolTarget /* M11 */ }, Usage { … }, Note { … }, Completed { … }, Failed { … },
}
pub trait Adapter: Send + Sync {
    /* como hoy */
    fn fence_codec(&self) -> Option<&dyn FenceCodec> { None }   // Some solo si `capabilities().fence == ToolCalls` por hook
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
// KINDS = ["agent_session_opened", "agent_message", "capability_degraded", "write_refused"]; is_audit: write_refused = false (mueve SessionLedger)
// crates/core/src/events/session/payloads.rs (M03)
pub struct AgentSessionOpenedPayload { pub session_id: SessionId, pub agent: Option<AgentName>, pub model: Option<ModelName>, pub capabilities: Capabilities, #[serde(default, skip_serializing_if = "Option::is_none")] pub fence: Option<FenceReport> }
impl AgentSessionOpenedPayload { pub fn new(session_id: SessionId, agent: Option<AgentName>, model: Option<ModelName>, capabilities: Capabilities, fence: Option<FenceReport>) -> Self; }
pub struct WriteRefusedPayload { pub target: ToolTarget }
impl WriteRefusedPayload { pub fn new(target: ToolTarget) -> Self; }
pub enum Policy { PostCheckOnly, NoWritableRoots, NoTokenBudget, NoSkills, NoRunTools, NetworkOpen, FreshSession }
// crates/core/src/events/session/ledger.rs (M04)
pub struct SessionLedger { /* por sesión: */ fence: Option<FenceReport>, refused: Vec<ToolTarget>, /* resto como M04 */ }
impl SessionLedger { pub fn fence_of(&self, session: &SessionId) -> Option<&FenceReport>; pub fn refused_by(&self, session: &SessionId) -> &[ToolTarget]; }
// crates/core/src/port/policy.rs (M09): la fila
(Capability::Fence, Absence::DegradeWith(Policy::PostCheckOnly)),   // una vez por run
```

`EventPayload::KINDS` pasa a 37; `crates/core/schemas/events.json` se
regenera con `cargo xtask schema` (D166: sin tag, sin bump de
`schema_version`); `spec-events.md` gana §5.x `write_refused` y §5.5 lleva
`fence`. `Capabilities` serializa `fence` como string; el corpus y los
fixtures que escriben `edit_hooks` se reescriben.

---

## 4. El comando `yunta fence <adapter-id>`

Oculto en `--help`. Es el hook: el CLI del agente lo ejecuta antes de cada
escritura con la llamada en stdin.

```rust
// crates/cli/src/commands/fence.rs
pub fn run(registry: &Adapters, adapter: &AdapterId, env: &dyn Fn(&str) -> Option<String>, stdin: &[u8]) -> HookReply;
// 1. adapter = registry.find(id) → sin codec → HookReply { exit: 2, stderr: "yunta: fence misconfigured: adapter `<id>` has no fence codec" }
// 2. (fence, worktree) = Fence::from_env(env) → error → exit 2, stderr "yunta: fence misconfigured: <causa>"
// 3. target = codec.decode(stdin) → error → exit 2 "yunta: fence misconfigured: <causa>"; None → codec.encode(&Verdict::Allowed)
// 4. codec.encode(&fence.judge(&worktree, &target))
```

Un fallo nuestro refusa; nunca permite. La cerca viaja por `YUNTA_FENCE` en
el entorno del hijo (no es secreto: son globs y paths), puesta por el adapter
en `spawn` con `Fence::to_env`. `yunta_bin` sale una sola vez de
`std::env::current_exe()` en `Context` (hoy `commands/mod.rs:120` lo repite
para `--detach`; pasa a consumir el mismo valor) y llega al engine por
`RunEnv.yunta_bin: &Path`; `Bench` lo inyecta desde `CARGO_BIN_EXE_yunta`.

---

## 5. El engine

- **`open_session` (M08)** construye la cerca en un solo lugar:
  `require(Capability::Fence)` (degrada `PostCheckOnly` una vez por run si el
  nivel es `None`); `fence = Fence::for_session(profile, scope, artifact_dir)`;
  `scratch_dir` lo crea `SessionSlot` siempre; `yunta_bin` de `RunEnv`.
  `scope` es el de la tarea (`Task.scope`) o el del nodo (`Node.scope`), ya
  `Vec<ScopeGlob>` (M12).
- **Post-check.** `scope_check` no cambia. Después de él, `node_close`
  llama `fence_breach(report: Option<&FenceReport>, result: &ScopeCheckResult) -> Option<Breach>`
  (pura, `engine/src/scope.rs`): `Some(Breach { paths: result.violations.clone() })`
  solo con `Coverage::Exact` y `violations` no vacías. Un `Breach` se
  registra por el `engine_finding` que ya existe (`ctx.rs:190`) con
  `node = Some(nodo)`, `id = "engine-fence-breach"` (el esquema `engine-*`
  de `exec.rs:236,280`), `severity = FindingSeverity::Major` (la tarea ya
  falla por la violación; el finding pide revisar el adapter, no bloquea más
  que esa falla), `title = "the fence declared exact let a write through"`,
  `location = <primer path>`, `detail = "fence exact on <adapter>; <n> paths reached the diff outside it: <paths separados por ", ">"`,
  además de la falla de la tarea que ya existe. Bajo `WidenedToRoots` o
  `ToolsOnly` una violación es lo que siempre fue: la tarea falla con la lista.
- **Nada más decide con la cerca.** `decide()` (M07) no la mira; los
  `write_refused` son crónica y conteo, no estado de nodo.
- **Crónica (M19).** `write_refused` → `Session::Refused { target }`,
  `kept: no`, dice `implement — write refused: src/db/schema.rs`.
  `agent_session_opened` dice
  `implement — session opened as builder on opus · fence exact` /
  `· fence widened to <n> roots` / `· fence on tool calls`; sin `fence`, nada.
  `capability_degraded` dice `! implement — fence not declared by aider: scope checked after the session`.
  Tabla en `cronica.md`.
- **`check` (M09).** Sin cambio: `Fence` degrada, no falla en `check`.

---

## 6. Adapters builtin

### claude-code — `FenceLevel::ToolCalls`

- **Instala.** `--add-dir <artifact_dir>` cuando hay `artifact_dir`;
  `--settings '<json>'` inline con
  `{"hooks":{"PreToolUse":[{"matcher":"Edit|Write|MultiEdit|NotebookEdit","hooks":[{"type":"command","command":"\"<yunta_bin>\" fence claude-code","timeout":10}]}]}}`;
  `YUNTA_FENCE` en el entorno del hijo. Nada bajo `cwd`. Los `10` segundos
  son el timeout del hook y se registran en D170 como umbral del adapter.
- **Codec** (`adapters/src/claude_code/fence.rs`): stdin es el JSON del
  hook; `tool_input.file_path`, o `tool_input.notebook_path` para
  `NotebookEdit`; sin ninguno → `MissingField`. `Refused` → `exit 2`,
  `stderr = Refusal`'s `Display`; `Allowed` → `exit 0`, vacío.
- **Cobertura.** `tools = Exact`. `others`: perfil `edit` → la lista de
  herramientas no tiene `Bash` → `Some(Exact)`; `full` → `None`;
  `read_only` → sin herramientas de escritura (3-07) → `Some(Exact)`. Es
  decir: `edit`/`read_only` reportan `Exact`, `full` reporta `ToolsOnly`.
- **Parser** (`claude_code/parse.rs`): un `tool_result` con `is_error: true`
  cuyo contenido `refused_target` reconoce → `WriteRefused { target: ToolTarget::of_path(path relativo al worktree) }`.
  El path viene del marcador; no hay correlación por id.
- **Límite declarado.** Si el hook no responde en el timeout, Claude Code
  deja pasar la llamada; el post-check la atrapa y `fence_breach` la nombra.

### codex — `FenceLevel::Filesystem`

- **Instala.** `--sandbox workspace-write` y
  `-c 'sandbox_workspace_write.writable_roots=["<artifact_dir>"]'` (por
  `ConfigOverride`, se conserva). Perfil `read_only` → `--sandbox read-only`;
  si además hay `artifact_dir`, el adapter registra
  `Degradation::new(Capability::Fence, codex, Policy::NoWritableRoots)` antes
  de abrir la sesión: los archivos declarados de ese nodo no se pueden
  escribir con este perfil en este adapter (cierra AD-D20).
- **Cobertura.** `WidenedToRoots { roots: [cwd, artifact_dir] }`: el sandbox
  es por directorio en los dos canales. `.git` lo protege el sandbox mismo.
- **Parser** (`codex/parse.rs`): el ítem de proceso terminado con la marca de
  sandbox denegado → `WriteRefused { target: ToolTarget::opaque(command) }`.
- **Límite declarado.** La política partida del sandbox (`/repo=write`,
  `/repo/a=none`) no se usa: no expresa globs y el post-check ya cubre lo que
  ella cubriría.

### mock — nivel del fixture

`capabilities: { fence: none | tool_calls | filesystem }` (twin
`FixtureCapabilities.fence: FenceLevel`, M09) y `fence_coverage: exact |
widened | tools_only` (opcional; default `exact` para `tool_calls`, `widened`
para `filesystem`). Un paso del guion que escribe bajo el worktree o fuera
(`write:`, `edit:`, según `fixture.rs`) pasa por `Fence::judge` con la
`fence` de la request: `Refused` → el mock emite `WriteRefused` y no escribe;
con nivel `filesystem` el juicio usa `Fence::everything(roots)` (el sandbox no
sabe de globs); con nivel `none` escribe siempre. `is_blocked`,
`blocked_markers` y `target_digest: "blocked:<path>"` se borran.

---

## 7. Adapters futuros: la muestra del mercado

Relevado el 2026-09-13 sobre los ocho CLIs más usados después de Claude Code
y Codex (evidencia en `auditoria/09-mercado-de-clis.md`). La tabla dice qué
construiría cada adapter y qué reportaría; no está construido y no es deuda:
es la prueba de que los tipos de §2 alcanzan para cualquiera de ellos sin
una variante nueva.

| CLI | nivel | mecanismo que el adapter usa | cobertura `edit` / `full` | rechazo en el stream | inyección por sesión | raíces |
|---|---|---|---|---|---|---|
| Gemini CLI | ToolCalls | `--policy <toml>` con `argsPattern` regex sobre el path absoluto, o hook `BeforeTool` = `yunta fence gemini` | Exact (`auto_edit` ya niega shell) / ToolsOnly | `tool_result.error.type = policy_violation` (estructurado) + marcador | `--policy`, `--include-directories`, `GEMINI_CLI_HOME=<scratch>` | `--include-directories` (≤5 con Seatbelt) |
| Copilot CLI | ToolCalls | `--allow-tool='write(<glob>)'` por cada glob, `--excluded-tools=bash,…` bajo `edit`, hook `preToolUse` bajo `COPILOT_HOME=<scratch>` | Exact / ToolsOnly (o `WidenedToRoots` con `--sandbox`) | `tool.execution_complete.error.code = denied` (estructurado) + marcador | flags; `COPILOT_HOME` | `--add-dir` + `write(<root>/**)` |
| Cursor CLI | ToolCalls | `permissions.allow: Write(<glob>)` en `cli-config.json` bajo `CURSOR_CONFIG_DIR=<scratch>`; hook `preToolUse` | Exact (exclusividad del allow no verificada) / ToolsOnly | sin campo documentado: marcador en la prosa del hook | `CURSOR_CONFIG_DIR` | `Write(/abs/**)` absoluto |
| OpenCode | ToolCalls | `OPENCODE_PERMISSION='{"edit":{"*":"deny","<glob>":"allow"},"bash":"deny"}'`; plugin `tool.execute.before` que lanza el marcador | Exact / ToolsOnly | `tool_use.part.state.status = error` + marcador | env inline | `external_directory: {"<root>/*":"allow"}` |
| Aider | None | ninguno (`--yes-always` acepta cualquier path; sin salida estructurada) | — (post-check) | — | `--config <scratch>/aider.yml`, `AIDER_*` | — |
| Goose | ToolCalls | plugin `PreToolUse` con `on_failure: block` bajo `GOOSE_PATH_ROOT=<scratch>` (si reubica `.agents/plugins`; si no, bajo `cwd` declarado por `staged_paths`) | Exact (hook también sobre `shell`) / ToolsOnly | `toolResponse.is_error` + prefijo fijo + marcador | `GOOSE_PATH_ROOT`, `GOOSE_MODE=auto` | ninguna (no confina) |
| Amp | ToolCalls | `amp.permissions` con `matches.path` por glob y `Bash` en `reject`, en `--settings-file <scratch>/settings.json` | Exact / ToolsOnly | `result.permission_denials[]` (estructurado) + marcador | `--settings-file`, `--mcp-config` inline | paths absolutos en `matches.path` |
| Kimi Code | ToolCalls | `[[permission.rules]] Write(!{<globs>})` + hook `PreToolUse` sobre `Bash` en `KIMI_CODE_HOME=<scratch>/config.toml`; o ACP con el harness como juez | Exact / ToolsOnly | prosa en `role: tool` + marcador | `KIMI_CODE_HOME`, `--add-dir` | `--add-dir` |

Lo que la muestra confirma y las cinco reglas que fija para todo adapter:

1. **Tres niveles alcanzan.** Ocho de nueve son `ToolCalls`; Codex es
   `Filesystem`; Aider es `None`. Un transporte distinto (hook, reglas
   declarativas, plugin, ACP) es el mismo nivel: cada llamada de escritura se
   juzga antes de ocurrir.
2. **El juez es uno.** Siete CLIs aceptan un hook de comando con JSON por
   stdin y rechazo por exit 2 o JSON en stdout; el adapter escribe solo el
   codec. Un adapter con reglas declarativas (Copilot, Amp, Kimi, OpenCode)
   traduce `Fence` a sus reglas con una función pura propia
   (`fence::rules_of(&Fence) -> …`) y sigue usando el marcador para el
   rechazo, o instala además el hook como segunda línea.
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
  `crates/adapters/src/codex/fence.rs` (`sandbox_args(&Fence, PermissionProfile) -> Vec<String>`, `coverage(&Fence, cwd) -> Coverage`),
  `crates/adapters/tests/fence.rs`, `docs/design/adr/D172-la-cerca.md`,
  `docs/design/plan-de-raiz/auditoria/09-mercado-de-clis.md`.
- **modifica:** `crates/core/src/capabilities.rs` (`fence: FenceLevel`,
  `Capability::Fence`, `as_str`, `declares`); `crates/core/src/lib.rs`
  (exporta `fence`); `crates/core/src/port/mod.rs` (`SessionRequest`,
  `AgentEvent`, `Adapter::fence_codec`, `FenceCodec`, `HookReply`,
  `CodecError`); `crates/core/src/port/policy.rs` (fila `Fence`);
  `crates/core/src/events/session/{kinds,payloads,ledger,happening}.rs`;
  `crates/core/src/events/wire.rs`; `crates/core/schemas/events.json`
  (regenerado); `crates/engine/src/run/mod.rs` (`RunEnv.yunta_bin`);
  `crates/engine/src/run/session_plan.rs` (`Fence::for_session`, `scratch_dir`, `yunta_bin`);
  `crates/engine/src/scope.rs` (`fence_breach`); `crates/engine/src/run/node_close.rs`
  (llama `fence_breach`, registra por `engine_finding`);
  `crates/engine/src/view/chronicle.rs`, `crates/cli/src/surface/chronicle.rs` (M19);
  `crates/cli/src/context.rs` y `crates/cli/src/commands/mod.rs:120`
  (`yunta_bin` una vez); `crates/cli/src/commands/mod.rs` (subcomando oculto `fence`);
  `crates/testkit/src/bench.rs` (`yunta_bin` desde `CARGO_BIN_EXE_yunta`);
  `crates/adapters/src/claude_code/{mod,permissions,parse}.rs`;
  `crates/adapters/src/codex/{mod,permissions,parse}.rs`;
  `crates/adapters/src/mock/{mod,fixture}.rs`; los fixtures YAML y el corpus
  que nombran `edit_hooks`; `docs/design/spec-adapter.md` (§2 `Capabilities`
  y `SessionRequest`, §3 `WriteRefused`, O5 → la cerca, §5 fila `fence`,
  §6 verdadero por adapter con nivel, cobertura y límite);
  `docs/design/contrato-del-run.md` (§6 "en caliente" → la cerca con nivel y
  cobertura; §6.1 la cerca como peldaño más fino); `docs/design/spec-events.md`
  (§5.5 `fence`, §5.x `write_refused`, `Capabilities` con `fence`);
  `docs/design/glosario.md` (cerca, nivel, cobertura, canal, juez, codec,
  rechazo); `docs/adapters.md` (nivel por adapter); `docs/design/status.md`
  (verificación en vivo de los dos hooks pendiente, como A-17);
  `docs/design/deuda-consciente.md` (A-13 cerrado por D172 al cerrar 3-08);
  `docs/design/adrs.md` (D13 revisada por D172; índice); `docs/design/adr/D167-build-or-register.md`
  (`revised_by: [D172]`, nota).
- **borra:** `edit_constraints`, `Glob`, `edit_hooks`, `Capability::EditHooks`,
  `Policy`'s prosa de `edit_hooks`, `mock::is_blocked`, `blocked_markers`,
  el `target_digest: "blocked:<path>"`, `spec-adapter` O5 viejo.

---

## 9. Tests

| test | archivo | prueba |
|---|---|---|
| `a_write_inside_an_allowed_glob_is_allowed` | core/tests/fence.rs | regla 3 |
| `a_write_outside_every_glob_is_refused_naming_the_target` | core/tests/fence.rs | regla 3; `Refusal.target` es el path normalizado |
| `a_write_under_a_root_is_allowed_whatever_the_globs_say` | core/tests/fence.rs | regla 2 antes que 3 |
| `a_write_into_dot_git_is_refused` | core/tests/fence.rs | regla 3, `.git/` |
| `a_relative_target_is_judged_against_the_worktree` | core/tests/fence.rs | regla 1 |
| `a_dot_dot_escape_is_refused_without_touching_disk` | core/tests/fence.rs | regla 1 y 4; el path no existe |
| `an_empty_allowed_set_refuses_every_worktree_write_and_keeps_the_roots` | core/tests/fence.rs | `read_only` |
| `a_refusal_message_round_trips_its_target` | core/tests/fence.rs | `Display` → `refused_target` |
| `text_without_the_marker_is_not_a_refusal` | core/tests/fence.rs | `refused_target` = `None` |
| `coverage_is_the_weakest_channel` | core/tests/fence.rs | la tabla de `Coverage::of`, las cinco combinaciones |
| `a_fence_round_trips_through_the_environment` | core/tests/fence.rs | `to_env` → `from_env` |
| `write_refused_is_a_session_kind_that_moves_the_ledger` | core/tests/events.rs | KINDS, `is_audit = false`, `SessionLedger::refused_by` |
| `the_fence_command_refuses_with_exit_two_and_the_reason_on_stderr` | cli/tests/fence_cmd.rs | codec claude-code, stdin real |
| `the_fence_command_allows_with_exit_zero_and_nothing_on_stdout` | cli/tests/fence_cmd.rs | ídem |
| `a_misconfigured_fence_refuses_rather_than_allows` | cli/tests/fence_cmd.rs | sin `YUNTA_FENCE` → exit 2 |
| `a_claude_session_installs_the_fence_by_settings_and_add_dir_and_nothing_under_cwd` | adapters/tests/claude_code.rs | args y `staged_paths` |
| `a_refused_write_in_the_stream_becomes_write_refused` | adapters/tests/claude_code.rs | parser |
| `an_edit_profile_reports_exact_coverage_and_full_reports_tools_only` | adapters/tests/claude_code.rs | `SessionOpened.fence` |
| `a_codex_session_reports_widened_coverage_with_its_roots` | adapters/tests/codex.rs | `WidenedToRoots { [cwd, artifact_dir] }` |
| `a_sandbox_denial_becomes_write_refused` | adapters/tests/codex.rs | parser |
| `a_read_only_codex_session_with_declared_files_degrades_by_name` | adapters/tests/codex.rs | `NoWritableRoots` |
| `a_fixture_write_outside_the_fence_is_refused_and_recorded` | adapters/tests/mock.rs | mock por `Fence::judge` |
| `every_capability_round_trips_through_a_fixture` | adapters/tests/mock.rs | M09, ahora con `fence` |
| `a_run_on_an_adapter_without_a_fence_says_so_once` | engine/tests/degradation.rs | reemplaza `…without_edit_hooks…` |
| `a_write_that_escapes_an_exact_fence_is_an_engine_finding` | engine/tests/degradation.rs | `fence_breach` |
| `a_write_inside_widened_roots_is_a_scope_violation_and_nothing_more` | engine/tests/degradation.rs | sin finding |
| `every_session_carries_its_fence_and_read_only_allows_nothing_under_the_worktree` | engine/tests/run_sessions.rs | `open_session` |
| `the_chronicle_says_the_fence_and_each_refusal` | cli/tests/chronicle.rs (M19) | tres líneas |
| `docs_sync`: `FenceLevel` ↔ spec-adapter §2; `Coverage` ↔ glosario; `write_refused` ↔ spec-events | xtask | M23 |

Rojo primero: `a_write_outside_every_glob_is_refused_naming_the_target` es
el primer test del ítem y falla porque `yunta_core::fence` no existe.

---

## 10. Cierra

AD-D2, AD-D7 (con 3-07), AD-D20, AD-D24, DO-D2, A-13.
