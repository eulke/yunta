# D119 — `network: false` emite `capability_degraded` cuando el adapter no puede aplicarlo

**Estado:** propuesta.

## Contexto

`permissions.network: false` se acepta en config y nodos, se congela en el manifest y ningún adapter lo aplica: el nodo corre con red y nadie lo ve.

## Decisión propuesta

Un nodo con `network: false` cuyo adapter resuelto no declara la capacidad de aislar red emite `capability_degraded { policy_applied: declarative-only }` antes de la sesión, y el recibo lo muestra. `check` avisa cuando ningún candidato del runner declara la capacidad.

## Racional

I9 e I11: las capacidades se declaran y la degradación nunca es silenciosa. Rechazar en `check` sería excesivo mientras ningún adapter la implemente; ocultarlo es la opción prohibida.

## Alternativas descartadas

- Rechazar en `check`: deja inusable una clave que documenta una intención legítima.
- Emular el aislamiento en el engine: emulación de capacidad ausente.
