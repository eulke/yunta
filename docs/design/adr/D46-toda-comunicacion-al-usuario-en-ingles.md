---
number: D46
title: "Toda comunicación al usuario en inglés"
status: revised
revises: []
revised_by: []
---

# D46 — Toda comunicación al usuario en inglés

*(Revisada: la salida viva ya no se nombra por un flag — D162 saca
**`--follow`** y hace la vista viva el default de `yunta run`.)* Amplía D40:
no solo código, CLI, errores y commits — también README, documentación de uso,
guías, textos de ayuda, salida de `status`/`stats`/la vista viva de `run`,
dashboard y cualquier superficie UI/UX de la herramienta.

Razón: la audiencia de Yunta es el ecosistema (packs compartidos, adapters de
terceros, equipos heterogéneos), y una herramienta con superficie mixta de
idiomas se percibe inconsistente y local. Única excepción vigente: los
documentos de diseño internos (este corpus, en `docs/design/`), que son del
equipo y permanecen en español.

Descartado: bilingüe (doble mantenimiento, drift entre versiones garantizado).
