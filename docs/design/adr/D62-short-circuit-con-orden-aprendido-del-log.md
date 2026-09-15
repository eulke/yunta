---
number: D62
title: "Short-circuit con orden aprendido del log"
status: revised
revises: []
revised_by: [D167]
---

# D62 — Short-circuit con orden aprendido del log

*(Revisada por D167: el orden aprendido del log se construye; el código lo
aprendía por invocación sin decisión.)* El engine evalúa los criterios del
pre-check de menor a mayor duración histórica (dato ya presente en el log) y
corta al primer no-`guard` en rojo. La heurística afecta solo el orden de
evaluación, jamás qué se verifica ni el veredicto — si el orden influyera en
el resultado, sería un bug de determinismo del criterio (§5.1), no de la
heurística.
