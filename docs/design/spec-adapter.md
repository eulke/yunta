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
    /// Puede bloquear ediciones fuera de un conjunto de globs
    /// en el momento en que ocurren (enforcement en caliente de scope).
    pub edit_hooks: bool,
    /// Distingue perfiles de permisos (read_only / edit / full).
    /// Sin esta capacidad, todo nodo corre con los permisos del CLI
    /// y los nodos que exigen `read_only` fallan en `yunta check`.
    pub permission_profiles: bool,
    /// Soporta agentes nombrados propios del CLI, seleccionables
    /// vía el campo portable `agent:` del runner.
    pub custom_agents: bool,
    /// Emite uso de tokens confiable en el stream. Si el CLI además
    /// distingue tokens leídos de caché, se reportan en `Usage`
    /// (habilita la tasa de cache de Contrato §8.4).
    pub usage_reporting: bool,
    /// Puede conectarse al servidor MCP por-run de Yunta como cliente
    /// (blackboard, tareas, findings, solicitud de ampliación de scope).
    pub run_tools: bool,
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
    /// Agente nombrado del adapter, si el runner lo pide. Campo
    /// PORTABLE: cada adapter lo traduce a su mecanismo nativo.
    /// Solo se puebla si `capabilities().custom_agents`; `probe()`
    /// valida que el agente exista en el entorno destino.
    pub agent: Option<String>,
    /// Perfil de permisos del nodo.
    pub permissions: PermissionProfile,   // ReadOnly | Edit | Full
    /// Env vars para la sesión. Los secretos llegan SOLO por acá,
    /// ya resueltos por el engine desde el manifest (I12), envueltos en
    /// `Secret`: `Debug` imprime `[redacted]` y el valor se expone una
    /// sola vez, al construir el entorno del hijo.
    pub env: HashMap<String, Secret<String>>,
    /// Globs de scope si el nodo/tarea los declara y el adapter
    /// tiene `edit_hooks`. El adapter DEBE ignorarlo (no fallar)
    /// si no declaró la capacidad: el engine ya degradó y avisó.
    pub edit_constraints: Option<Vec<Glob>>,
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
    fn id(&self) -> &'static str;

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

    /// Abre una sesión nueva. Contrato de eventos en §4.
    async fn spawn(&self, req: SessionRequest) -> Result<Box<dyn AgentSession>>;

    /// Reanuda una conversación previa. Default: no soportado.
    /// Solo se invoca si `capabilities().resume_session`.
    async fn resume(
        &self,
        session: &SessionId,
        req: SessionRequest,
    ) -> Result<Box<dyn AgentSession>> {
        Err(AdapterError::Unsupported { adapter: self.id().into(), what: "resume_session" })
    }
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
}
```

## 3. Eventos del adapter

```rust
pub enum AgentEvent {
    /// OBLIGATORIO como primer evento de toda sesión. `model` es el que
    /// el CLI reportó; ausente cuando no reporta ninguno — nunca el pedido.
    SessionOpened { session_id: SessionId, model: Option<ModelName> },
    /// Actividad resumida: qué herramienta usó, sobre qué (digest).
    /// Nunca contenido completo ni secretos (§4, O3).
    ToolUse { name: String, target_digest: String },
    /// Uso acumulable de tokens. Frecuencia: al menos al cierre;
    /// idealmente incremental. Requerido si `usage_reporting`.
    /// `cached_input_tokens` es opcional: se puebla solo si el CLI
    /// distingue lectura de caché (Contrato §8.4).
    Usage { input_tokens: u64, output_tokens: u64, cached_input_tokens: Option<u64> },
    /// Texto final o resumen de progreso significativo (acotado).
    Note { text: String },
    /// Cierre exitoso de la sesión (el agente terminó su turno).
    Completed { result: AgentOutcome },
    /// Cierre con error. `retryable` guía la política del engine.
    Failed { error: AgentError, retryable: bool },
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
  `Failed`. Muerte del proceso sin evento = el engine sintetiza
  `Failed {retryable: true}` (crash ≠ error del agente).
- **O3. Nada sensible en eventos**: los payloads llevan digests y resúmenes, jamás
  contenido de archivos, prompts completos ni valores de env. El engine además
  redacta todo valor de secreto conocido antes de persistir (I12): defensa en
  profundidad, no permiso para descuidarse.
- **O5. El prompt viaja por stdin**: el adapter escribe el prompt en la entrada
  estándar del CLI y cierra el pipe; nunca lo pasa como argumento, donde cualquier
  proceso de la máquina lo leería en la lista de procesos. El CLI que termina antes
  de leerlo cierra el pipe de su lado y su stream lo reporta (O2); la escritura no
  es un fallo del adapter.
- **O4. Presupuesto colaborativo, enforcement del engine**: el adapter pasa los
  límites al CLI si el CLI los soporta; el engine corta por `interrupt → kill`
  cuando el conteo de `Usage` o el timeout lo exigen, tenga o no ayuda del CLI.
- **O5. `edit_constraints` es best-effort declarado**: con `edit_hooks`, el adapter
  instala el bloqueo antes de la primera edición posible y reporta cada bloqueo como
  `ToolUse` con marca; sin la capacidad, ignora el campo sin error.
- **O6. Sin estado propio**: un adapter no persiste nada entre sesiones fuera de lo
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
| `edit_hooks` | scope solo post-check + warning |
| `permission_profiles` | nodo `read_only` → error en check; `edit`/`full` corren con permisos del CLI |
| `custom_agents` | runner con `agent:` sobre este adapter, o nodo con `agent:` que lo use → error en check, nunca ignorado |
| `usage_reporting` | presupuesto de tokens no exigible → solo timeout y max_turns; warning por run |
| `run_tools` | nodos que requieren blackboard/findings en caliente fallan en check; findings solo vía artifacts |

## 6. Adapters builtin

- **`claude-code`**: declara todas las capacidades. Lanza `claude -p` headless con
  salida en streaming JSON y traduce ese stream a `AgentEvent`; `resume` reutiliza
  la sesión vía el flag de reanudación del CLI con el `SessionId` persistido;
  `edit_hooks` se implementa instalando hooks de pre-edición en la configuración de
  la sesión que validan contra `edit_constraints`; `custom_agents` mapea `agent:` a
  los agentes definidos por el equipo en su configuración de Claude Code,
  verificando su existencia en `probe()`; `permission_profiles` mapea a los modos de
  permisos del CLI; skills y MCP por-run se inyectan por la configuración de sesión.
  Los detalles de flags viven en el adapter y se validan en `probe()` contra la
  versión instalada; el engine no conoce ninguno.
- **`codex`**: `codex exec` headless con salida JSON; `permission_profiles` mapea a
  sus modos de sandbox; `resume_session: false` salvo que `probe()` detecte soporte
  en la versión instalada (las capacidades pueden calcularse en el constructor a
  partir del probe, nunca cambiar después).
- **`mock`**: pieza de primera clase, no un helper de tests: reproduce sesiones desde
  fixtures (guiones YAML de eventos + efectos sobre el filesystem), con fallos y
  latencias inyectables, y puede simular solicitudes de ampliación de scope y
  posteos al blackboard. Es lo que permite testear el engine completo (ciclo de
  tareas, degradación, cancelación, resume, paralelismo) en CI sin ningún LLM, y
  validar workflows nuevos sin gastar presupuesto (`yunta run --adapter mock`).

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
