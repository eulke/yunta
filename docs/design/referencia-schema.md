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

# Runners: bindings que los nodos piden vía nombre de rol.
# Cada rol resuelve a una lista ordenada de candidatos.
runners:
  planner:
    - { adapter: claude-code, model: claude-opus-4-8 }
    - { adapter: codex, model: gpt-5-codex }
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
  isolation: worktree               # worktree | none (§7.3; `inherit` solo en nodos workflow)
  timeout_minutes: 45
  max_parallel_nodes: 4
  on_failure: pause                 # pause | abort | continue
  on_interrupt: restart_node        # restart_node | resume_session

skills:
  paths: [.yunta/skills, ~/.yunta/skills]
  always: [conventions]
  executors:
    - { name: coverage-gate, kind: binary, path: .yunta/bin/coverage-gate }

mcp_servers:                        # servers para la fuente de contexto `mcp`
  internal-docs: { url: "https://docs.interna.example/mcp", auth_env: DOCS_TOKEN }

baseline:
  suite: "cargo test --workspace"
coverage:
  cmd: "cargo llvm-cov --summary-only"
  threshold: 90

storage:
  path: ~/.yunta/yunta.db           # SQLite en modo WAL — único backend (D53)
  retention_days: 90

paths:                              # dónde vive el estado (§2.2); `YUNTA_HOME` los sobreescribe
  runs: ~/.yunta/runs
  worktrees: ~/.yunta/worktrees

permissions:                        # techo; las capas inferiores solo estrechan (§6.1)
  commands:
    deny: ["curl * | *", "sudo *"]
  packs:
    executors: prompt               # allow | prompt | deny
    publishers: { allow: [acme] }
  network:
    default: true

limits:
  max_tokens_per_run: 2_000_000
  max_loop_iterations: 12
  max_concurrent_runs: 3
  max_workflow_depth: 4
  max_artifact_bytes: 50_000_000    # guardia contra accidentes (§4)
  inline_context_bytes: 32_000      # sobre este umbral, el contexto se monta por referencia (§9.1)

pricing:                            # opcional — sin esto, stats y recibo son solo tokens (§8.4)
  claude-opus-4-8: { cost_per_1k_tokens: 0.015 }
  claude-sonnet-4-6: { cost_per_1k_tokens: 0.003 }

secrets:                            # nombres de env vars; valores JAMÁS acá
  - GITHUB_TOKEN
```

## Workflow de referencia: build-feature.yaml

```yaml
name: build-feature
description: Feature completa con grill, documento de tareas verificado, review multi-runner y PR
yunta_schema: ">=1 <2"              # opcional (§2.1); sin declarar, se infiere del binario
inputs:
  idea:
    type: string
    required: true
    description: "Qué construir — se convierte en el brief"

modes:                              # nombres y cantidad libres del autor (§10.1);
                                    # el orden declara la escalera de promoción
  quick:    { include: [grill, plan, implement, lint, fix-lint, tests, ship, pr] }
  standard: { include: [grill, plan, approve-plan, implement, lint, fix-lint, tests, review, fix-findings, ship, pr] }
  full:     { include: all }

node_defaults:
  hooks:
    after:
      - run: "cargo fmt"

nodes:                              # id: letra seguida de letras, dígitos, `_` o `-`
  - id: grill
    kind: prompt
    runner: planner
    skills: [grill]
    interactive: true               # §4.1 — dato de presentación: cómo se muestran las preguntas
    prompt: |
      Identificá las ambigüedades de "{{inputs.idea}}" y escribí las preguntas
      necesarias como artifact; no converses. Con las respuestas, escribí el brief.
    artifacts:
      produces: [questions, brief.md]

  - id: plan
    kind: prompt
    runner: planner
    permissions: read-only
    depends_on: [grill]
    context:
      - artifact: { node: grill, name: brief.md }
      - knowledge: {}
      - files: ["docs/architecture.md"]
      - command: "git log --oneline -20"
      - mcp: { server: internal-docs, query: "{{inputs.idea}}" }
    prompt: { file: prompts/plan.md }   # §9.3 — también admite string inline
    artifacts:
      produces: [tasks]

  - id: approve-plan
    kind: gate
    depends_on: [plan]
    assignee: lead
    message: "Plan registrado. ¿Aprobás?"
    options: [aprobar, ajustar, abortar]
    on: { ajustar: plan }

  - id: implement
    kind: loop
    runner: executor
    depends_on: [approve-plan]
    until: all_tasks_complete
    invariant: true
    concurrency: 2                  # tareas simultáneas con scopes disjuntos (§5.5); default 1
    scope_expansion:                # §6.2 — default deny si se omite
      mode: ask
      within: ["src/**"]
      max_per_run: 3
    prompt: |
      Leé tu tarea del documento de tareas. Implementala dentro de su scope.

  - id: lint
    kind: bash
    invariant: true
    depends_on: [implement]
    run: "cargo clippy -- -D warnings"
    on_failure: { goto: fix-lint, max_reroutes: 2 }

  - id: fix-lint
    kind: prompt
    runner: mechanical
    context: [{ node-output: { node: lint } }]
    prompt: "Corregí exclusivamente los errores del reporte."
    scope: ["src/**"]

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
    prompt: "Auditá los cambios y reportá cada hallazgo."
    artifacts:
      produces: [findings]

  - id: fix-findings
    kind: prompt
    runner: executor
    depends_on: [review]
    context:
      - run-events: { filter: findings }   # hallazgos como datos, no como prosa
    prompt: "Corregí hallazgos bloqueantes; documentá los descartados con motivo."

  - id: ship
    kind: gate
    depends_on: [fix-findings]
    assignee: lead
    message: "¿Creo el PR?"

  - id: pr
    kind: bash
    depends_on: [ship]
    run: |
      git push -u origin {{run.branch}}
      gh pr create --fill --base {{project.base_branch}}

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
existe el **MCP por-run** (endpoint que el engine pasa en
`SessionRequest.run_tools_endpoint`) con tools de scope de run: `yunta_post_finding`,
`yunta_get_blackboard`, `yunta_task_status`, `yunta_request_scope_expansion`.

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
  prompt, su `run:`, sus hooks y sus patrones de `context:` — `{{run.dir}}`,
  `{{run.worktree}}`, `{{run.branch}}`, `{{node.artifacts}}` (el directorio propio
  del nodo, donde escribe lo que declara), `{{runner.role}}` cuando el nodo declara
  un runner, `{{project.name}}`/`{{project.base_branch}}`/`{{project.branch_prefix}}`
  según lo que declare `project:`, y un `{{inputs.<nombre>}}` por input declarado.
  Una variable que no está definida ahí falla el nodo nombrándola; `yunta check`
  además rechaza estáticamente todo `{{inputs.x}}` que `inputs:` no declare.
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
  ```

  En workflows compartidos, preferir el agente en los candidatos del runner
  (portabilidad multi-adapter); el override por nodo es para workflows internos del
  equipo.
