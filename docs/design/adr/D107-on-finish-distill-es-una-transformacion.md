---
number: D107
title: "`on_finish.distill` es una transformación determinista, jamás una sesión de agente (Contrato §8.3/§9.2/D20; resuelve la cuestión abierta de T5.8)"
status: revised
revises: []
revised_by: [D157]
---

# D107 — `on_finish.distill` es una transformación determinista, jamás una sesión de agente (Contrato §8.3/§9.2/D20; resuelve la cuestión abierta de T5.8)

*(Revisada por D157: una entrada de `distill` nombra un nodo y una identidad
—`kind:` o `name:`—, no un path; el engine copia los bytes que el log nombra y
registra ese mismo hash en `provenance.yaml`.)* Para cada path declarado
(relativo a `run.dir/artifacts/`, y cada uno debe coincidir con un
`artifacts.produces` declarado — error de `check` si no), el engine copia el
archivo a `<worktree>/.yunta/knowledge/distilled/<workflow>/<run_id>/` — un
subdirectorio por run, jamás un índice compartido mutable (dos PRs
concurrentes destilando a un índice = conflicto de merge garantizado) — y
escribe al lado un `provenance.yaml` derivado por función pura de (manifest,
log, reloj inyectado): run de origen, workflow+hash, modo, artifacts con
content hash (o `missing: true` — path declarado y no producido en runtime →
`finding_posted` minor, el resto se destila igual), y evidencia de
verificación contada del log (criterios ejecutados/verdes/reusados, findings
por severidad). Secuencia de cierre: distill → `run_finished` → export
`events.jsonl` → cleanup; corre solo en cierres reales (`Finish` y promoción —
el conocimiento del intento corto es conocimiento), jamás en pausa ni
cancelación. Bajo `isolation: worktree` el engine commitea a la rama del run
(`docs(knowledge): distill from <run_id>` — el conocimiento viaja en el mismo
PR que el trabajo y pasa la misma revisión humana) y pushea solo si ya hay
upstream; el `cleanup` con `git branch -d` (jamás `-D`) garantiza que un
commit de distill sin mergear no muere con el cierre. Bajo `none` los archivos
quedan sin commitear — el engine jamás commitea la rama del usuario; la
fricción del árbol sucio en el próximo run es deliberada y visible. Racional
del "jamás agente": §11.1 ya fija para el pegamento de cierre "solo comandos,
nunca IA"; un destilado escrito por LLM al cierre sería contenido no
verificable entrando a la capa de conocimiento sin criterio ni scope — la
puerta trasera exacta contra "la palabra del agente no es evidencia" — y A8
exige que el cierre corra con mock. Un equipo que quiere un resumen redactado
lo produce con un nodo `prompt` (runner, presupuesto y verificación propios) y
nombra ese artifact en `distill` — composición, no mecanismo nuevo. Los
findings nunca se auto-destilan: un nodo consolidador los escribe como
artifact `kind: findings` y el workflow lo declara.

Descartado: sesión de agente en el cierre; índice global compartido;
auto-destilar findings sin declaración del workflow.
