---
number: D71
title: "El audit de packs inventaría, no juzga: sin detección de patrones en prompts"
status: accepted
revises: []
revised_by: []
---

# D71 — El audit de packs inventaría, no juzga: sin detección de patrones en prompts

`yunta pack audit` muestra los **prompts completos** de los workflows del pack
junto al resto del inventario (comandos, fuentes de contexto, permisos,
agentes, mcp_servers, executors) — inventario, nunca veredicto. Descartada la
detección de patrones sospechosos en lenguaje natural: es trivialmente
evadible (parafraseo, otro idioma, instrucciones repartidas entre archivos) y
produciría falsa sensación de seguridad — la gente dejaría de leer los prompts
porque "el audit los revisó", que es la seguridad aparente que §6.1 rechaza.
La mitigación real es estructural y ya existe: el agente no puede marcar
tareas (I5), no sale del scope (I13), no ve secretos no declarados (I12), y el
techo `declares.permissions` del pack lo acota. El daño posible queda en
"escribir código dentro del scope declarado" — el mismo riesgo de cualquier
dependencia que se instala. La documentación lo dice sin eufemismos: los
prompts de un pack son código ajeno; leelos con el mismo criterio que
cualquier dependencia.
