---
number: D61
title: "Baseline y coverage entran en la memoización (§7)"
status: accepted
revises: []
revised_by: []
---

# D61 — Baseline y coverage entran en la memoización (§7)

Mismo mecanismo de §5.4: son comandos caros y deterministas respecto del
árbol. Un workflow con varios `baseline_compare` no paga la suite dos veces
sobre el mismo árbol; la reutilización se registra como tal. Extensión de un
mecanismo existente a dos llamadores más, sin diseño nuevo.
