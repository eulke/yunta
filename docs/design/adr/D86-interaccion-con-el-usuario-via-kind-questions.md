---
number: D86
title: "Interacción con el usuario vía `kind: questions`, no vía nodos conversacionales (Contrato §4.1)"
status: revised
revises: []
revised_by: [D156, D157, D173]
---

# D86 — Interacción con el usuario vía `kind: questions`, no vía nodos conversacionales (Contrato §4.1)

*(Revisada por D156: el documento entra por `yunta_submit_questions`. Revisada
por D157: las respuestas son una aceptación más del log, proyectada bajo su
nodo, y la ronda las relee por ahí. Revisada por D173: el hecho de preguntar
es `questions_asked`, par de `questions_answered`, y el nodo espera entre los
dos sin segundo `node_started`; un nodo que declara `questions` no declara
otro artifact, e `interactive` se retira: la superficie disponible decide cómo
se presentan las preguntas.)* Un nodo que necesita información del usuario
escribe un artifact de preguntas (id, text, answer_type, values, required) y
**termina**; el engine lo renderiza según la superficie disponible — terminal
con TTY pregunta por pregunta, sin TTY o desde MCP como el objeto de gate de
§5.3 — y las respuestas quedan como artifact que el nodo siguiente consume
como contexto. `interactive: true` pasa a ser un dato de presentación, no un
modo de ejecución. Ventajas sobre las alternativas: no requiere capacidad de
adapter (todo CLI escribe archivos, ninguno degrada), preserva resumibilidad y
ejecución desatendida (si el run muere durante la espera, las preguntas están
en disco y se rehacen; no hay conversación a medias que reconstruir), unifica
toda interacción humana en una sola maquinaria (`HumanInteraction`), y en CI
queda `waiting` de forma natural.

Descartados: nodo conversacional con terminal cedido (rompe resumibilidad y
exige capacidad de adapter) y resolver el caso solo con gates de texto libre
(funcional pero pierde estructura y usabilidad).
