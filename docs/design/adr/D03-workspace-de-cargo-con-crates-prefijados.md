---
number: D03
title: "Workspace de Cargo con crates prefijados"
status: accepted
revises: []
revised_by: []
---

# D03 — Workspace de Cargo con crates prefijados

Layout `crates/{core,storage,adapters,engine,cli}` con packages `yunta-core`,
`yunta-storage`, `yunta-adapters`, `yunta-engine` y `yunta` (binario). Las
fronteras las impone el compilador: `yunta-engine` no puede importar SQLite ni
conocer un CLI concreto porque no están en sus dependencias — A1 y D53 dejan
de depender de la disciplina de PR. Suma compilación incremental por crate y
la opción de publicar el engine para embeberlo. Packages prefijados porque
crates.io es namespace plano (`core`/`engine` no son publicables); directorios
sin prefijo por convención; binario sin sufijo porque es el comando que se
instala. La distribución no cambia: un único binario estático.

Descartados: crate único con módulos (fronteras por disciplina) y packages sin
prefijo (impublicables).
