---
number: D42
title: "Ensamblado de contexto estable-primero (cache-consciente)"
status: accepted
revises: []
revised_by: []
---

# D42 — Ensamblado de contexto estable-primero (cache-consciente)

Toda fuente tiene clase de estabilidad (`stable` / `run-stable` / `volatile`);
el engine ensambla siempre estable → run-estable → volátil → prompt, con
serialización canónica y sin contenido no determinista en segmentos estables,
para que las rehidrataciones compartan prefijo byte-idéntico y el prompt
caching del proveedor aplique. Evento `context_assembled` con hashes por
segmento como verificación mecánica. El engine no gestiona el cache (es del
proveedor/CLI): garantiza la condición.

Descartado: tratarlo como capability del adapter con degradación — es
responsabilidad del engine y funciona con cualquier adapter.
