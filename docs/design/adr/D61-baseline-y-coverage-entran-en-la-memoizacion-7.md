---
number: D61
title: "Baseline y coverage entran en la memoización (§7)"
status: revised
revises: []
revised_by: [D176]
---

# D61 — Baseline y coverage entran en la memoización (§7)

*(Revisada por D176: un linaje paga la suite una vez y todo run nace
teniendo la medición de la raíz; `baseline_compare` reutiliza por el memo
de §5.4 y `coverage_gate` mide cada vez, porque su veredicto lee la
salida.)* Mismo mecanismo de §5.4: son comandos caros y deterministas respecto del
árbol. Un workflow con varios `baseline_compare` no paga la suite dos veces
sobre el mismo árbol; la reutilización se registra como tal. Extensión de un
mecanismo existente a dos llamadores más, sin diseño nuevo.
