# Config y workflows de referencia

Prototipos canónicos con la terminología del Contrato v0.5. **Estos ejemplos son la
referencia de schema para la implementación** y los fixtures de parseo del
workspace (`crates/core/tests/fixtures/`).

## config.yaml (capas repo → usuario → org)

```yaml
# Resolución en capas (mayor precedencia primero):
#   1. .yunta/config.yaml        (repo — versionado, del equipo)
#   2. ~/.yunta/config.yaml      (usuario — credenciales, paths locales)
#   3. /etc/yunta/config.yaml    (org — defaults corporativos, opcional)
# Merge por clave; los arrays se reemplazan.
version: 1

project:
  name: mi-repo
  base_branch: main
  branch_prefix: yunta/

forge:                              # la forja del equipo; token_env nombra la variable, nunca el token
  github: { repo: acme/web, token_env: GITHUB_TOKEN }

# Runners: bindings que los nodos piden vía nombre de rol.
# Cada rol resuelve a una lista ordenada de candidatos.
runners:
  planner:
    - { adapter: claude-code, model: claude-opus-4-8, effort: high }
    - { adapter: codex, model: gpt-5-codex, effort: high }
                                    # effort: low | medium | high | xhigh — cuánto razona
                                    # el modelo; ausente lo deja al CLI. Un nodo puede
                                    # decir el suyo, que gana
  executor:
    - { adapter: claude-code, model: claude-sonnet-4-6 }
  reviewer:
    - { adapter: claude-code, model: claude-sonnet-4-6, agent: benito }
  reviewer-alt:
    - { adapter: codex, model: gpt-5-codex }
  mechanical:
    - { adapter: claude-code, model: claude-haiku-4-5 }

adapters:
  claude-code:
    binary: ~/.local/bin/claude     # override local (capa usuario)
  codex:
    binary: /usr/local/bin/codex
    adapter_settings: { sandbox: workspace-write }

defaults:
  runner: executor
  isolation: worktree               # worktree | none (§7.3; la misma palabra en un nodo workflow)
  timeout_minutes: 45
  max_parallel_nodes: 4
  on_failure: pause                 # pause | abort | continue
  on_interrupt: restart_node        # restart_node | fail_if_uncertain | resume_session

skills:
  paths: [.yunta/skills, ~/.yunta/skills]
  always: [conventions]
  executors:
    - { name: coverage-gate, kind: binary, path: .yunta/bin/coverage-gate }

mcp_servers:                        # servers para la fuente de contexto `mcp`
  internal-docs: { url: "https://docs.interna.example/mcp", auth_env: DOCS_TOKEN }

commands:                           # lo que el proyecto corre por cada capacidad que un workflow nombra
  lint: "cargo clippy --workspace -- -D warnings"
  fmt: "cargo fmt"
  format: "rustfmt --edition 2021"

format:                             # qué comando formatea qué archivos en cuanto una sesión los escribe (D255)
  - { paths: ["**/*.rs"], run: format }   # la primera regla que cubre un archivo; recibe las rutas como argumentos

shared_dirs:                        # caches que comparten todos los comandos y sesiones de un run; nunca la salida de un build (D236)
  PIP_CACHE_DIR: ~/.cache/yunta/pip   # exportado bajo la variable; escribible en cada sandbox que escribe
baseline:
  suite: "cargo test --workspace"
  held: [lint]                      # comandos de `commands:` medidos al lado de la suite: el que pasa
                                    # en la base sostiene cada tarea como guarda, igual que la suite (D256)
coverage:
  cmd: "cargo llvm-cov --summary-only"
  threshold: 90

storage:
  path: ~/.yunta/yunta.db           # SQLite en modo WAL — único backend (D53)
  retention_days: 90
  pool:                             # lo que los checkouts del proyecto guardan del disco (D247)
    budget: 60G                     # bytes (80G, 500M, 1.5T) o parte del disco (20%)
    keep_free: 10%                  # lo que del disco siempre queda libre

paths:                              # dónde vive el estado (§2.2); `YUNTA_HOME` los sobreescribe
  runs: ~/.yunta/runs
  worktrees: ~/.yunta/worktrees

permissions:                        # techo; las capas inferiores solo estrechan (§6.1)
  commands:
    allow: []                       # vacía: todo lo no denegado corre; no vacía: lista blanca estricta
    deny: ["curl * | *", "sudo *"]
  paths:
    deny: [".github/**"]            # ningún run los escribe, sea cual sea el scope
  packs:
    executors: prompt               # allow | prompt | deny
    publishers: { allow: [acme] }
  network:
    default: true
  scope_expansion: { max_mode: ask }   # rules | ask | deny: lo más permisivo que un nodo puede pedir

limits:
  max_tokens_per_run: 2000000
  max_loop_iterations: 12
  max_concurrent_runs: 3
  max_workflow_depth: 4
  max_artifact_bytes: 50000000    # guardia contra accidentes (§4)
  inline_context_bytes: 32000      # sobre este umbral, el contexto se monta por referencia (§9.1)

pricing:                            # opcional — sin esto, stats y recibo son solo tokens (§8.4)
  claude-opus-4-8: { cost_per_1k_tokens: 0.015 }
  claude-sonnet-4-6: { cost_per_1k_tokens: 0.003 }

secrets:                            # nombres de env vars; valores JAMÁS acá
  - GITHUB_TOKEN
```

## Workflow de referencia: build-feature.yaml

```yaml
name: build-feature
description: Complete feature with grill, verified tasks document, multi-runner review and PR
yunta_schema: ">=1 <2"              # opcional (§2.1); sin declarar, se infiere del binario
inputs:
  idea:
    type: string
    required: true
    description: "What to build — it becomes the brief"

modes:                              # nombres y cantidad libres del autor (§10.1);
                                    # el orden declara la escalera de promoción
  quick:    { include: [grill, brief, plan, implement, lint, fix-lint, tests, ship, pr], ask: required }
                                    # ask: qué rondas de preguntas detienen el run —`always`
                                    # (default) o `required`—; el `ask` de un nodo gana (D244)
  standard: { include: [grill, brief, plan, approve-plan, implement, lint, fix-lint, tests, review, fix-findings, ship, pr] }
  full:     { include: all }

node_defaults:
  hooks:
    after:
      - run: "cargo fmt"
  cli_config: isolated              # inherit (default) | isolated: qué config del CLI
                                    # del agente cargan las sesiones; un nodo puede
                                    # decir la suya (D250)

nodes:                              # id: letra seguida de letras, dígitos, `_` o `-`
  - id: grill
    kind: prompt
    runner: planner
    skills: [grill]
    prompt: |
      Identify the ambiguities in "{{inputs.idea}}" and write the questions
      needed as an artifact; do not converse.
    artifacts:
      produces: [questions]         # §4.1 — un nodo que pregunta no declara nada más
                                    # ask: always | required — solo en un nodo que pregunta

  - id: brief
    kind: prompt
    runner: planner
    depends_on: [grill]
    context:
      - artifact: { node: grill, kind: questions }
      - artifact: { node: grill, kind: answers }
    prompt: |
      Write the brief for "{{inputs.idea}}" from the questions and their
      answers.
    artifacts:
      produces: [brief.md]

  - id: plan
    kind: prompt
    runner: planner
    effort: xhigh                   # sobre el `effort` del candidato que resuelva
    permissions: read-only
    depends_on: [brief]
    context:
      - artifact: { node: brief, name: brief.md }
      - knowledge: {}
      - files: ["docs/architecture.md"]   # or { path: ..., optional: true } when the node can do without it (D186)
      - command: "git log --oneline -20"
      - mcp: { server: internal-docs, query: "{{inputs.idea}}" }
    prompt: { file: prompts/plan.md }   # §9.3 — también admite string inline
    artifacts:
      produces: [tasks]

  - id: approve-plan
    kind: gate
    depends_on: [plan]
    assignee: lead
    message: "Plan recorded. Do you approve?"
    options: [approve, adjust]
    on: { adjust: plan }              # pide qué cambiar y retoma la sesión de `plan` con eso (D194)
    shows: [{ node: plan, kind: tasks }]   # el plan que se aprueba, atado a su hash en `gate_waiting`

  - id: implement
    kind: loop
    runner: executor
    depends_on: [approve-plan]
    until: all_tasks_complete
    invariant: true
    concurrency: 2                  # tareas simultáneas con scopes disjuntos (§5.5); default 1
    scope_expansion:                # §6.2 — default deny si se omite
      mode: ask
      max_per_run: 3
    prompt: |
      Read your task in the tasks document. Implement it within its scope.

  - id: lint
    kind: bash
    invariant: true
    optional: true                  # sin `commands.lint` el run corre sin este nodo (D204)
    depends_on: [implement]
    run: { command: lint }
    on_failure: { goto: fix-lint, max_reroutes: 1 }

  - id: fix-lint
    kind: prompt
    runner: mechanical
    context: [{ node-output: { node: lint } }]
    prompt: "Fix only the errors in the report."
    scope: run                     # lo que el run cambió hasta acá (D205)

  - id: tests
    kind: check
    builtin: baseline_compare
    invariant: true
    depends_on: [lint]

  - id: review
    kind: prompt
    depends_on: [tests]
    runners: [reviewer, reviewer-alt]
    permissions: read-only
    prompt: "Audit the changes and report each finding."
    artifacts:
      produces: [findings]

  - id: fix-findings
    kind: prompt
    runner: executor
    depends_on: [review]
    context:
      - run-events: { filter: findings }   # hallazgos como datos, no como prosa
    prompt: "Fix the blocking findings; document each dismissed one with its reason."

  - id: ship
    kind: gate
    depends_on: [fix-findings]
    assignee: lead
    message: "Create the PR?"

  - id: pr
    kind: pull_request     # push de la rama del run + PR por el forge (D207)
    depends_on: [ship]
    title: "{{inputs.idea}}"

on_finish:
  - cleanup: worktree
  - distill: [{ node: plan, kind: tasks }]
```

## Workflow compuesto de referencia: release-cycle.yaml

```yaml
name: release-cycle
inputs:
  rfc:
    type: path
    required: true
    description: "RFC a revisar"
  feat_a:
    type: string
    required: true
  feat_b:
    type: string
    required: true
nodes:
  - id: design
    kind: workflow
    use: design-review
    inputs: { rfc: "{{inputs.rfc}}" }

  - id: approve-design
    kind: gate
    depends_on: [design]
    assignee: arquitectura

  - id: build
    kind: parallel
    depends_on: [approve-design]
    nodes:
      - { id: feat-a, kind: workflow, use: build-feature, inputs: { idea: "{{inputs.feat_a}}" } }
      - { id: feat-b, kind: workflow, use: build-feature, inputs: { idea: "{{inputs.feat_b}}" } }

  - id: qa
    kind: workflow
    use: qa-review
    depends_on: [build]

  - id: ship
    kind: gate
    depends_on: [qa]
    assignee: lead
```

## Integración con Claude Code (MCP)

```json
{ "mcpServers": {
    "yunta": { "command": "yunta", "args": ["mcp"] }
} }
```

El cliente lanza `yunta mcp` como subproceso por stdio; tools expuestas:
`list_workflows` (catálogo vivo del repo y de packs: nombre, descripción, inputs,
modos), `run_workflow`, `workflow_status`, `resume_run`, `resolve_gate`. Además
existe el **MCP por-run**, que el engine monta él mismo en cada sesión bajo el
nombre `yunta-run` (endpoint que pasa en `SessionRequest.run_tools_endpoint`), con
tools de scope de run: `yunta_post_finding`,
`yunta_get_blackboard`, `yunta_task_status`, `yunta_request_scope_expansion`,
—solo en una sesión de tarea— `yunta_task` y `yunta_check_task`, y —solo en la
sesión propia de un nodo con `scope:`— `yunta_check_scope`.

Para que el agente cliente sepa **cuándo** usar todo esto, `yunta init` instala una
skill de mecanismo en el repo y ofrece una línea para el CLAUDE.md del equipo (D74).
La skill nunca contiene el catálogo de workflows: lo consulta con `list_workflows`,
que siempre está al día.

## Notas de schema

- **`artifacts.produces`**: una lista de strings sueltos. Un string que nombra un
  kind — el conjunto es cerrado: `tasks`, `findings` y `questions` — declara ese
  documento, y todo otro string es el nombre de un archivo opaco. Un nodo produce a
  lo sumo uno de cada kind, así que el kind identifica al documento y no hay nombre
  que elegir: declarar el mismo kind dos veces es error de `check`, y los tres
  nombres de kind quedan reservados como nombres de archivo. El kind lo nombra un
  único tipo, del que salen el valor que se escribe acá, el argumento de
  `yunta schema <kind>`, el catálogo de la tool `document_shape` y el documento del
  que habla un reporte de lectura (D132). Declararlo alcanza para que el nodo reciba
  la forma de ese documento en su contexto: no hay una segunda clave que la pida.
  Qué implica declararlo — parser, reglas, rechazo de un documento inválido — está
  en el Contrato §4.1; la forma de cada kind se lee con `yunta schema <kind>`, y su
  JSON Schema con `--json`.
- **`inputs:`**: un mapa de nombre a especificación, con `type:` eligiendo la forma
  — `string`, `number`, `boolean`, `enum`, `path`, `document` —, y solo los campos
  de ese tipo disponibles. Un `document` declara además el `kind:` con el que el run
  lee el archivo: el run nace teniéndolo como artifact propio, sin nodo productor, y
  `{{inputs.<nombre>}}` rinde su identidad (`sha256:<hash>`) y no la ruta. El resto
  está en el Contrato §2.3.
- **Referencias a un artifact**: una `context: [{ artifact }]`, una entrada de
  `mounts:` y una de `on_finish.distill` nombran el artifact por lo que lo
  identifica — `kind: <k>` para un documento que el engine lee, `name: <archivo>`
  para uno opaco — y exactamente uno de los dos. Un `as:` de mount renombra un
  opaco, que es lo que el hijo pasa a tener; un interpretado se identifica por su
  kind en todo run que lo tenga, así que `as:` al lado de un `kind:` se rechaza al
  leer el workflow.
- **Variables de template**: lo que un nodo puede escribir entre `{{ }}` en su
  prompt, su `run:`, sus hooks y sus patrones de `context:`. El conjunto es cerrado
  (`TemplateVar`): `{{run.dir}}`, `{{run.worktree}}`, `{{run.branch}}`,
  `{{run.base}}` (el commit del que partió el run: `git diff {{run.base}}` es todo
  lo que el run cambió), `{{run.staging}}` (la raíz bajo la que cuelga el staging
  de cada nodo),
  `{{node.artifacts}}` (el directorio propio del nodo, donde escribe lo que
  declara), `{{node.id}}`, `{{runner.name}}` cuando el nodo declara un runner,
  `{{project.name}}`/`{{project.base_branch}}`/`{{project.branch_prefix}}` según lo
  que declare `project:`, y un `{{inputs.<nombre>}}` por input declarado. Un nombre
  que no es ninguna de esas no es una variable y se rechaza al leer el template,
  nombrando las que existen; una variable del conjunto que nadie definió en ese
  sitio falla el nodo nombrándola; `yunta check` además rechaza estáticamente todo
  `{{inputs.x}}` que `inputs:` no declare.
- **`skills:` vs `context:`**: propiedades separadas por diseño. `context:` inyecta
  datos (sobre qué trabajar) vía `ContextSource`; `skills:` monta instrucciones y
  capacidades (cómo trabajar) por el mecanismo nativo del adapter. La sintaxis
  completa de `context:` con todas las fuentes builtin está en el Contrato §9.
- **`agent:` a nivel nodo**: override del agente del runner para ese nodo (Contrato
  §13.3):

  ```yaml
  - id: review-security
    kind: prompt
    runner: reviewer
    agent: security-auditor
    permissions: read-only
    prompt: "Auditá los cambios y reportá cada hallazgo de seguridad."
  ```

  El nombre resuelve a `.yunta/agents/<nombre>.md` del proyecto, o a la definición
  que un pack declara en `contents.agents` (`publisher/agente`): front matter con
  `name` y `description`, y el cuerpo como instrucciones. La misma definición corre
  en todo adapter; un nombre sin definición falla el nodo antes de abrir la sesión.
- **`effort:`** en un candidato del runner o en el nodo, que gana: `low`, `medium`,
  `high` o `xhigh`, los niveles que aceptan todos los CLIs. `claude-code` lo recibe
  como `--effort` y `codex` como `-c model_reasoning_effort`, también al retomar una
  sesión; un nivel que solo un CLI tiene (`max`, `minimal`) se rechaza al leer el
  YAML. El nivel pedido queda en `agent_session_opened.effort`.
