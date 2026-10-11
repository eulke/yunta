# Spec — Trait Adapter (v0.2): interfaces engine ↔ CLIs

**Estado:** normativo v0.2 · **Alcance:** define la frontera entre el engine y los
CLIs de agentes. Todo lo que Yunta sabe de Claude Code, Codex o cualquier CLI futuro
pasa por esta interfaz; el engine no contiene ningún conocimiento específico de
ninguno. Complementa al Contrato del Run: los adapters son los únicos emisores de
eventos que no son el engine, y este documento fija sus obligaciones.

## 1. Rol y filosofía

Un `Adapter` convierte una **solicitud de sesión** (prompt, worktree, contexto
montado, permisos, presupuesto) en un **stream de eventos tipados**. Nada más. El
adapter no interpreta el workflow, no evalúa criterios, no decide reintentos: ejecuta
una sesión y reporta lo que ocurre. Toda inteligencia de orquestación queda del lado
del engine; así, agregar un CLI nuevo es implementar una traducción, no re-entender
Yunta.

Los adapters son heterogéneos: no todo CLI soporta reanudar sesiones, instalar hooks
de edición o reportar costos. La heterogeneidad se maneja con **capacidades
declaradas**: el adapter dice qué sabe hacer, el engine consulta antes de pedir, y la
ausencia de una capacidad produce degradación explícita (evento + warning), jamás un
fallo silencioso ni una emulación a medias.

## 2. Tipos e interfaz

```rust
use futures::stream::BoxStream;

/// Identificador de sesión del CLI subyacente, opaco para el engine.
/// Se persiste en el event log (`agent_session_opened`) y solo se
/// reutiliza pasándoselo de vuelta al mismo adapter.
pub struct SessionId(String);

#[derive(Clone, Copy, Debug, Default)]
pub struct Capabilities {
    /// Puede reanudar una conversación previa vía `resume()`.
    pub resume_session: bool,
    /// Qué puede construir este adapter para mantener las escrituras
    /// de una sesión dentro de su cerco. `None` es nada: el diff del
    /// post-check es lo único que atrapa una escritura fuera del scope.
    /// Un log escrito antes del cerco lleva `edit_hooks` y se lee como
    /// `None` (D172).
    pub fence: FenceLevel,   // None | ToolCalls | Filesystem
    /// Distingue perfiles de permisos (read_only / edit / full).
    /// Sin esta capacidad, todo nodo corre con los permisos del CLI
    /// y los nodos que exigen `read_only` fallan en `yunta check`.
    pub permission_profiles: bool,
    /// Puede sumar las instrucciones de un agente (`SessionRequest.agent`)
    /// a las instrucciones de sistema de su CLI.
    pub custom_agents: bool,
    /// Emite uso de tokens confiable en el stream. Si el CLI además
    /// distingue tokens leídos de caché, se reportan en `Usage`
    /// (habilita la tasa de cache de Contrato §8.4).
    pub usage_reporting: bool,
    /// Puede montar directorios de skill (`SessionRequest.skills`) por
    /// el mecanismo nativo del CLI. Una skill es instrucción agregada,
    /// nunca corrección: su ausencia degrada con `capability_degraded`,
    /// jamás falla.
    pub skills: bool,
    /// Puede conectarse al servidor MCP por-run de Yunta como cliente
    /// (blackboard, tareas, findings, solicitud de ampliación de scope).
    pub run_tools: bool,
    /// Puede confinar los comandos de la sesión sin acceso a red, que es
    /// lo que hace cumplir el `network: false` de un nodo (`network` del
    /// pedido). Donde esta capacidad falta, `network: false` degrada con
    /// `capability_degraded` — queda registrado para política y
    /// auditoría, nunca exigido (D252).
    pub network_isolation: bool,
}

pub struct SessionRequest {
    /// Prompt ya renderizado por el engine (templates resueltos).
    pub prompt: String,
    /// Directorio de trabajo: el worktree del run.
    pub cwd: PathBuf,
    /// Contexto materializado por el engine (Contrato §9):
    /// paths a montar/referenciar + el bloque inline si lo hay.
    pub context: ResolvedContext,
    /// Skills a exponer al agente (paths a directorios de skill).
    pub skills: Vec<PathBuf>,
    /// Modelo pedido por el runner (el adapter lo traduce o rechaza).
    pub model: Option<String>,
    /// El agente que pidió el nodo o el runner, ya resuelto por el engine
    /// a su definición (`.yunta/agents/<nombre>.md` o la de un pack): su
    /// nombre y sus instrucciones. El adapter solo se las entrega a su
    /// CLI, sumadas a las propias; nunca busca un agente por su cuenta
    /// (D253).
    pub agent: Option<AgentDefinition>,   // { name, instructions }
    /// Cuánto razona el modelo (`low | medium | high | xhigh`), el del nodo
    /// sobre el del runner; `None` lo deja al CLI. Todo adapter lo entrega,
    /// también al retomar: `claude-code` como `--effort`, `codex` como
    /// `-c model_reasoning_effort`.
    pub effort: Option<Effort>,
    /// Perfil de permisos del nodo.
    pub permissions: PermissionProfile,   // ReadOnly | Edit | Full
    /// Env vars para la sesión. Los secretos llegan SOLO por acá,
    /// ya resueltos por el engine desde el manifest (I12), envueltos en
    /// `Secret`: `Debug` imprime `[redacted]` y el valor se expone una
    /// sola vez, al construir el entorno del hijo.
    pub env: HashMap<String, Secret<String>>,
    /// Lo que esta sesión puede escribir: la única fuente de permiso de
    /// escritura, siempre presente. `allowed` es `None` cuando el nodo
    /// no declaró scope (todo bajo el worktree) y `Some([])` bajo
    /// `read_only` (nada bajo el worktree; las raíces siguen escribibles).
    /// El adapter construye tanto de él como su CLI permita y reporta
    /// cuánto fue; uno que no puede construir nada lo ignora, sin fallar
    /// — el engine ya degradó y avisó (D172).
    pub fence: Fence,   // { allowed: Option<Vec<ScopeGlob>>, roots: Vec<PathBuf>, advice }
    /// El comando que los hooks de un CLI ejecutan para preguntarle al
    /// engine — antes de escribir, primero que nada. `None` solo en un
    /// arnés sin binario que correr; un adapter cuyo cerco lo necesita y
    /// no lo tiene falla la sesión con
    /// `AdapterError::FenceUnbuildable(HookUnavailable)`.
    pub rail_command: Option<RailCommand>,
    /// Dónde esos hooks encuentran al engine: la ruta `/rail` del listener
    /// de esta sesión, con el mismo token que sus herramientas (D249).
    pub rail: Option<RailEndpoint>,
    /// Endpoint del MCP por-run de Yunta, si `run_tools`. Forma
    /// concreta (HTTP loopback + token bearer, ciclo de vida por
    /// sesión) en Contrato §6.5 — el adapter solo debe traducirlo
    /// al mecanismo nativo de su CLI para conectarse a un servidor
    /// MCP externo, nunca inventar su propio transporte.
    pub run_tools_endpoint: Option<Endpoint>,
    /// Presupuesto duro de la sesión. El adapter lo pasa al CLI si
    /// puede; el engine lo hace cumplir igual contando `Usage`.
    pub budget: Budget,                   // max_tokens, max_turns, timeout
    /// Settings específicos del adapter, opacos para el engine,
    /// declarados en el runner (p. ej. flags de sandbox). Solo para
    /// lo que NO tiene expresión portable: la selección de agente
    /// va en `agent:`, jamás acá. El adapter valida en `probe()`
    /// lo que pueda validarse y rechaza lo que no reconozca.
    pub adapter_settings: serde_json::Map<String, serde_json::Value>,
}

#[async_trait]
pub trait Adapter: Send + Sync {
    /// Identidad del adapter ("claude-code", "codex", "mock", ...).
    fn id(&self) -> &'static AdapterId;

    /// Capacidades. Debe ser constante durante el proceso; el engine
    /// la consulta en `yunta check` y al planificar cada nodo.
    fn capabilities(&self) -> Capabilities;

    /// Chequeo de salud: binario presente, versión compatible,
    /// autenticación válida. Corre en `yunta doctor` y al crear runs.
    async fn probe(&self) -> Result<ProbeReport>;

    /// Rutas del worktree (relativas) que una sesión abierta para `req`
    /// escribe por su propia mecánica — un mount, un archivo de settings —,
    /// nunca trabajo del agente. El scope del engine excluye exactamente
    /// esas rutas y ninguna otra. Default: ninguna.
    fn staged_paths(&self, req: &SessionRequest) -> Vec<PathBuf>;

    /// Archivos del checkout de los que la CLI del adapter toma configuración
    /// que ejecuta código o monta herramientas (sus settings con hooks, sus
    /// servidores MCP). Ninguna sesión de ningún run los escribe. Default: ninguno.
    fn own_config(&self) -> Vec<ScopeGlob>;

    /// Retira lo que una sesión abierta para `req` montó por su propia
    /// mecánica, una vez terminada: nunca fue trabajo del agente, y no
    /// debe quedar en el árbol que el run conserva. Default: nada.
    fn unstage(&self, req: &SessionRequest) -> Result<()>;

    /// Cómo traduce este adapter entre los hooks de su CLI y el riel del engine.
    /// `Some` exactamente cuando `capabilities().fence` es `ToolCalls`: un cerco por
    /// sandbox no necesita codec. Default: `None`.
    fn rail_codec(&self) -> Option<&dyn RailCodec>;

    /// Abre una sesión nueva. Contrato de eventos en §4.
    async fn spawn(&self, req: SessionRequest) -> Result<Box<dyn AgentSession>>;

    /// Variables de entorno con las que la CLI del adapter alcanza su servicio
    /// (un proxy, una URL base), que la shell captura para `reachable`. Default: ninguna.
    fn service_vars(&self) -> &'static [&'static str];

    /// Si el servicio con el que habla una sesión abierta para `req` responde desde
    /// esta máquina ahora; `None` cuando el adapter no puede saber dónde está. `shell`
    /// son las variables que la shell tenía para `service_vars`. Default: no puede saberlo.
    async fn reachable(
        &self,
        req: &SessionRequest,
        shell: &BTreeMap<String, String>,
    ) -> Option<bool>;

    /// Reanuda una conversación previa. Default: no soportado.
    /// Solo se invoca si `capabilities().resume_session`.
    async fn resume(
        &self,
        session: &SessionId,
        req: SessionRequest,
    ) -> Result<Box<dyn AgentSession>>;
}

#[async_trait]
pub trait AgentSession: Send {
    /// Stream de eventos de la sesión. Termina con exactamente un
    /// `Completed` o un `Failed`; nada puede seguir después.
    fn events(&mut self) -> BoxStream<'_, AgentEvent>;

    /// Pedido de terminación ordenada (equivale a Esc/SIGINT):
    /// el agente puede cerrar limpio; si no lo hace en el plazo
    /// configurado, el engine escala a `kill`.
    async fn interrupt(&mut self) -> Result<()>;

    /// Terminación forzosa de TODO el árbol de procesos de la
    /// sesión (process group / job object). Nunca deja zombies:
    /// es la garantía sobre la que se construye `yunta cancel`.
    async fn kill(&mut self) -> Result<()>;

    /// Da al proceso de la sesión hasta `grace` para salir solo después de un
    /// `interrupt`, y responde en cuanto sale. Default: responde de inmediato.
    async fn allow_exit(&mut self, grace: Duration);

    /// Grupo de procesos (pgid) del árbol de la sesión, cuando el adapter corre uno:
    /// lo que el engine registra en `run.dir/scratch/engine.json` para que un proceso
    /// aparte (`yunta cancel` tras un crash) pueda exterminarlo. Default: `None`.
    fn pgid(&self) -> Option<Pid>;

    /// Cómo terminó el proceso de la sesión. Solo se pregunta a una sesión cuyo
    /// stream terminó sin evento terminal. Default: `None`.
    async fn exit(&mut self) -> Result<Option<SessionExit>>;
}
```

## 3. Eventos del adapter

```rust
pub enum AgentEvent {
    /// OBLIGATORIO como primer evento de toda sesión. `model` es el que
    /// el CLI reportó; ausente cuando no reporta ninguno — nunca el pedido.
    /// `fence` es cuánto de la sesión cercó realmente lo que el adapter
    /// construyó, calculado —nunca declarado—; ausente cuando no
    /// construyó ninguno.
    SessionOpened { session_id: SessionId, model: Option<ModelName>, fence: Option<Coverage> },
    /// Una escritura que el cerco rechazó antes de ocurrir. Crónica y
    /// conteo, nunca estado del nodo: la sesión siguió.
    WriteRefused { target: ToolTarget },
    /// Cuántas herramientas del run tiene de verdad la sesión, según las
    /// reportó su CLI. Solo lo emite un adapter cuyo CLI nombra el conjunto
    /// de herramientas de la sesión: el silencio significa que no lo sabe,
    /// nunca que el conteo sea cero.
    RunToolsMounted { count: usize },
    /// Falló una herramienta conocida del servidor `yunta-run` de la sesión.
    /// Ningún argumento ni texto de error del CLI cruza esta frontera.
    RunToolFailed { tool: RunTool, cause: RunToolFailureCause },
    /// Actividad resumida: qué herramienta usó, sobre qué. `display`
    /// solo cuando el argumento nombra el repositorio; el digest,
    /// siempre. Nunca contenido completo ni secretos (§4, O3).
    ToolUse { name: String, target: ToolTarget },
    /// Uso acumulable de tokens. Frecuencia: al menos al cierre;
    /// idealmente incremental. Requerido si `usage_reporting`. Cada cuenta
    /// es opcional: ausente significa no informada, nunca cero.
    /// `input_tokens` es todo lo que la llamada leyó, caché incluida, y
    /// `cached_input_tokens` la parte leída de la caché —se puebla solo si
    /// el CLI distingue lectura de caché (Contrato §8.4)—, así que
    /// `cached_input_tokens ≤ input_tokens` en todo adapter.
    /// `claude-code` informa cada mensaje una vez y concilia el resto con
    /// el `result`; `codex`, cada `turn.completed`.
    Usage { input_tokens: Option<u64>, output_tokens: Option<u64>, cached_input_tokens: Option<u64> },
    /// Texto final o resumen de progreso significativo (acotado).
    Note { text: String },
    /// Cierre exitoso de la sesión (el agente terminó su turno).
    Completed { result: AgentOutcome },
    /// Cierre con error. `cause` dice si otra sesión con el mismo pedido puede
    /// hacerlo mejor: el engine lo registra como `retryable`.
    Failed { error: AgentError, cause: FailureCause },
}
```

`AgentOutcome` transporta lo que el agente reportó, y nada de eso es veredicto: el
engine corre criterios, scope y artifacts igual (Contrato §5.2). El outcome es
telemetría, no evidencia.

## 4. Obligaciones del adapter (normativas)

- **O1. `SessionOpened` primero, siempre**: sin session_id en el log no hay
  resumibilidad de nivel nodo; un adapter que no puede obtenerlo emite un id
  sintético propio y declara `resume_session: false`.
- **O2. Terminación única**: el stream termina con exactamente un `Completed` o
  `Failed`. Un stream que termina sin ninguno de los dos no es un error del
  agente: el adapter nunca inventa un terminal, el engine le pregunta a esa
  sesión —y sólo a esa— cómo salió su proceso (`AgentSession::exit`) y registra
  la muerte con esa salida (`SessionDeath`: el adapter y cómo salió el proceso).
  Preguntar es matar primero: el grupo muere, las cañerías se cierran y recién
  después se recoge la salida, de modo que la espera está acotada y nada
  sobrevive al run. Una sesión sin proceso propio responde `None` (D180).
- **O3. Nada sensible en eventos**: los payloads llevan digests y resúmenes, jamás
  contenido de archivos, prompts completos ni valores de env. El engine además
  redacta todo valor de secreto conocido antes de persistir (I12): defensa en
  profundidad, no permiso para descuidarse.
- **O4. El prompt viaja por stdin**: el adapter escribe el prompt en la entrada
  estándar del CLI y cierra el pipe; nunca lo pasa como argumento, donde cualquier
  proceso de la máquina lo leería en la lista de procesos. El CLI que termina antes
  de leerlo cierra el pipe de su lado y su stream lo reporta (O2); la escritura no
  es un fallo del adapter.
- **O5. Presupuesto colaborativo, enforcement del engine**: el adapter pasa los
  límites al CLI si el CLI los soporta; el engine corta por `interrupt → kill`
  cuando el conteo de `Usage` o el timeout lo exigen, tenga o no ayuda del CLI. El
  timeout cuenta tiempo en que el host está despierto: una máquina suspendida no lo
  gasta, y el log registra la suspensión.
- **O6. El cerco se construye o se declara ausente**: el adapter instala tanto del
  cerco como su CLI permita antes de la primera escritura posible, calcula la
  cobertura de lo que construyó (nunca la declara) y la reporta en `SessionOpened`;
  el riel del engine registra `write_refused` cuando juzga una escritura y la
  rechaza (`crates/engine/src/run_tools/rail.rs`). El parser de Codex traduce un
  ítem `command_execution` con estado `sandbox_denied` a `WriteRefused`, con el
  comando como objetivo; Codex 0.157.0 no emite ese estado cuando su sandbox niega
  un comando, y Claude no informa ninguna negativa de su sandbox. Un adapter con
  nivel `None` ignora el campo sin error. Un adapter que
  necesita un mecanismo que no tiene —el hook, o raíces escribibles bajo un sandbox
  de solo lectura— falla la sesión antes de spawn con `AdapterError::FenceUnbuildable`,
  porque una capacidad ausente falla en vez de gastar la sesión (D172).
- **O7. Sin estado propio**: un adapter no persiste nada entre sesiones fuera de lo
  que el CLI ya persiste. Todo lo que el engine necesita recordar viaja en eventos.

## 5. Degradación

El engine resuelve capacidad requerida vs declarada **en dos momentos**:
estáticamente en `yunta check` (un workflow que exige `permissions: read_only` con
un adapter sin `permission_profiles` es error de validación, no sorpresa en runtime)
y dinámicamente al despachar cada nodo. La degradación dinámica emite un evento
`capability_degraded` con la capacidad, el adapter y la política aplicada:

| Capacidad ausente | Política |
|---|---|
| `resume_session` | `on_interrupt: resume_session` degrada a `restart_node` · warning |
| `fence` | scope solo post-check + warning, una vez por run |
| `permission_profiles` | nodo `read_only` → error en check; `edit`/`full` corren con permisos del CLI |
| `custom_agents` | runner con `agent:` sobre este adapter, o nodo con `agent:` que lo use → error en check, nunca ignorado. Un `agent:` sin definición falla el nodo antes de abrir la sesión |
| `usage_reporting` | presupuesto de tokens no exigible → solo timeout y max_turns; warning por run |
| `skills` | la sesión corre sin las skills que el nodo declara · `capability_degraded` |
| `run_tools` | un nodo que declara un artifact interpretado, que está en un grupo `coordination: blackboard` o que es un `loop` no tiene otra puerta, y correrlo sin lo que declaró sería emular en silencio: `yunta check` lo rechaza cuando ningún candidato de su runner declara la capacidad, y un run que igual llegue rechaza la sesión. El resto de los nodos corre; sus findings llegan sólo por artifacts |
| `network_isolation` | `network: false` queda registrado para política y auditoría, no exigido · `capability_degraded` |
| `run_tools` montado y sin efecto | el adapter reporta cuántas tools del servidor por sesión trae la sesión; cero con endpoint entregado emite `capability_degraded` al abrir la sesión, antes de gastarla. Un CLI que no nombra sus tools (`codex`) no reporta nada: el listener de la sesión sabe si las listó o llamó alguna —los dos CLIs las listan antes de su primer turno—, y una sesión que abrió y terminó sin hacerlo emite la misma degradación al cerrar |

## 6. Adapters builtin

Cada adapter construido declara sus capacidades como una tabla fija: es lo que
`yunta check` juzga contra el `permissions:` y el `agent:` de cada nodo, y lo que
el engine consulta antes de pedir nada (§5).

### `claude-code`

| capacidad | valor |
|---|---|
| `resume_session` | `true` |
| `fence` | `tool_calls` |
| `permission_profiles` | `true` |
| `custom_agents` | `true` |
| `usage_reporting` | `true` |
| `skills` | `true` |
| `run_tools` | `true` |
| `network_isolation` | `true` |

Lanza `claude -p` headless con salida en streaming JSON y traduce ese stream a
`AgentEvent`; `resume` reutiliza la sesión vía el flag de reanudación del CLI con el
`SessionId` persistido; el cerco es `FenceLevel::ToolCalls`: un hook `PreToolUse`
sobre cada herramienta de escritura ejecuta `yunta rail claude-code before-write`, que
le pregunta al engine por el listener de la sesión, y uno `Stop` ejecuta
`yunta rail claude-code before-stop`, cuya negativa es `{"decision":"block","reason":…}`
y para el que el adapter apaga el tope del CLI (`CLAUDE_CODE_STOP_HOOK_BLOCK_CAP=0`,
D254); cuando el run formatea, uno `PostToolUse` sobre las mismas herramientas
ejecuta `yunta rail claude-code after-write` y lo que el engine dice llega al
modelo como `additionalContext` (D255); las raíces
del cerco entran por `--add-dir`. Las ediciones se cubren exactas; los comandos, por
directorio: `WidenedToRoots { [cwd, …raíces] }` bajo `edit`, `WidenedToRoots { raíces }`
bajo `read_only` con raíces y `Exact` sin ellas, `ToolsOnly` bajo `full`. `read_only`
conserva `Write`/`Edit` solo cuando el cerco tiene raíces, que es donde van los
archivos declarados. Límite declarado: si el hook no responde en su timeout, el CLI
deja pasar la llamada; el post-check la atrapa y `fence_breach` la nombra.
`custom_agents`: las instrucciones del agente llegan por
`--append-system-prompt-file`, desde un archivo del directorio de la sesión;
`permission_profiles` mapea a los modos de permisos del CLI;
skills y MCP por-run se inyectan por la configuración de sesión. Los comandos corren
en el sandbox del CLI (`sandbox` en `--settings`, que falla en vez de correr sin
confinar): bajo `edit` escriben el workspace y las raíces, bajo `read_only` solo las
raíces (`denyWrite` sobre el workspace), y bajo `full` no se confinan; la red, loopback
incluido, se abre solo cuando el pedido la abre (`allowedDomains: ["*"]`,
`allowLocalBinding`), así que `network_isolation` se declara (D252). Los detalles de flags viven en el adapter y se validan en
`probe()` contra la versión instalada; el engine no conoce ninguno.

### `codex`

| capacidad | valor |
|---|---|
| `resume_session` | `true` |
| `fence` | `tool_calls` |
| `permission_profiles` | `true` |
| `custom_agents` | `true` |
| `usage_reporting` | `true` |
| `skills` | `true` |
| `run_tools` | `true` |
| `network_isolation` | `true` |

`codex exec` headless con salida JSON; `permission_profiles` mapea a sus modos de
sandbox. El cerco es `FenceLevel::ToolCalls`: un hook `PreToolUse` sobre
`apply_patch` ejecuta `yunta rail codex before-write`, que le pregunta al engine por
cada path que el patch nombra, uno `Stop` ejecuta `yunta rail codex before-stop`
(D254) y, cuando el run formatea, uno `PostToolUse` sobre `apply_patch` ejecuta
`yunta rail codex after-write` (D255); los hooks llegan como overrides `-c` junto con su propia
confianza en `hooks.state` (el hash con que Codex confía en un hook), así que ningún
hook ajeno corre sin revisar (D251). Los comandos quedan confinados por el sandbox:
`--sandbox workspace-write` más las raíces del cerco en
`sandbox_workspace_write.writable_roots`, de modo que la cobertura es
`WidenedToRoots { [cwd, …raíces] }`; bajo `read-only` sin raíces es `Exact` y bajo
`danger-full-access`, `ToolsOnly`. Un perfil `read_only` con raíces que mantener
escribibles corre bajo `workspace-write` —`read-only` sella también las raíces— y el
hook rechaza toda edición que no sea a ellas. Límite declarado: la política partida del sandbox
(`/repo=write`, `/repo/a=none`) no se usa —no expresa globs. Las skills se montan
como en `claude-code`: un enlace por skill bajo `.agents/skills/` del directorio de la
sesión, donde el CLI descubre las de un proyecto, declarado como staging y retirado al
cerrar. `custom_agents`: las instrucciones del agente llegan como
`-c developer_instructions=…`, que Codex suma a las suyas; el modelo es el del runner,
no el de un perfil de Codex (D253). La
red de los comandos la abre el pedido (`sandbox_workspace_write.network_access`); bajo
`read-only` no hay red y bajo `danger-full-access` no se cierra, y `check` rechaza
`network: false` en un nodo `full` (D252).

### `mock`

Pieza de primera clase, no un helper de tests: reproduce sesiones desde fixtures
(guiones YAML de eventos + efectos sobre el filesystem), con fallos y latencias
inyectables, y puede simular solicitudes de ampliación de scope y posteos al
blackboard. No lleva tabla fija porque no tiene capacidades fijas: las declara el
fixture (`capabilities`, `fence_coverage`), que es lo que permite ejercitar una
capacidad ausente sin un CLI que la niegue. Cada efecto pasa por el mismo
`Fence::judge` que los adapters reales: un fixture ejercita la regla, nunca una
segunda implementación de ella. Es lo que permite testear el engine completo (ciclo
de tareas, degradación, cancelación, resume, paralelismo) en CI sin ningún LLM, y
validar workflows nuevos sin gastar presupuesto (`yunta run --adapter mock`). El
binario no lo construye para una corrida real: los fixtures entran por `yunta test`.

## 7. Invariantes

- **A1.** El engine no contiene conocimiento específico de ningún CLI; todo vive en
  adapters. Los conceptos portables (modelo, agente, permisos, presupuesto) son
  campos tipados del request; `adapter_settings` es solo para lo que no tiene
  expresión portable.
- **A2.** Las capacidades son constantes tras la construcción del adapter y el
  engine nunca pide lo no declarado.
- **A3.** Toda sesión emite `SessionOpened` primero y exactamente un evento terminal.
- **A4.** `kill` extermina el árbol de procesos completo; ningún camino de código
  deja procesos huérfanos.
- **A5.** Los eventos de adapter nunca contienen secretos ni contenido completo;
  solo digests y resúmenes.
- **A6.** La ausencia de capacidad produce error en check o degradación con evento,
  nunca emulación silenciosa.
- **A7.** El outcome del agente es telemetría; la verificación del engine corre
  siempre e íntegra.
- **A8.** `mock` implementa el trait completo y es suficiente para ejercitar todo
  camino del engine en CI.
