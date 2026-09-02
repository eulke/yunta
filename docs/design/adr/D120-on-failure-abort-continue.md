# D120 — `defaults.on_failure: abort | continue` se implementan

**Estado:** propuesta.

## Contexto

La config acepta `defaults.on_failure` con tres valores y el scheduler solo aplica `pause`; `abort` y `continue` se aceptan y se ignoran.

## Decisión propuesta

Implementar los dos valores en el scheduler: `abort` cierra el run como fallido tras el primer nodo fallido sin re-ruta; `continue` sigue con los nodos cuyas dependencias no pasen por el fallido y cierra el run como fallido al final. Ambos emiten el evento correspondiente con el nodo causante.

## Racional

Un valor aceptado y no aplicado es una promesa vacía. Los dos comportamientos son pasos del scheduler puro con semántica obvia y testeable por replay.

## Alternativas descartadas

- Retirar los valores: son útiles en CI (`abort`) y en runs exploratorios (`continue`).
