---
number: D80
title: "Un solo schema de hallazgo, dos vías de entrada; toda denegación de scope deja finding"
status: accepted
revises: []
revised_by: []
---

# D80 — Un solo schema de hallazgo, dos vías de entrada; toda denegación de scope deja finding

(a) El formato de §4.1 rige tanto para findings producidos como artifact al
cerrar un nodo como para los reportados en caliente vía `yunta_post_finding`:
no hay dos calidades según el momento — se cuentan, deduplican y consultan
juntos, y la vía en caliente valida contra el mismo schema (reportar
incompleto es error visible, no texto libre inprocesable). (b) Toda solicitud
de ampliación de scope denegada — por regla, persona, cap agotado o modo
`deny` — se convierte automáticamente en `finding_posted` con la razón y el
criterio que el agente ya escribió: cubre el caso más frecuente de hallazgo en
caliente sin depender de que además se acuerde de reportarlo. (c)
Explícitamente NO se impone un artifact de findings al cierre: el destino
garantizado ya lo dan el event log y el `events.jsonl` exportado, y qué hacer
con los hallazgos (consolidar, promover, destilar, ignorar) es decisión del
workflow — un artifact obligatorio sería el engine acoplándose a un proceso
concreto, contra D57. El engine garantiza que no se pierdan y que sean
consultables; nada más.
