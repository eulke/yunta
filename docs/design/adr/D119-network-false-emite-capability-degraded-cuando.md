---
number: D119
title: "`network: false` emite `capability_degraded` cuando el adapter no puede aplicarlo"
status: accepted
revises: []
revised_by: []
---

# D119 — `network: false` emite `capability_degraded` cuando el adapter no puede aplicarlo

Un nodo con `network: false` cuyo adapter resuelto no declara la capacidad de
aislar la red emite `capability_degraded { policy_applied: "declarative-only"
}` antes de la sesión, y el recibo lo muestra. `check` avisa cuando ningún
candidato del runner declara la capacidad.

Racional: I9 e I11 — las capacidades se declaran y la degradación nunca es
silenciosa. Rechazar en `check` sería excesivo mientras ningún adapter la
implemente; ocultarlo es la opción prohibida. La política sigue siendo
declarativa (D105): el evento registra que la política no se aplicó, no emula
un aislamiento que el engine no tiene.

Descartado: rechazar en `check` (deja inusable una clave que documenta una
intención legítima); emular el aislamiento en el engine (emulación de una
capacidad ausente).
