---
number: D109
title: "Capa `org` de knowledge: unión de los knowledge packs instalados; colisión entre packs = error (cierra la pregunta de precedencia de DI-31; completa el criterio org de T6.5 sobre el mecanismo de M11)"
status: accepted
revises: []
revised_by: []
---

# D109 — Capa `org` de knowledge: unión de los knowledge packs instalados; colisión entre packs = error (cierra la pregunta de precedencia de DI-31; completa el criterio org de T6.5 sobre el mecanismo de M11)

La fuente `knowledge` resuelve su capa `org` como la unión de todos los packs
vendoreados en `.yunta/packs/` cuyo `contents.knowledge` no esté vacío,
leyendo cada entrada declarada (archivo o directorio, recursivo) con la misma
regla que las capas `repo`/`user`. La precedencia entre capas no cambia: `org
< user < repo` — el merge por nombre de archivo existente (§9.2), donde lo más
local pisa a lo general, con lo cual un archivo org sigue siendo pisable por
el repo. Entre packs org no hay orden: dos packs instalados que shippean el
mismo nombre de archivo son un error tipado que nombra a ambos packs y al
archivo — jamás se resuelve por orden alfabético ni de instalación.

Racional: mismo criterio que la resolución namespaced de workflows (§5/T11.3:
la ambigüedad entre packs nunca se resuelve por orden de instalación) y que la
degradación explícita (A6) — una precedencia arbitraria haría desaparecer
silenciosamente un conflicto real de conocimiento org, contaminación no
auditable, exactamente lo que D56 descartó del RAG automático. El error solo
dispara cuando la capa org se resuelve de verdad (el default `knowledge: {}`
la incluye); `layers: [repo]` sigue sin tocar packs. La gobernanza de qué
publishers pueden instalarse pertenece a `pack add` (`permissions.packs`), no
a esta resolución, que toma lo vendoreado tal cual — nota:
`permissions.packs.publishers/executors` hoy parsea y mergea pero no se hace
cumplir en `add` (registrado como DI-32, fuera de este ADR).

Descartado: precedencia alfabética por `publisher/name` (determinista pero
arbitraria — el conflicto se enmascara en vez de resolverse); namespacing por
pack dentro del merge (rompería la regla de merge por nombre que las tres
capas comparten y un archivo org dejaría de poder ser pisado por el repo).
