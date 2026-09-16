---
number: D58
title: "Onboarding: `init`, `new` y `pack add` con propósitos disjuntos; no interactivo por default"
status: accepted
revises: []
revised_by: []
---

# D58 — Onboarding: `init`, `new` y `pack add` con propósitos disjuntos; no interactivo por default

Tres verbos que nunca se cruzan: **`init` prepara el repo** (una vez; detecta
lenguaje, comando de test, rama base y CLIs disponibles vía `probe()`; escribe
`.yunta/config.yaml` y `.gitignore`; separa lo del equipo del repo de lo
personal en `~/.yunta/`), **`new` crea contenido propio** (escribe
`.yunta/workflows/<n>.yaml` a partir de esqueletos de schema mínimos — un nodo
con criterio y scope, cadena lint→fix, ledger vacío — comentados y editables,
más cerca de `cargo new` que de un workflow real; corre `check` al final), y
**`pack add` trae contenido ajeno** (versionado, pineado en el lock, de solo
lectura). Regla que evita la confusión: **`new` no acepta referencias a
packs** — no instala, no toca el lock, no deja procedencia; lo que crea es del
equipo desde el primer segundo. Partir de un workflow ajeno es copiarlo
explícitamente (futuro `pack eject` si alguna vez molesta). Los esqueletos de
`new` no contradicen D57: son papel rayado de schema, no flujos con opinión de
proceso — los flujos ejecutables siguen siendo packs.

**Modo de interacción: no interactivo por default, `--interactive`/`-i` para
el asistente.** Razón: los comandos deben funcionar igual en CI, contenedores
sin TTY y cuando los invoca un agente — que un default interactivo cuelga. Sin
TTY, `-i` degrada a no interactivo con aviso (nunca cuelga esperando input).
El default detecta todo lo que puede, escribe con defaults sensatos, informa
qué hizo y sugiere `-i` para ajustar.

Descartado: interactivo por default con `--yes` para saltearlo (invierte la
carga sobre el caso automatizable, que es el más frágil).
