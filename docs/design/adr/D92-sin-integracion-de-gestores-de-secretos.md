---
number: D92
title: "Sin integración de gestores de secretos: composición por entorno (Contrato §6.3)"
status: accepted
revises: []
revised_by: []
---

# D92 — Sin integración de gestores de secretos: composición por entorno (Contrato §6.3)

Los secretos llegan solo por env vars declaradas en el manifest, redactadas de
todo payload y ausentes del event log (I12). El gestor que el equipo ya usa
puebla el entorno antes de invocar el binario (`op run -- yunta run …`, `vault
exec …`); Yunta consume env vars como cualquier herramienta de línea de
comandos. Decisión, no omisión: integrar gestores significaría un adapter por
proveedor con su propia autenticación y pondría al engine a custodiar
credenciales — lo que convierte una herramienta sin infraestructura en algo
que un área de seguridad debe auditar (contra D05). La composición con el
gestor existente es más simple, más auditable y funciona con cualquiera,
incluidos los que todavía no existen.
