---
number: D47
title: "Executors externos: contrato JSON por stdin/stdout; WASM descartado para v1"
status: accepted
revises: []
revised_by: []
---

# D47 — Executors externos: contrato JSON por stdin/stdout; WASM descartado para v1

El executor recibe por stdin un JSON con su input (`with:`, paths del run, env
declarado) y responde por stdout un JSON de resultado; el exit code es el
veredicto; timeout del engine. Razones: cualquier lenguaje puede escribir uno,
debuggeable a mano, simulable gratis por el mock. WASM daba
sandboxing/portabilidad pero cuesta toolchain y restringe lenguajes; el
sandboxing real lo dará la allowlist org (una sola palanca de seguridad, no
dos a medias). `kind: wasm` queda como extensión aditiva futura si los datos
la piden.
