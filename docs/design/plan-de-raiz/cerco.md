# El cerco y el riel

Lo que una sesión puede escribir se dice una sola vez, en un `Fence`, y lo
juzga una sola función, `Fence::judge`. Antes de cada escritura de una
herramienta de archivo, la CLI del agente pregunta al engine por medio de su
hook, que es el riel: `yunta rail <adapter-id> <evento>`. El codec del adapter
traduce lo que la CLI envía, el engine juzga la llamada contra el cerco de la
sesión y registra cada rechazo antes de responderlo, y el codec escribe la
respuesta en la forma que esa CLI lee. Lo que el riel decide deja su hecho en
el log: `write_refused`, `stop_refused` o `write_formatted`.

**El riel no es un sandbox.** Juzga las rutas que una herramienta de archivo
de la CLI declara. No confina procesos, comandos de shell, red ni escrituras
que la llamada no nombra. Lo que escribe un comando lo confina el sandbox de la
CLI cuando lo tiene, por directorio y nunca por glob; cuando no lo tiene, nada
lo confina. La garantía es el post-check por diff (`scope_check`): el cerco
evita que una escritura fuera de alcance ocurra cuando puede, y la cobertura
dice cuánto pudo evitar.

Decisiones: D172 (el cerco, revisada por D249, D250 y D251); D249 (los hooks
preguntan al engine); D251 (una edición de Codex se juzga antes de aplicarse).
Completan D252 (los comandos corren en el sandbox de la CLI), D254 (el engine
juzga la parada antes de que la sesión termine) y D255 (lo que una sesión
escribe se formatea al escribirlo).

---

## 1. Vocabulario

| término | qué es | dónde vive |
|---|---|---|
| **cerco** (`Fence`) | lo que una sesión puede escribir: los globs bajo el worktree (`allowed`: `None` no pone techo, `Some([])` no admite nada), las raíces absolutas fuera de él (`roots`), lo que ninguna sesión escribe (`denied`) y el consejo que da un rechazo (`advice`) | `yunta_core::fence` |
| **juez** (`Fence::judge`) | la función pura que decide si un path está dentro del cerco; no lee el disco | `yunta_core::fence` |
| **nivel** (`FenceLevel`) | lo que un adapter sabe construir: `none`, `tool_calls` (un juicio por cada llamada de herramienta de archivo) o `filesystem` (un sandbox por directorio) | `Capabilities::fence` |
| **canal** | por dónde escribe una sesión: las herramientas de archivo de la CLI, o todo lo demás (comandos, MCP, tareas delegadas). Es prosa; el código lo expresa como `Fenced` | `Coverage::of` |
| **cobertura** (`Coverage`) | lo que la sesión quedó cercada, calculado de lo que el adapter construyó: `exact`, `widened_to_roots` o `tools_only`. Es la del canal más débil | `agent_session_opened.fence` |
| **riel** | el hook de una CLI y la conversación que abre con el engine: la CLI pregunta antes de escribir, después de escribir y antes de parar | `yunta_core::rail` |
| **evento** (`RailEvent`) | el momento en que la CLI pregunta: `before-write`, `after-write` o `before-stop` | `yunta_core::rail` |
| **llamada y respuesta** (`RailCall`, `RailAnswer`) | lo que la CLI pregunta, con sus rutas; lo que el engine responde: `proceed`, `refuse` o `tell` (superado por [D266](../adr/D266-the-rail-owns-its-windows-answers-and-failures.md): `WriteAnswer`, `FormatAnswer` y `StopAnswer`) | `yunta_core::rail` |
| **codec** (`RailCodec`) | la traducción de una CLI al riel: lee la llamada de su stdin y escribe la respuesta en la forma que esa CLI lee | un codec por adapter; el trait está en `yunta_core::port` |
| **comando del riel** (`RailCommand`) | la línea que la configuración de la CLI ejecuta: `yunta rail <adapter-id> <evento>` | `yunta_core::rail`, construido por el CLI |
| **rechazo** (`write_refused`) | una escritura que el engine negó antes de que ocurriera; el evento queda en el log antes de que la CLI lo reciba | evento del dominio `session` |

En prosa: cerco, juez, nivel, canal, cobertura, riel, codec y rechazo. En
código: `Fence`, `Fence::judge`, `FenceLevel`, `Coverage`, `RailEvent`,
`RailAnswer` (superado por [D266](../adr/D266-the-rail-owns-its-windows-answers-and-failures.md)), `RailCodec` y `write_refused`.

---

## 2. Tipos

```rust
// crates/core/src/capabilities.rs
pub struct Capabilities {
    pub fence: FenceLevel,   // un log escrito antes lleva `edit_hooks`, que se lee como `None`
    /* ... */
}
pub enum FenceLevel { #[default] None, ToolCalls, Filesystem }   // "none" | "tool_calls" | "filesystem"

// crates/core/src/fence.rs
pub struct Fence {
    pub allowed: Option<Vec<ScopeGlob>>,   // None: sin techo bajo el worktree; Some([]): nada
    pub roots: Vec<PathBuf>,               // absolutas, escribibles fuera del worktree
    pub advice: Advice,                    // qué dice un rechazo: pedir ampliación o reportar
    pub denied: Vec<ScopeGlob>,            // lo que ninguna sesión escribe bajo el worktree
    pub naming: ToolNaming,                // cómo la CLI nombra las herramientas del run
}
pub enum Advice { RequestExpansion, ReportFinding }
pub enum Verdict { Allowed, Refused(Refusal) }
pub struct Refusal {
    pub target: PathBuf,
    pub worktree: PathBuf,
    pub allowed: Option<Vec<ScopeGlob>>,
    pub roots: Vec<PathBuf>,
    pub advice: Advice,
    pub denied: bool,
    pub naming: ToolNaming,
}
pub enum Fenced { Exact, Roots(Vec<PathBuf>) }
pub enum Coverage { Exact, WidenedToRoots { roots: Vec<PathBuf> }, ToolsOnly }
impl Coverage {
    pub fn of(tools: Fenced, others: Option<Fenced>) -> Coverage;
}
impl Fence {
    pub fn everything(roots: Vec<PathBuf>, advice: Advice) -> Self;   // allowed = None
    pub fn read_only(roots: Vec<PathBuf>, advice: Advice) -> Self;    // allowed = Some([])
    pub fn for_session(
        profile: PermissionProfile,
        scope: Option<&[ScopeGlob]>,
        granted: &[ScopeGlob],
        artifact_dir: Option<&Path>,
        advice: Advice,
    ) -> Self;
    pub fn sharing(self, dirs: &[PathBuf]) -> Self;   // las raíces que comparte el run
    pub fn denying(self, denied: &[ScopeGlob]) -> Self;
    pub fn naming(self, naming: ToolNaming) -> Self;
    pub fn judge(&self, worktree: &Path, target: &Path) -> Verdict;   // pura: no lee el disco
}
// Superado por D266: el marcador se define una vez, en `core::rail::answer`, y `fence` lo reexporta.
pub const REFUSAL_MARKER: &str = "yunta: write refused: ";
pub fn lexical_absolute(base: &Path, path: &Path) -> PathBuf;

// crates/core/src/rail.rs
pub const SUBCOMMAND: &str = "rail";
pub const URL_VAR: &str = "YUNTA_RAIL_URL";
pub const TOKEN_VAR: &str = "YUNTA_RAIL_TOKEN";
pub const ROUTE: &str = "/rail";
pub enum RailEvent { BeforeWrite, AfterWrite, BeforeStop }   // "before-write" | "after-write" | "before-stop"
pub struct RailCall { pub session_id: SessionId, pub act: RailAct }
pub enum RailAct {
    BeforeWrite { paths: Vec<PathBuf> },
    AfterWrite { paths: Vec<PathBuf> },
    BeforeStop,
}
// Superado por D266: WriteAnswer, FormatAnswer y StopAnswer, uno por momento, sustituyen a RailAnswer.
// La negativa de una escritura sin engine la arma WriteRefusal::unasked; no hay un unasked común.
pub enum RailAnswer { Proceed, Refuse { reason: String }, Tell { context: String } }
pub fn unasked(event: RailEvent, why: &str) -> RailAnswer;
pub struct RailCommand { bin: PathBuf }   // RailCommand::new(bin); argv(&AdapterId, RailEvent) -> [bin, "rail", id, evento]
pub struct RailEndpoint { pub url: String, pub token: Secret<String> }

// crates/core/src/port/session.rs
pub trait RailCodec: Send + Sync {
    // Superado por D266: decode devuelve RailCall sin Option, y hay un método por momento (before_write, after_write y before_stop).
    fn decode(&self, event: RailEvent, stdin: &[u8]) -> Result<Option<RailCall>, CodecError>;
    fn encode(&self, event: RailEvent, answer: &RailAnswer) -> HookReply;
}
```

En el cable, `RailCall` lleva `{"session_id": …, "act": {"event": "before-write",
"paths": [...]}}`; `RailAnswer`, `{"answer": "proceed"}`, `{"answer": "refuse",
"reason": …}` o `{"answer": "tell", "context": …}`; `Coverage`, `{"coverage":
"exact"}`, `{"coverage": "widened_to_roots", "roots": [...]}` o `{"coverage":
"tools_only"}`. *Superado por [D266](../adr/D266-the-rail-owns-its-windows-answers-and-failures.md): cada momento tiene su respuesta, `FormatAnswer` lleva `notes` en vez de `context`, y `StopAnswer` lleva su `reason`.*

### Reglas del juez

`Fence::judge` responde en este orden, y decide la primera regla que aplica:

1. `target` se resuelve con `lexical_absolute(worktree, target)`: un path
   relativo se resuelve contra el worktree, `.` y `..` se colapsan sin leer el
   disco, y un symlink no se sigue. Un CLI que entrega paths relativos los
   entrega relativos a su cwd, que es el worktree.
2. Bajo alguna raíz: `Allowed`.
3. Fuera del worktree: `Refused`.
4. Bajo el worktree, si `denied` lo matchea: `Refused`, con `denied` en el
   rechazo.
5. Bajo el worktree, si la ruta relativa es `.git` o empieza por `.git/`:
   `Refused`. El registro del run nunca es trabajo de una tarea. `.git` a secas
   es el archivo que deja `git worktree`.
6. Bajo el worktree: con `allowed` en `None`, `Allowed`; con `Some(globs)`,
   `Allowed` solo si algún glob lo matchea; si no, `Refused`.

`allowed = Some([])` es el perfil `read_only`: toda escritura bajo el worktree
queda en `Refused`, y las raíces siguen escribibles, porque son el registro del
run y no el trabajo.

### El texto del rechazo

`Refusal` tiene un único `Display`: es el texto que lee el modelo y el que la
CLI muestra. Siempre empieza por `REFUSAL_MARKER`. Si `denied` lo causa:

```text
yunta: write refused: <ruta> is outside what this session may ever write: the project denies it to every run (permissions.paths.deny), or it holds a test a person approved, and no request widens that. Do not write here.
```

En otro caso:

```text
yunta: write refused: <ruta> is outside this session's scope.
Allowed under <worktree>: <globs | everything | none>; also writable: <raíces | none>.
<consejo>
```

El consejo depende de `advice`. `RequestExpansion` se da a una sesión que puede
pedir más alcance con la herramienta de ampliación del run: dice que la
ampliación rige desde el intento siguiente y que, si un criterio propio de la
tarea falla señalando ese archivo como `path:line`, el engine la concede al
terminar el intento. `ReportFinding` pide reportar la necesidad como hallazgo.

### Las ampliaciones concedidas

El alcance de una sesión de tarea es el declarado más lo concedido en intentos
anteriores (`already_granted_paths`), y el cerco se arma con ese alcance. Una
ampliación pedida durante un intento la evalúa el engine al terminar el
intento; si la concede, la sesión siguiente la tiene dentro del cerco. No hay
un flujo que pida y escriba en el mismo intento: la escritura se rechaza, el
pedido queda registrado y el reintento lo usa.

---

## 3. Puerto y eventos

```rust
// crates/core/src/port/session.rs
pub struct SessionRequest {
    /* ... */
    pub fence: Fence,                       // siempre: la única fuente de permisos de escritura
    pub rail_command: Option<RailCommand>,  // None solo en un arnés sin binario
    pub rail: Option<RailEndpoint>,         // el riel del listener de la sesión, con su token
    pub artifact_dir: Option<PathBuf>,      // dónde van los archivos declarados; ningún adapter lo lee como permiso
    pub scratch_dir: PathBuf,               // andamiaje de la sesión, fuera de `cwd`
    /* ... */
}
pub enum AgentEvent {
    SessionOpened { session_id: SessionId, model: Option<ModelName>, fence: Option<Coverage> },   // None si el nivel es None
    WriteRefused { target: ToolTarget },    // un rechazo que el adapter reporta desde su stream
    RunToolsMounted { count: usize },
    RunToolFailed { tool: RunTool, cause: RunToolFailureCause },
    ToolUse { name: String, target: ToolTarget },
    Usage { input_tokens: Option<u64>, output_tokens: Option<u64>, cached_input_tokens: Option<u64> },
    Note { text: String },
    Completed { result: AgentOutcome },
    Failed { error: AgentError, cause: FailureCause },
}
pub enum Unbuildable { HookUnavailable }   // "this adapter cannot reach the rail: ..." (crates/core/src/error.rs)
// AdapterError::FenceUnbuildable { adapter, source: Unbuildable }: falla antes de lanzar la CLI
pub trait Adapter: Send + Sync {
    fn capabilities(&self) -> Capabilities;
    fn rail_codec(&self) -> Option<&dyn RailCodec> { None }   // Some solo con FenceLevel::ToolCalls
    /* ... */
}
```

El cerco deja cuatro hechos de sesión (`crates/core/src/events/session/`). Tres
salen del riel, y el cuarto sale de la apertura de la sesión:

- `write_refused { session_id, target }`: una escritura negada antes de que
  ocurriera. El ledger del nodo lo pliega en `NodeRecord.refused`, por sesión.
- `stop_refused { session_id, state, why, task_id }`: una parada devuelta.
  `state` es el estado del trabajo; `why` lista lo que lo impedía
  (`criteria_red`, `outside_scope`, `artifact_missing`, `artifact_refused`).
- `write_formatted { session_id, run, paths, changed, exit_code, duration_ms }`:
  un formateador que el engine corrió después de una escritura.
- `agent_session_opened.fence`: la cobertura de la sesión, calculada por el
  adapter.

Un adapter sin nivel degrada con la política `(Capability::Fence,
Absence::DegradeWith(Policy::PostCheckOnly))`: el run lo registra una vez, y la
sesión escribe sin juicio previo. El post-check es entonces la única guarda.

---

## 4. El comando `yunta rail <adapter-id> <evento>`

Oculto en `--help`; su nombre es `yunta_core::rail::SUBCOMMAND`. Ninguna persona
lo invoca. La CLI del agente lo ejecuta antes de cada escritura de archivo,
después de cada una y antes de parar, con la llamada en stdin y con
`YUNTA_RAIL_URL` y `YUNTA_RAIL_TOKEN` en el entorno. El adapter que abrió la
sesión es quien puso ese comando en la configuración de la CLI.

> **Superado por [D266](../adr/D266-the-rail-owns-its-windows-answers-and-failures.md).** Los pasos 3 a 5 cambian: `decode` ya no devuelve `None`; la ventana de escritura cuenta desde el arranque del proceso del hook (7 s); y cada pregunta pasa por `settle`, que responde con el tipo del momento y reporta una vez, por `/rail/report`, una falla que tiene causa cerrada. Lo que sigue es el diseño original.

`rail_hook` (`crates/cli/src/cli/dispatch.rs`) llama a `commands::rail::run`, en
este orden:

1. Lee stdin completo.
2. Construye el adapter sin `probe()` y toma su codec. Sin codec, sale con
   código 2 y `yunta: rail misconfigured: this adapter has no rail codec` en
   stderr.
3. `decode` lee la llamada. Una sin nada que preguntar (`Ok(None)`) pasa sin
   preguntar. Una que el codec no lee responde como `unasked`.
4. Sin `YUNTA_RAIL_URL` y `YUNTA_RAIL_TOKEN` en el entorno, responde como
   `unasked`.
5. Pregunta por `POST /rail/<evento>` con `Authorization: Bearer` al listener de
   la sesión. Espera 8 s en `before-write` y en `after-write`; en `before-stop`
   espera lo que el engine tarde. Si la pregunta falla, responde como `unasked`.
6. `encode` escribe la respuesta en la forma de esa CLI.

> **Superado por [D266](../adr/D266-the-rail-owns-its-windows-answers-and-failures.md).** `unasked` ya no es una respuesta común: cada tipo de respuesta tiene su propio `unasked`, y la negativa de una escritura sin engine lleva la causa en su texto (`WriteRefusal::unasked`). Un `after-write` y un `before-stop` que no pueden preguntar pasan.

`unasked` es la respuesta de un hook que no puede preguntar. Un `before-write`
responde `refuse` con `yunta: write refused: the engine could not be asked
(<motivo>).`; los demás eventos pasan. Nadie sabe qué puede escribir la sesión si no pregunta,
por eso el rechazo es la respuesta segura.

Lo que el codec devuelve a cada CLI. `crates/adapters/src/session.rs` comparte
las dos formas de parada y de contexto:

| respuesta | evento | forma |
|---|---|---|
| `refuse` | `before-write` | código 2; el motivo en stderr |
| `refuse` | `before-stop` | stdout `{"decision":"block","reason":…}`; código 0 |
| `tell` | `after-write` | stdout `{"hookSpecificOutput":{"hookEventName":"PostToolUse","additionalContext":…}}`; código 0 |
| `proceed` | todos | código 0; sin salida |

Codex lee un rechazo con stderr vacío como un hook roto y corre el parche de
todos modos. Por eso su codec nunca deja el motivo vacío: pone `yunta: write
refused`.

> **Superado por [D266](../adr/D266-the-rail-owns-its-windows-answers-and-failures.md).** Las ventanas viven en `core::rail::window`: el engine responde a un hook de escritura dentro de 7 s desde el arranque del proceso, y la CLI corta ese hook a los 10 s (`WRITE_HOOK_CLI_MS`). `HOOK_TIMEOUT_SECONDS` y el plazo de 8 s desde la pregunta ya no existen. El hook de parada sigue esperando hasta el límite de la sesión, con un `timeout` de al menos un segundo.

Tiempos. La CLI corta su hook de escritura a los 10 s (`HOOK_TIMEOUT_SECONDS`);
el hook se rinde a los 8 s y rechaza antes que la CLI. Si un hook no responde a
tiempo, la CLI deja pasar la llamada, y el post-check diff la atrapa. El hook de
parada espera al engine hasta el límite de la sesión
(`RunToolsEndpoint::call_timeout`: el tiempo del presupuesto, o una hora).

---

## 5. El engine

- **Al abrir la sesión** (`crates/engine/src/run/session_plan.rs`).
  `Fence::for_session` recibe el perfil, el alcance de la tarea o del nodo, el
  directorio de archivos declarados y el consejo. Después, `denying` con lo que
  ninguna sesión del run escribe: lo que el proyecto niega a toda corrida
  (`permissions.paths.deny`) y los tests que una persona aprobó; `naming` con
  cómo la CLI nombra las herramientas del run; y `sharing` con las raíces que
  comparte el run, salvo en `read_only`. El consejo es `RequestExpansion` cuando
  la sesión tiene herramientas del run y puede pedir alcance: siempre en una
  tarea, y en un nodo cuando una persona puede ampliar su alcance. En los demás
  casos es `ReportFinding`. El cerco entra en el riel de la sesión
  (`SessionRails::fence`) antes de despachar; el primero que llega gana.
- **Antes de escribir** (`crates/engine/src/run_tools/rail.rs`, `before_write`).
  Juzga cada ruta con `Fence::judge`. Por cada rechazo registra `write_refused`
  antes de responder; la ruta se guarda relativa al worktree, cuando está bajo
  él. La respuesta es `refuse` con los textos de rechazo unidos por una línea en
  blanco. Si ninguna ruta se rechaza, `proceed`. *Superado por [D266](../adr/D266-the-rail-owns-its-windows-answers-and-failures.md): el registro va bajo la sesión que atiende el listener, no bajo la que nombra el cuerpo, y una llamada que no puede juzgarse queda como `rail_failed` con su causa.*
- **Después de escribir** (`crates/engine/src/run_tools/format.rs`). De las rutas
  que el cerco permite y que son archivos, corre el comando de cada regla
  `format:` que las cubre y registra `write_formatted`. Responde `tell` solo si
  algo cambió o si el comando falló, para que la sesión vuelva a leer el archivo
  antes de editarlo. *Superado por [D266](../adr/D266-the-rail-owns-its-windows-answers-and-failures.md): los formateadores de una escritura comparten una cota de 5 s (`FORMAT_MS`); el que sigue corriendo al llegar a ella se corta, con `exit_code` -2 y `timed_out`; la salida se guarda redactada y `write_formatted` la lleva por su hash.*
- **Antes de parar** (`crates/engine/src/run_tools/stop.rs`). El trabajo de una
  sesión de tarea se juzga como lo juzga `yunta_check_task`; el de una sesión de
  nodo, como lo juzga su cierre: los documentos que entrega y su alcance. Si el
  cierre no tomaría el trabajo, el engine registra `stop_refused` y responde
  `refuse` con lo que falta.
  Un mismo estado del trabajo se devuelve una sola vez. Tampoco se devuelve
  mientras el fin del intento le debe a la sesión una respuesta sobre su alcance
  (una ampliación que pidió, o un rechazo que un criterio rojo localiza), ni
  cuando la sesión se está cortando: en esos casos la parada pasa.
- **Post-check.** `scope_check` no cambia. `fence_breach`
  (`crates/engine/src/scope.rs`) decide el caso exacto: bajo `Coverage::Exact`,
  una violación en el diff es además un hallazgo del engine (`engine-fence-breach`,
  severidad mayor), además de la falla de la tarea. Bajo `widened_to_roots` o
  `tools_only`, la violación es solo la falla de siempre. *Superado por [D265](../adr/D265-a-breach-is-filed-where-the-recorded-confinement-leaves-the-checkout-to-the-fence-alone.md): la brecha se registra según la confinación guardada, no según `Coverage::Exact`.*

---

## 6. Adapters builtin

Reglas comunes:

- Un adapter con nivel `tool_calls` no abre una sesión sin riel. Si falta el
  comando o el endpoint, falla antes de lanzar la CLI con
  `FenceUnbuildable(HookUnavailable)`.
- Ningún adapter lee `artifact_dir` como permiso. `--add-dir` y
  `writable_roots` salen de `fence.roots`.
- Los hooks del run se cargan con `cli_config` `inherit` o `isolated`. Los
  archivos de configuración de la CLI que el adapter declara en `own_config` no
  los escribe ninguna sesión (D250).

### claude-code: `FenceLevel::ToolCalls`

- **Instala.** Con `--settings '<json>'` en la línea de comando va el JSON de los
  hooks: `PreToolUse` con el matcher `Edit|Write|MultiEdit|NotebookEdit`
  (`before-write`, 10 s); `Stop` (`before-stop`, con el límite de la sesión); y
  `PostToolUse` con el mismo matcher (`after-write`), solo cuando el engine
  formatea. El mismo JSON lleva el `sandbox` de los comandos. Cada raíz llega con
  `--add-dir`, y `YUNTA_RAIL_URL` y `YUNTA_RAIL_TOKEN` van en el entorno del hijo.
- **Perfiles.** `edit`: lectura, `Bash`, `Edit`, `Write`, `MultiEdit` y
  `NotebookEdit`; los comandos corren bajo el sandbox `workspace-write`, o el que
  diga `adapter_settings.sandbox`. `read_only`: lectura y `Bash` bajo un sandbox
  `read-only` que niega escribir el worktree; `Write` y `Edit` solo si la sesión
  tiene archivos declarados. `full`: todas las herramientas, sin sandbox.
- **Codec** (`crates/adapters/src/claude_code/rail.rs`). La llamada es el JSON del
  hook: `session_id` y, en una escritura, `tool_input.file_path`, o
  `tool_input.notebook_path` en `NotebookEdit`. Sin ninguno de los dos, el codec
  no la lee y el riel responde `unasked`. *Superado por [D266](../adr/D266-the-rail-owns-its-windows-answers-and-failures.md): una llamada que no se lee es una falla `decode`, y la negativa lleva su causa.*
- **Cobertura.** Las herramientas son `exact`. Los comandos: con `read_only` y sin
  raíces, `exact` (la sandbox no deja escribir nada); con `read_only` y raíces,
  las raíces; con `edit`, el worktree y las raíces; con `full`, nada
  (`tools_only`).
- **Parser.** No lee rechazos de su stream. Los rechazos del riel los registra el
  engine.

### codex: `FenceLevel::ToolCalls`

- **Instala.** Con `-c` (`ConfigOverride`): el hook `PreToolUse` con el matcher
  `apply_patch` (`before-write`), `Stop` (`before-stop`) y `PostToolUse`
  (`after-write`), solo cuando el engine formatea. Cada hook queda confiado por su
  propio hash en `hooks.state`: el adapter lo calcula como lo calcula Codex, en la
  misma llamada, y no pasa `--dangerously-bypass-hook-trust` (D251). Además,
  `--sandbox <modo>` y `sandbox_workspace_write.writable_roots` con las raíces.
- **Perfiles.** `read_only` sin raíces: `read-only`. `read_only` con raíces:
  `workspace-write`, y el riel rechaza toda edición que no caiga en una raíz.
  `edit`: el modo de `adapter_settings.sandbox`, por defecto `workspace-write`.
  `full`: `danger-full-access`.
- **Codec** (`crates/adapters/src/codex/rail.rs`). La llamada lleva el parche en
  `tool_input.command`. `patch_paths` saca las rutas de las cabeceras
  `*** Add File:`, `*** Update File:`, `*** Delete File:` y `*** Move to:`. Un
  parche se pregunta entero: una sola ruta fuera del cerco rechaza el parche
  completo. Un parche que no nombra ningún archivo no se lee.
- **Cobertura.** Las ediciones son `exact`. Los comandos: con `read-only`,
  `exact`; con `workspace-write`, el worktree y las raíces; con
  `danger-full-access`, nada.
- **Parser** (`crates/adapters/src/codex/parse.rs`). Un comando con estado
  `sandbox_denied` sale como `write_refused`, con el comando como objetivo
  opaco. La versión medida de Codex (0.157.0) no emite ese estado.

### mock: nivel del fixture

- `capabilities: { fence: none | tool_calls | filesystem }` y, opcional,
  `fence_coverage: exact | widened | tools_only`. Sin `fence_coverage`, `tool_calls`
  reporta `exact` y `filesystem` reporta `widened`.
- Con `tool_calls` y riel, cada efecto del fixture se pregunta al engine por el
  riel, como lo haría una CLI. Sin riel, o con otro nivel, `Fence::judge` decide
  en proceso antes de escribir; con `filesystem`, el juicio usa
  `Fence::everything(roots)`. Un efecto rechazado no se escribe y queda registrado.

### Límites declarados

- **No es un sandbox.** Un comando, un script o un servidor MCP escriben sin
  pasar por el riel. Lo que cubre el canal de comandos lo dice la cobertura, y el
  post-check por diff es la garantía.
- **Rutas no nombradas.** El juicio cubre lo que el codec lee de la llamada. Una
  herramienta que escribe una ruta que la llamada no nombra no se juzga.
- **Symlinks.** El juicio es léxico: un symlink que apunta fuera del worktree no
  se sigue. El post-check diff lo atrapa.
- **Tiempo.** Si un hook no responde a tiempo, la CLI deja pasar la llamada. El
  post-check diff la atrapa. *Superado por [D266](../adr/D266-the-rail-owns-its-windows-answers-and-failures.md) en la ventana: el engine niega una escritura a los 7 s, antes del corte de 10 s de la CLI, y la negativa lleva su causa.*
- **Sandbox de la CLI.** Una negativa del sandbox de la CLI no pasa por el riel.
  El parser de codex la registra solo si la CLI la informa en su stream, y la
  versión medida no la informa.

---

## 7. Adapters futuros

Un CLI nuevo entra con el mismo riel si cumple lo que cumplen los dos adapters
de arriba:

1. **Tres niveles alcanzan.** `tool_calls` cuando su hook corre antes de cada
   escritura de archivo, sea un hook, una extensión o un programa delegado;
   `filesystem` cuando lo que confina es un sandbox por directorio; `none` cuando
   no hay ninguno de los dos.
2. **El juez es uno.** El adapter escribe el codec y la configuración que instala
   el hook. Las reglas declarativas del CLI (un `write(<glob>)` o un
   `permission.edit`) son una segunda línea, nunca la fuente de la cobertura: su
   glob no es el del juez.
3. **El engine registra el rechazo.** El codec traduce la respuesta; no lee
   rechazos de la salida del CLI (D249).
4. **La configuración de la sesión va bajo `scratch_dir`.** Lo que la CLI solo lee
   del árbol del proyecto se planta bajo `cwd` y se declara en `staged_paths`,
   como los skills.
5. **La cobertura se calcula, no se declara.** `Coverage::of` la fija en core a
   partir de lo que el adapter construyó.

La muestra de ocho CLIs del mercado (Gemini, Copilot, Cursor, OpenCode, Aider,
Goose, Amp y Kimi) está en `auditoria/09-mercado-de-clis.md`, con lo no verificado
marcado como tal. Ninguno tiene adapter en este repositorio.

---

## 8. Archivos

- **Cerco y protocolo (core):** `crates/core/src/fence.rs` (cerco, juez,
  cobertura, texto del rechazo); `crates/core/src/rail.rs` (eventos, llamada,
  respuesta, `unasked`, comando, endpoint y la pregunta por loopback);
  `crates/core/src/port/session.rs` (`SessionRequest`, `AgentEvent`,
  `RailCodec`, `HookReply`); `crates/core/src/capabilities.rs` (`FenceLevel`,
  `Capability::Fence`); `crates/core/src/port/policy.rs` (la política de
  `Capability::Fence`); `crates/core/src/error.rs` (`Unbuildable::HookUnavailable`);
  `crates/core/src/config/env.rs` (lee `YUNTA_RAIL_URL` y `YUNTA_RAIL_TOKEN` en el
  borde); `crates/core/src/events/session/` (`write_refused`, `stop_refused`,
  `write_formatted`).
- **Hook (cli):** `crates/cli/src/cli/mod.rs` (el subcomando oculto);
  `crates/cli/src/cli/dispatch.rs` (`rail_hook`); `crates/cli/src/commands/rail.rs`
  (decodifica, pregunta, codifica); `crates/cli/src/context.rs` (el `RailCommand`
  que el binario entrega a las sesiones).
- **Juicio (engine):** `crates/engine/src/run_tools/rail.rs` (`before-write`);
  `crates/engine/src/run_tools/format.rs` (`after-write`);
  `crates/engine/src/run_tools/stop.rs` (`before-stop`);
  `crates/engine/src/run_tools/listener.rs` (la ruta `/rail/<evento>` detrás del
  mismo token que `/mcp`); `crates/engine/src/run/session_plan.rs` y
  `crates/engine/src/run/session_plan/held_to.rs` (el cerco de cada sesión y el
  alcance del que sale); `crates/engine/src/scope.rs` (`fence_breach`).
- **Adapters:** `crates/adapters/src/claude_code/{mod,rail,permissions}.rs`;
  `crates/adapters/src/codex/{mod,rail,fence,parse,config}.rs`;
  `crates/adapters/src/session.rs` (las formas de respuesta de los hooks);
  `crates/adapters/src/mock/{mod,effects}.rs`.

---

## 9. Tests

| test | archivo | prueba |
|---|---|---|
| `a_write_outside_every_glob_is_refused_naming_the_target` | `crates/core/tests/fence.rs` | regla 3; el objetivo del rechazo es el path normalizado |
| `a_write_into_dot_git_is_refused` | `crates/core/tests/fence.rs` | regla 5, `.git` y `.git/` |
| `a_relative_target_is_judged_against_the_worktree` | `crates/core/tests/fence.rs` | regla 1 |
| `a_dot_dot_escape_is_refused_without_touching_disk` | `crates/core/tests/fence.rs` | reglas 1 y 3; el path no existe |
| `an_empty_allowed_set_refuses_every_worktree_write_and_keeps_the_roots` | `crates/core/tests/fence.rs` | `read_only` |
| `a_granted_expansion_is_inside_the_fence` | `crates/core/tests/fence.rs` | `for_session` con `granted` |
| `a_refusal_message_names_its_target_and_says_its_advice` | `crates/core/tests/fence.rs` | el texto del rechazo, con sus dos consejos |
| `coverage_is_the_weakest_channel` | `crates/core/tests/fence.rs` | `Coverage::of` |
| `a_denied_path_is_refused_where_the_scope_allows_it` | `crates/core/tests/fence.rs` | `denied` dentro del alcance |
| `an_unscoped_fence_still_refuses_denied_paths` | `crates/core/tests/fence.rs` | `denied` sin alcance declarado |
| `write_refused_is_a_session_kind_that_moves_the_ledger` | `crates/core/tests/events.rs` | el kind y su pliegue en el ledger |
| `an_old_log_without_a_fence_level_reads_as_none` | `crates/core/tests/events.rs` | un log con `edit_hooks` |
| `a_refused_write_is_exit_two_with_the_engine_s_reason_on_stderr` | `crates/cli/tests/rail_cmd.rs` | el hook con su codec, código 2 y motivo |
| `a_hook_that_cannot_ask_the_engine_refuses` | `crates/cli/tests/rail_cmd.rs` | `unasked` en `before-write` |
| `a_codex_patch_with_one_path_outside_is_refused_whole` | `crates/cli/tests/rail_cmd.rs` | un parche con varias rutas |
| `a_refused_stop_is_a_block_decision_on_stdout_for_every_adapter` | `crates/cli/tests/rail_cmd.rs` | parada devuelta, en stdout |
| `a_stop_hook_that_cannot_ask_lets_the_session_stop` | `crates/cli/tests/rail_cmd.rs` | `unasked` en `before-stop` |
| `a_red_stop_is_sent_back_once_and_the_same_session_closes_its_task` | `crates/engine/tests/stop_judgment.rs` | la parada devuelta una vez |
| `a_stop_on_the_same_work_is_sent_back_only_once` | `crates/engine/tests/stop_judgment.rs` | un mismo estado, una sola parada |
| `a_session_whose_refused_write_its_red_output_locates_is_never_sent_back` | `crates/engine/tests/stop_judgment.rs` | la parada no se juzga cuando el intento le debe una respuesta |
| `every_session_carries_its_fence_and_read_only_allows_nothing_under_the_worktree` | `crates/engine/tests/run_sessions.rs` | el cerco de `open_session` |
| `a_claude_session_installs_the_fence_by_settings_and_add_dir_and_nothing_under_cwd` | `crates/adapters/tests/claude_code.rs` | hooks en `--settings`, `--add-dir` |
| `a_claude_session_without_a_hook_or_an_engine_to_ask_fails_before_spawning` | `crates/adapters/tests/claude_code.rs` | `HookUnavailable` antes de lanzar |
| `coverage_follows_what_the_sandbox_leaves_commands` | `crates/adapters/tests/claude_code.rs` | cobertura por modo de sandbox |
| `a_codex_session_installs_its_write_hook_trusted_and_nothing_more` | `crates/adapters/tests/codex.rs` | hooks confiados por hash |
| `a_codex_session_reports_widened_coverage_with_its_roots` | `crates/adapters/tests/codex.rs` | cobertura ampliada a raíces |
| `a_sandbox_denial_becomes_write_refused` | `crates/adapters/tests/codex.rs` | la rama del parser de codex |
| `a_fixture_effect_outside_the_fence_is_refused_and_recorded` | `crates/adapters/tests/mock.rs` | el mock por `Fence::judge` |

---

## 10. Cierra

El riel: las decisiones D249 a D251 están en el registro, y la deuda A-13 queda
cerrada en `deuda-consciente.md`.
