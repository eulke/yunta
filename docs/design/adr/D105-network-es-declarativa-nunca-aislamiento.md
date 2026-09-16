---
number: D105
title: "`network` es declarativa; nunca aislamiento de sistema operativo (Contrato §6.1)"
status: accepted
revises: []
revised_by: []
---

# D105 — `network` es declarativa; nunca aislamiento de sistema operativo (Contrato §6.1)

A diferencia de `commands` (que el engine sí compara contra patrones en
runtime), `network: false` no activa sandboxing: es una declaración para
política y auditoría, exigible solo si un `executor` concreto decide
implementarla. Tres capas separadas y nombradas: policy (el YAML) ≠ capability
(lo que un executor puede hacer cumplir) ≠ OS enforcement (garantía física,
nunca de Yunta). Generaliza §6.1 ("gobernanza, no sandbox") al caso específico
donde más se presta a malentendido.

Descartado: implementar enforcement de red en el core (contradice D05,
arrastra al engine a custodiar aislamiento que no le corresponde).
