---
number: D51
title: "Modelo unificado de `permissions` con techos que solo se estrechan (Contrato §6.1)"
status: accepted
revises: []
revised_by: []
---

# D51 — Modelo unificado de `permissions` con techos que solo se estrechan (Contrato §6.1)

En lugar de una `policy:` separada, un solo modelo de permisos con escalera de
niveles: org → repo/usuario → pack (`declares.permissions`) → nodo (perfil) →
scope de tarea — todos con la misma semántica de techo. Precedencia invertida
deliberadamente respecto del resto de la config: la capa org manda y abajo
solo se restringe (sin inversión, la gobernanza es teatro). Contenido org:
`commands` (denylist default, allowlist opcional estricta), `packs` (executors
allow/prompt/deny, allowlist de publishers), `network`. Enforcement en check
(estático) y en runtime (templates construyen comandos que el YAML no
muestra). Límite documentado: gobernanza, no sandbox — el aislamiento real es
del entorno.

Descartados: concepto `policy:` separado (tercer nombre para lo mismo),
allowlist forzada desde v1 (mata adopción), y prometer sandboxing textual
(seguridad aparente).
