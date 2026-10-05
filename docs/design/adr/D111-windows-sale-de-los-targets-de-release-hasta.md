---
number: D111
title: "Windows sale de los targets de release hasta que la capa de procesos sea portable"
status: accepted
revises: []
revised_by: []
---

# D111 — Windows sale de los targets de release hasta que la capa de procesos sea portable

La matriz de release y `compatibility.md` dejan de nombrar
`x86_64-pc-windows-msvc`; `yunta-engine` falla en compilación fuera de `unix`
con un `compile_error!` que explica por qué. Windows vuelve como target cuando
exista una implementación de la capa de procesos con las mismas garantías (job
objects para el exterminio del árbol, liveness de locks) y sus tests de
cancelación pasen en un runner de Windows.

Racional: un binario publicado promete el Contrato completo; la gestión de
procesos — process groups, señales de interrupción y exterminio del árbol,
liveness de locks — está escrita contra POSIX, y un binario que deja procesos
huérfanos al cancelar es exactamente la degradación silenciosa que I11
prohíbe. Retirar el target es explícito, reversible y no bloquea a nadie que
hoy no pueda usarlo.

Descartado: completar la portabilidad ahora (un módulo nuevo con su propia
superficie de tests y sin usuarios que lo pidan); publicar con una nota de
limitación (una nota no impide el proceso huérfano).
