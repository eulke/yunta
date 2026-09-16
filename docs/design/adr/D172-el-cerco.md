---
number: D172
title: "El cerco: un juez en core, un nivel por adapter, una cobertura por sesión, y el post-check como garantía"
status: accepted
revises: [D13, D167]
revised_by: []
---

# D172 — El cerco: un juez en core, un nivel por adapter, una cobertura por sesión, y el post-check como garantía

## Contexto

D13 fija el scope por globs y su enforcement "en caliente vía capability
`edit_hooks`". Ningún adapter la declara; D167 la registró como deuda A-13.
Lo que una sesión puede escribir se decía en cuatro lugares que no se
conocían: `edit_constraints: Option<Vec<String>>` en `SessionRequest`,
`edit_hooks: bool` en `Capabilities`, `artifact_dir` leído por cada adapter
como el permiso de escribir fuera del worktree, y el marcador `blocked:<path>`
que el mock inventaba para decir que no escribió. Claude Code corre
`read_only` con la herramienta `Write` (AD-D7); Codex corre `read_only` con un
`artifact_dir` que no puede escribir y no lo dice (AD-D20).

Un relevamiento de los ocho CLIs más usados después de Claude Code y Codex
(`plan-de-raiz/auditoria/09-mercado-de-clis.md`) muestra tres formas de
controlar escrituras y ninguna cuarta: un juicio por llamada de herramienta
(Gemini, Copilot, Cursor, OpenCode, Goose, Amp, Kimi), un sandbox de
filesystem por directorios (Codex; Gemini, Copilot y Cursor solo para el
shell), o nada (Aider). Los siete del primer grupo ejecutan un comando antes
de escribir, con JSON por stdin y rechazo por exit 2 o JSON en stdout. Cuatro
señalan el rechazo con un campo estructurado; cuatro solo con prosa. Siete
reubican su configuración con una variable de entorno. Ninguno cerca el shell
por path sin sandbox. Todos alimentan la razón del rechazo al modelo.

## Decisión

1. **Un juez.** `yunta_core::fence::Fence { allowed: Option<Vec<ScopeGlob>>, roots: Vec<PathBuf>, advice }`
   con `judge(worktree, target) -> Verdict`, función pura, es el único lugar
   que decide si un path está dentro de lo que una sesión puede escribir.
   `allowed` es el scope declarado más las ampliaciones ya concedidas, lo
   mismo que el post-check evalúa (Contrato §6.2); una ampliación pedida
   durante un intento rige desde el siguiente. `None` es un nodo que no
   declaró scope —todo bajo el worktree— y `Some([])` es `read_only`, que no
   admite nada: dos hechos distintos, distinguidos por tipo y no por un
   patrón que los represente a los dos.
2. **Un hook y un codec por adapter.** El CLI construye `FenceHook` una vez
   con su propio binario; el subcomando `yunta fence <adapter-id>` invoca al
   juez y responde con el codec del adapter. El nombre del subcomando vive en
   `core::fence`; un adapter escribe solo el codec. Las reglas declarativas
   de un CLI son una segunda línea, nunca la fuente de la cobertura.
3. **Un nivel por adapter.** `Capabilities::fence: FenceLevel { None, ToolCalls, Filesystem }`
   reemplaza `edit_hooks: bool`. Un transporte distinto (hook, extensión,
   programa delegado, ACP) no es un nivel distinto.
4. **Una cobertura por sesión.** `agent_session_opened.fence: Option<Coverage { Exact, WidenedToRoots, ToolsOnly }>`,
   calculada por `Coverage::of` como el canal más débil entre las
   herramientas de archivo y todo lo demás. Se calcula de lo que el adapter
   construyó; no se declara. El nivel viaja una sola vez, en `capabilities`.
5. **Un rechazo.** El kind `write_refused { session_id, target }` registra
   cada escritura rechazada. Su texto para el modelo nace una sola vez en
   core y empieza por un marcador fijo; el parser de cada adapter lo reconoce,
   y usa además el campo estructurado del CLI donde existe.
6. **El cerco vive fuera del checkout.** Un adapter instala su cerco bajo
   `scratch_dir` y apunta la variable de home del CLI ahí; lo que un CLI solo
   lee desde el árbol del proyecto se declara en `staged_paths`. Ningún
   adapter lee `artifact_dir` como permiso: las raíces salen de `fence.roots`.
7. **El post-check sigue siendo la garantía.** `scope_check` no cambia. Una
   escritura que llega al diff bajo `Coverage::Exact` es además un
   `engine_finding` (`engine-fence-breach`, `Major`): el adapter declaró
   exacto un cerco que algo cruzó.
8. **`read_only` es un cerco con `allowed` vacío y raíces intactas.** Los
   archivos declarados siguen siendo escribibles. Claude Code conserva
   `Write`/`Edit` bajo `read_only` solo cuando hay archivos declarados, y el
   cerco rechaza todo lo demás. Codex no puede dejar el worktree de solo
   lectura y las raíces escribibles a la vez: falla la sesión antes de spawn
   con `AdapterError::FenceUnbuildable(SealedRoots)`, porque una capacidad
   ausente falla en vez de gastar la sesión.
9. **Umbral.** El hook de Claude Code corre con `timeout: 10` segundos: el
   juez no toca disco ni red y diez segundos cubren cualquier arranque frío
   del binario; cambiarlo es revisar esta decisión.
10. **`edit_constraints`, `Glob`, `edit_hooks`, `Capability::EditHooks` y los
    marcadores del mock se borran.** A-13 se cierra al cerrar el ítem 3-08.

Especificación completa: `docs/design/plan-de-raiz/cerco.md` (M25).

## Racional

Frontera: el engine conoce al adapter por lo que declara (el nivel) y por lo
que construyó (la cobertura), nunca por su mecanismo; el adapter conoce el
hook por un valor del puerto, nunca por el nombre de un comando del CLI. Un
lugar: el juez, el texto del rechazo, la regla de cobertura y el nombre del
subcomando viven en core; un segundo juez por adapter sería la copia que
señala el lugar que falta. Degradación explícita: lo que no se pudo cercar se
dice con un evento (`capability_degraded`, la cobertura, `write_refused`) o
falla con su causa (`FenceUnbuildable`), y el post-check lo atrapa. Parsear es
validar: `ScopeGlob` compila al parsear; un cerco inválido no llega a una
sesión.

## Alternativas descartadas

- **Un nivel por sesión en lugar de una capacidad.** El engine necesita
  saber antes de abrir la sesión si degrada; el nivel es del adapter y la
  cobertura es de la sesión.
- **Cobertura como producto por canal** (`{ tools, shell, mcp }`). El engine
  decide una sola cosa con ella (el `engine_finding` bajo `Exact`); el valor
  más débil lo decide igual y se lee en una palabra.
- **Un `FenceReport { level, coverage }` en el evento.** Repite el nivel que
  `capabilities` ya lleva en el mismo payload.
- **Un nivel `Judge` para ACP y SDKs.** Es el mismo contrato (cada llamada
  juzgada antes de ocurrir) sin el fail-open del hook; la diferencia es un
  límite del transporte, que spec-adapter §6 declara, no un nivel.
- **Mantener `edit_hooks: bool` y `artifact_dir` como permiso.** Deja tres
  fuentes para un hecho y no distingue un sandbox de un hook.
- **Un juez por adapter, o las reglas declarativas del CLI como juez.**
  Siete CLIs ejecutan el mismo comando; la copia, o la semántica de glob
  ajena, sería el vicio V1.
- **Degradar en vez de fallar cuando Codex no puede escribir las raíces.**
  La sesión gastaría tokens para un nodo que no puede entregar sus archivos.
- **Conceder ampliaciones en caliente durante el intento.** Cambia el flujo
  de Contrato §6.2 y el momento en que decide la política; el reintento ya
  existe y alcanza.
- **La política partida del sandbox de Codex** (`/repo/a=none`). No expresa
  globs; el post-check ya cubre lo que ella cubriría.
