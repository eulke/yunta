---
number: D152
title: "El directorio de artifacts del run se audita al cierre, como se audita el worktree"
status: retired
revises: []
revised_by: [D157]
---

# D152 — El directorio de artifacts del run se audita al cierre, como se audita el worktree

*(Retirada por D157: el directorio compartido que la justificaba dejó de ser
la respuesta a qué artifacts tiene un run.)* `ArtifactsSnapshot` lee
`artifacts/` antes de abrir la sesión de un nodo y el cierre compara: todo
archivo agregado, cambiado o borrado que el nodo no declara en
`artifacts.produces` lo falla, nombrándolo.

Racional: los dos CLIs conceden escritura por directorio, nunca por archivo,
así que conceder el artifact declarado de un nodo (D149) concede de hecho el
de todos los demás — `artifacts/` es uno solo por run. El worktree tiene
exactamente esa forma y el run ya la contesta así: la sesión puede escribir y
el cierre audita contra lo declarado (`scope_checked`); no hay razón para que
la segunda superficie escribible del run se gobierne distinto de la primera.
Un nodo sin artifacts declarados queda auditado contra el conjunto vacío, que
es la misma regla que `read-only` aplica al worktree (D150). El archivo de
respuestas que el engine escribe al lado de un artifact `questions` queda
excluido por sufijo: los nodos de un grupo `parallel` se interleavean, así que
esa escritura puede caer durante la sesión de un hermano, y es del engine, no
del hermano.

Descartados: aislar los artifacts por nodo en subdirectorios (`artifacts/`
plano es contrato normativo del run, D108 se apoya en que un artifact sea
direccionable por nombre sin nodo, y los packs publicados y el scaffold de
`yunta new` traen esa ruta escrita a mano); atribuir cada escritura a su autor
llevando un registro de las del engine (cierra también la ventana del archivo
de respuestas, a costa de un mecanismo nuevo que hoy nada más necesita); un
evento `artifacts_checked` propio (el `node_failed` ya lleva el diagnóstico
entero, y agregar un `kind` a `spec-events.md` es tocar un documento normativo
para un caso que no lo pide).
