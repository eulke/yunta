---
number: D29
title: "`agent:` campo portable de primera clase"
status: accepted
revises: []
revised_by: []
---

# D29 — `agent:` campo portable de primera clase

Con capability `custom_agents`; cada adapter lo traduce a su mecanismo nativo,
`probe()` valida existencia.

Descartado: claves propietarias por adapter (`cc_*`) en `adapter_settings` —
ese campo queda solo para lo sin expresión portable.
