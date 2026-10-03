---
number: D91
title: "Estimación previa derivada del histórico, informativa y con advertencia de presupuesto (Contrato §8.6)"
status: accepted
revises: []
revised_by: []
---

# D91 — Estimación previa derivada del histórico, informativa y con advertencia de presupuesto (Contrato §8.6)

Antes de arrancar, el engine muestra la distribución observada de runs pasados
del mismo workflow (mediana y p90 de tokens y wall-clock) en `yunta run` y en
`list_workflows`, donde puede cambiar qué elige un agente cliente. Nunca
bloquea; lo accionable es la advertencia cuando el presupuesto declarado queda
bajo el p90 histórico — un run que se detiene a mitad por un límite mal
elegido es el desperdicio más caro. **Silencio con menos de tres runs**: un
número sin distribución detrás es adivinanza con apariencia de dato.
Descartada la estimación estática (contar nodos por un promedio genérico), por
la misma razón por la que el progreso usa contadores y no porcentajes (D45):
fabricar precisión que no se tiene erosiona la confianza en todo lo demás que
el sistema informa.
