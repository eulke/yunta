---
number: D93
title: "Rendimiento de la verificación: el sistema audita su propia ceremonia (Contrato §8.7)"
status: accepted
revises: []
revised_by: []
---

# D93 — Rendimiento de la verificación: el sistema audita su propia ceremonia (Contrato §8.7)

El engine aplica al workflow el mismo criterio que aplica al trabajo — si algo
no puede fallar, no está probando nada — y detecta sobre el histórico la
verificación que dejó de rendir: criterios nunca rojos en pre-check, re-rutas
nunca disparadas, gates siempre aprobados sin ajuste, modos que nadie elige,
tareas que siempre pasan al primer intento. **La métrica núcleo es la tasa de
rojo en pre-check, no la de fallos totales**: confundirlas haría que el
sistema sugiriera borrar los criterios que mejor funcionan (rojo antes, verde
después es el comportamiento correcto, no una anomalía). Cada hallazgo muestra
el conteo que lo sostiene y, para los criterios, **las dos lecturas posibles**
(redundante o mal escrito) — nunca una sola. Superficies: `stats --workflow` y
también `yunta check`, para que el hallazgo llegue cuando alguien ya está
editando ese workflow y puede actuar. Tres guardas: **sugiere y jamás actúa**
(un engine que se quita verificación a sí mismo vacía de sentido al recibo),
**nunca toca `invariant: true`** (su valor no se mide en frecuencia de fallo),
y **evidencia suficiente por criterio**, no por workflow. Racional de fondo:
un sistema que no puede volverse más liviano cuando la realidad mejora se
convierte en burocracia — y a medida que los ejecutores se vuelvan más
confiables, la ceremonia que hoy es necesaria dejará de serlo en partes.
