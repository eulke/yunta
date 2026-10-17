---
number: D108
title: "Montaje cross-run de artifacts por vínculo: lo declara el padre (`mounts:`), la entrega es copia al nacer el hijo (Contrato §12; cierra DI-26)"
status: revised
revises: []
revised_by: [D157]
---

# D108 — Montaje cross-run de artifacts por vínculo: lo declara el padre (`mounts:`), la entrega es copia al nacer el hijo (Contrato §12; cierra DI-26)

*(Revisada por D157: la fuente es el log y el store del run de origen, no su
`artifacts/`; un mount nombra una identidad —`kind:` o `name:`, con `as:` solo
para un opaco— y la entrega es una aceptación del hijo con `origin:
inherited`. Un artifact que el run de origen no tiene falla el nodo como
`Unheld`.)* Sintaxis: `mounts: [{artifact: {node, name, as?}}]`, solo en nodos
`kind: workflow`. `node` nombra cualquier nodo del propio padre: si es otro
nodo `kind: workflow`, la fuente es el `run.dir/artifacts/` del último hijo
vinculado de ese nodo que llegó a terminal (`child_run_finished` en el log del
padre) — la resolución camina exclusivamente el grafo de vínculos del propio
log, con lo cual la regla de §12 "nadie monta artifacts de runs ajenos" pasa
de cumplirse por construcción a estar verificada; si es un nodo común, la
fuente es el artifact del propio padre. Cada mount implica `depends_on` sobre
el nodo referido (la misma expansión implícita que `context: artifact:` ya
usa), lo que garantiza gratis el "hermanos terminados" de §12 y deja los
ciclos vía mount cubiertos por la detección de ciclos existente. Entrega:
copia a `run.dir/artifacts/` del hijo al nacer (con `as:` como renombre
opcional) — el mecanismo idéntico de la herencia por promoción, generalizado,
que es exactamente lo que §12 anuncia ("la promoción es un caso particular de
este mecanismo general"); igual que la promoción, sin evento nuevo: la
declaración queda congelada en el workflow del manifest del padre y la copia
es determinista desde ella. Fuente faltante (artifact declarado y nunca
producido, o hermano sin hijo terminal) → `node_failed` del nodo workflow con
diagnóstico accionable, jamás lectura silenciosa ni omisión. Consumo en el
hijo: `artifact: {name}` sin `node` en `context:` — "un artifact de mi
run.dir, lo haya producido quien sea, montado incluido"; el resolver ya lee
`run.dir/artifacts/<name>` directo, así que la forma sin `node` solo omite la
arista implícita — con lo cual el hijo queda paramétrico y jamás sabe que es
hijo. Los `inputs:` siguen siendo el canal para escalares y paths; `mounts` es
el canal tipado por vínculo. `check`: mount a nodo inexistente, a sí mismo o a
un nodo con `runners:` (fan-out — no hay "el" hermano una vez que son varios)
es error. Racional del lado padre: el acoplamiento a la topología es real y
quien la conoce es el padre; un workflow del catálogo que declarara `run:
sibling:<node>` quedaría atado a la topología de un padre concreto y rompería
su corrida standalone.

Descartado: selector de run declarado por el hijo (`artifact: {run: parent |
sibling:<node>, node, name}` — la candidata original de DI-26); resolución en
runtime del contexto del hijo contra el log del padre (invierte la dirección
de conocimiento que T9.3 mantiene: el hijo no sabe que es hijo); un segundo
mecanismo de entrega distinto del de promoción.
