# D111 — Windows sale de los targets de release hasta que la capa de procesos sea portable

**Estado:** propuesta.

## Contexto

La matriz de release incluye `x86_64-pc-windows-msvc`, pero la gestión de procesos del engine — process groups, señales de interrupción y exterminio del árbol, `kill -0` para liveness de locks — está escrita contra POSIX. El binario de Windows compila y no puede cumplir el Contrato en cancelación ni en locks.

## Decisión propuesta

Retirar el target de Windows de la matriz de release y de `compatibility.md`, con un `compile_error!` fuera de `unix` que explique por qué. Windows vuelve como target cuando exista una implementación de la capa de procesos con las mismas garantías (job objects) y sus tests de cancelación pasen en un runner de Windows.

## Racional

Un binario publicado promete el Contrato completo. Publicar uno que deja procesos huérfanos al cancelar es exactamente la degradación silenciosa que I11 prohíbe. Retirar el target es explícito, reversible y no bloquea a nadie que hoy no pueda usarlo.

## Alternativas descartadas

- Completar la portabilidad ahora: es un módulo nuevo con su propia superficie de tests y sin usuarios que lo pidan.
- Publicar con una nota de limitación: una nota no impide el proceso huérfano.
