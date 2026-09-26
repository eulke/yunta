---
number: D110
title: "Claves desconocidas: rechazo en todo YAML de autor, tolerancia solo en lo persistido"
status: revised
revises: []
revised_by: [D168]
---

# D110 — Claves desconocidas: rechazo en todo YAML de autor, tolerancia solo en lo persistido

*(Revisada por D168: el alias `task-ledger` se retira del YAML de autor y del
CLI.)* Todo YAML escrito por una persona — la config en sus tres capas, los
workflows, `pack.yaml`, los casos de test, los fixtures del mock, los ledgers
y los artifacts de preguntas — rechaza una clave desconocida con un error que
nombra el archivo, la ruta completa de la clave y las claves válidas en ese
nivel. Todo lo que el engine persiste y vuelve a leer — eventos, manifest,
lock de packs — conserva el lector tolerante de D70.

Racional: parsear es validar; una clave mal escrita (`modes` por `mode`,
`depends-on` por `depends_on`) que el tipo ignora en silencio es un estado
inválido que entra por la puerta de atrás y el autor descubre recién cuando el
run se comporta distinto de lo que escribió. El costo del rechazo es cero para
el autor correcto y es la única forma de que el error aparezca en `check`,
antes de gastar un token. La tolerancia en lo persistido responde a otra
pregunta (compatibilidad N/N-1 del log) y no se mezcla con la frontera de
autoría.

Descartado: warning en `check` por clave desconocida (un warning que nadie lee
es un error diferido); tolerancia uniforme (conserva la fuente de errores
silenciosos).
