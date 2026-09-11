# Corpus de diseño

Documentos normativos del engine, en español. Cada uno manda sobre lo suyo; ante una contradicción entre código y documento, el documento gana y el código se corrige, salvo que una decisión registrada diga lo contrario.

Orden de autoridad:

1. [`contrato-del-run.md`](contrato-del-run.md) — comportamiento del engine e invariantes del run.
2. [`spec-adapter.md`](spec-adapter.md) — traits `Adapter`/`AgentSession`, capacidades y obligaciones de un adapter.
3. [`spec-ledger.md`](spec-ledger.md) — schema del ledger de tareas, validación y errores.
4. [`spec-events.md`](spec-events.md) — payloads del event log, versionado y cadena de hashes.
5. [`adrs.md`](adrs.md) — decisiones aceptadas con racional y alternativas descartadas; fuente de desempate. Las propuestas pendientes viven en [`adr/`](adr/).
6. [`rfc-0001.md`](rfc-0001.md) visión y arquitectura · [`rfc-0002.md`](rfc-0002.md) packs · [`rfc-0003.md`](rfc-0003.md) diferenciales de producto · [`rfc-0004.md`](rfc-0004.md) distribución, licencia y sostenibilidad.
7. [`referencia-schema.md`](referencia-schema.md) — config y workflows canónicos; los fixtures de parseo del workspace salen de acá.
8. [`deuda-consciente.md`](deuda-consciente.md) — lo deliberadamente no resuelto; cada ítem se resuelve con una decisión registrada, nunca de facto.

Complementos:

- [`glosario.md`](glosario.md) — los términos del dominio, con la palabra que se usa y las que se evitan.
- [`status.md`](status.md) — lo que sigue abierto y las posturas cerradas que no son deuda.
- [`smoke-checklist.md`](smoke-checklist.md) — verificación en vivo de adapters, forja y MCP contra sistemas reales.

La documentación de uso, en inglés, vive un nivel arriba en [`docs/`](../README.md).
