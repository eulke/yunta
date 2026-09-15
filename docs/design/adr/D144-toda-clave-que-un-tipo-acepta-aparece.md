---
number: D144
title: "Toda clave que un tipo acepta aparece en el ejemplo publicado, y toda clave del ejemplo tiene su propio diagnóstico"
status: accepted
revises: []
revised_by: []
---

# D144 — Toda clave que un tipo acepta aparece en el ejemplo publicado, y toda clave del ejemplo tiene su propio diagnóstico

Dos tests encadenados sobre `Document`: el primero afirma que el ejemplo
escribe cada clave de `REQUIRED ∪ OPTIONAL`; el segundo toma ese mismo
ejemplo, le pone a cada clave un valor del tipo equivocado y afirma que el
recorrido produce un diagnóstico que la nombra, nunca un
`Problem::Unreadable`.

Racional: un agente escribió bien todas las claves para las que el ejemplo le
daba un literal y erró la única que el ejemplo mencionaba solo en un
comentario — no como descuido sino igual en las tres tareas, que es lo que
hace una inferencia y no un desliz. La segunda mitad cubre el caso simétrico:
`justification` y el `cmd` de `proposed_criterion` se aceptaban sin chequeo de
recorrido, así que escribirlas mal caía a la prosa cruda de serde, lo único
que esta frontera existe para evitar. Las dos sondas son `true` y un mapping
vacío: entre las dos son del tipo equivocado para toda forma que una clave
puede tener, así que no hay tabla por clave que mantener.

Descartados: derivar las listas de claves de los tipos en runtime (mete la
generación de schemas en el binario, D124/D139); revisar la correspondencia a
ojo (es la disciplina que ya había fallado).
