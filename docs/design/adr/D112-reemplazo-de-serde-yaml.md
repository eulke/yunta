# D112 — Reemplazo de `serde_yaml`

**Estado:** propuesta.

## Contexto

Todo YAML de autor y varios artifacts se parsean con `serde_yaml`, un crate archivado por su autor y sin mantenimiento. Los advisories futuros no tendrán fix y el ecosistema migra a forks con API compatible.

## Decisión propuesta

Migrar a un fork mantenido con API compatible en todas las fronteras de parseo, validado por los fixtures de round-trip de `crates/core/tests/fixtures/`, que definen el comportamiento que el reemplazo debe conservar byte a byte en serialización.

## Racional

Un parser sin mantenimiento en la frontera de entrada de todo el sistema es un pasivo de seguridad que crece con el tiempo. Los fixtures de referencia existen precisamente para que un cambio así sea verificable mecánicamente.

## Alternativas descartadas

- Mantener `serde_yaml` con `ignore` en cargo-deny: pospone el costo y lo agranda.
- Cambiar el formato de autoría: el YAML es el contrato con los usuarios y los packs.
