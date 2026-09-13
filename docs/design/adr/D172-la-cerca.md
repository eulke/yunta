---
number: D172
title: "La cerca: un juez en core, un nivel por adapter, una cobertura por sesión, y el post-check como garantía"
status: accepted
revises: [D13, D167]
revised_by: []
---

# D172 — La cerca: un juez en core, un nivel por adapter, una cobertura por sesión, y el post-check como garantía

## Contexto

D13 fija el scope por globs y su enforcement "en caliente vía capability
`edit_hooks`". Ningún adapter la declara; D167 la registró como deuda A-13.
Lo que una sesión puede escribir se decía en cuatro lugares que no se
conocían: `edit_constraints: Option<Vec<String>>` en `SessionRequest`,
`edit_hooks: bool` en `Capabilities`, `artifact_dir` como el permiso
implícito de escribir fuera del worktree, y el marcador `blocked:<path>` que
el mock inventaba para decir que no escribió. Claude Code corre `read_only`
con la herramienta `Write` (AD-D7); Codex corre `read_only` con un
`artifact_dir` que no puede escribir y no lo dice (AD-D20).

Un relevamiento de los ocho CLIs más usados después de Claude Code y Codex
(`plan-de-raiz/auditoria/09-mercado-de-clis.md`) muestra tres formas de
controlar escrituras y ninguna cuarta: un juicio por llamada de herramienta
(Gemini, Copilot, Cursor, OpenCode, Goose, Amp, Kimi), un sandbox de
filesystem por directorios (Codex; Gemini, Copilot y Cursor solo para el
shell), o nada (Aider). Siete aceptan un hook de comando con JSON por stdin y
rechazo por exit 2 o JSON en stdout. Cuatro señalan el rechazo con un campo
estructurado; cuatro solo con prosa. Siete reubican su configuración con una
variable de entorno. Ninguno cerca el shell por path sin sandbox. Todos
alimentan la razón del rechazo al modelo.

## Decisión

1. **Un juez.** `yunta_core::fence::Fence { allowed: Vec<ScopeGlob>, roots: Vec<PathBuf> }`
   con `judge(worktree, target) -> Verdict`, función pura, es el único lugar
   que decide si un path está dentro de lo que una sesión puede escribir. Los
   adapters escriben solo el codec entre ese juez y el mecanismo de su CLI;
   el comando oculto `yunta fence <adapter>` los une.
2. **Un nivel por adapter.** `Capabilities::fence: FenceLevel { None, ToolCalls, Filesystem }`
   reemplaza `edit_hooks: bool`. Un transporte distinto (hook, reglas
   declarativas, plugin, ACP) no es un nivel distinto.
3. **Una cobertura por sesión.** `agent_session_opened` lleva
   `FenceReport { level, coverage: Coverage { Exact, WidenedToRoots, ToolsOnly } }`,
   calculada por `Coverage::of` como el canal más débil entre las
   herramientas de archivo y todo lo demás. Se calcula de lo que el adapter
   construyó; no se declara.
4. **Un rechazo.** El kind `write_refused { target: ToolTarget }` registra
   cada escritura refusada. Su texto para el modelo nace una sola vez en
   core y empieza por un marcador fijo; el parser de cada adapter lo reconoce,
   y usa además el campo estructurado del CLI donde existe.
5. **La cerca vive fuera del checkout.** Un adapter instala su cerca bajo
   `scratch_dir` y apunta la variable de home del CLI ahí; lo que un CLI solo
   lee desde el árbol del proyecto se declara en `staged_paths`.
6. **El post-check sigue siendo la garantía.** `scope_check` no cambia. Una
   escritura que llega al diff bajo `Coverage::Exact` es además un
   `engine_finding`: el adapter declaró exacta una cerca que algo cruzó.
7. **`read_only` es una cerca con `allowed` vacío y raíces intactas.** Los
   archivos declarados siguen siendo escribibles; el adapter que no puede
   honrar eso (Codex con `--sandbox read-only`) lo dice con
   `capability_degraded(NoWritableRoots)`.
8. **`edit_constraints`, `Glob`, `edit_hooks`, `Capability::EditHooks` y los
   marcadores del mock se borran.** A-13 se cierra al cerrar el ítem 3-08.

Especificación completa: `docs/design/plan-de-raiz/cerca.md` (M25).

## Racional

Frontera: el engine conoce al adapter por lo que declara (el nivel) y por lo
que construyó (la cobertura), nunca por su mecanismo. Un lugar: el juez, el
texto del rechazo y la regla de cobertura viven en core; un segundo juez por
adapter sería la copia que señala el lugar que falta. Degradación explícita:
lo que no se pudo cercar se dice con un evento (`capability_degraded`, la
cobertura, `write_refused`) y se atrapa con el post-check. Parsear es validar:
`ScopeGlob` compila al parsear; una cerca inválida no llega a una sesión.

## Alternativas descartadas

- **Un nivel por sesión en lugar de una capacidad.** El engine necesita
  saber antes de abrir la sesión si degrada; el nivel es del adapter y la
  cobertura es de la sesión.
- **Cobertura como producto por canal** (`{ tools, shell, mcp }`). El engine
  decide una sola cosa con ella (el `engine_finding` bajo `Exact`); el valor
  más débil lo decide igual y se lee en una palabra.
- **Un nivel `Judge` para ACP y SDKs.** Es el mismo contrato (cada llamada
  juzgada antes de ocurrir) sin el fail-open del hook; la diferencia es un
  límite del transporte, que spec-adapter §6 declara, no un nivel.
- **Mantener `edit_hooks: bool` y `artifact_dir` como permiso.** Deja tres
  fuentes para un hecho y no distingue un sandbox de un hook.
- **Un juez por adapter.** Siete CLIs lo aceptan idéntico; la copia sería el
  vicio V1.
- **La política partida del sandbox de Codex** (`/repo/a=none`). No expresa
  globs; el post-check ya cubre lo que ella cubriría.
