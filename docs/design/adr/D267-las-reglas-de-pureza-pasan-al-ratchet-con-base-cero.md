---
number: D267
title: "Las reglas de pureza pasan al ratchet, con base cero"
status: accepted
revises: [D257]
revised_by: []
---

# D267 — Las reglas de pureza pasan al ratchet, con base cero

## Contexto

Las reglas que miran el código del engine sin ejecutarlo vivían en
`crates/engine/tests/purity.rs`: reloj, entorno, git y disco. Cada una era una
prueba que lee los fuentes y falla cuando una línea nueva cruza la frontera. Al
mismo tiempo, `cargo xtask smells` (el ratchet) ya medía otras reglas de esa misma
forma, con contadores, dueños y una línea base. Dos mecanismos para una clase de
regla dejaban dos sitios donde corregir una exención, y la prueba y el contador
podían decir cosas distintas del mismo árbol.

## Decisión

1. Las cuatro reglas pasan al ratchet, cada una con su contador y sus dueños:
   `system_clock_outside_boundary` (el reloj), `env_read_outside_boundary` (el
   entorno), `git_command_outside_git_rs` (git) y `engine_fs_outside_runtime`
   (disco). `crates/engine/tests/purity.rs` se borra; ninguna regla queda en dos
   lugares.
2. Cada contador lleva base cero en `xtask/smells.baseline`. Un contador que llega a
   cero se lista en `ZERO_COUNTERS`, en la prueba
   `the_tree_holds_each_counter_that_reaches_zero_at_zero`, y no puede subir sin que
   `cargo xtask smells --check` falle.
3. La exención del shell (`process.rs`, `process/` y `process_registry.rs`) y la de
   los módulos sin runtime (`catalog.rs`, `check/` y `pack_audit.rs`) pasan a
   `engine_fs_outside_runtime`. Una llamada de disco se justifica con un
   `// blocking:` en la línea de arriba, y la justificación cubre la primera línea de
   código debajo de ese comentario.
4. La regla de disco asíncrono (`sync_fs_in_async`) cuenta solo producción. No cuenta
   lo que `// blocking:` justifica ni lo que va dentro de `spawn_blocking(`.
5. Un `let _ =` o un `.ok();` de sentencia en producción se cuenta, salvo que la
   línea de arriba sea `// best-effort:`.
6. Revisa a D257 en su punto 1: el spawn de git fuera de `engine/src/git/` ya no lo
   rechaza «el test de pureza», sino el contador `git_command_outside_git_rs`, que
   el ratchet sostiene en cero. La decisión de un solo dueño para git no cambia.

## Racional

Un contador tiene dos propiedades que una prueba no tiene: se mide en la misma
corrida que las demás reglas del repositorio, y su número queda escrito en el
baseline, así que una excepción nueva es un cambio visible en un archivo y no una
lista que crece sin que nadie la note. La base cero convierte cada regla en una
prohibición: el contador no admite ningún sitio más que los justificados, y los
justificados se nombran donde están.

## Alternativas descartadas

**Mantener `purity.rs` junto al ratchet.** Dos mecanismos para una regla: una
exención se corregiría en dos lugares, y la prueba y el contador podían disentir
sobre el mismo árbol.

**Contadores con una línea base mayor que cero.** Una cuenta aceptada alta vuelve la
regla un número que se negocia en cada cambio. La base cero obliga a justificar cada
sitio en su propio código.

## Consecuencias

- `cargo xtask smells --check` es el único lugar donde se verifica esta frontera. CI
  lo corre, y un cambio que la cruza falla allí con el nombre del contador y del
  archivo.
- Un `std::fs::` nuevo fuera del shell y de los módulos sin runtime falla el ratchet
  aunque ningún test lo ejercite, porque la regla lee el código y no un camino de
  ejecución.
- Los sitios justificados sin canal de run quedan listados con su razón en
  `deuda-consciente.md`, en la sección `## Sitios justificados`. No graban nada en el
  log, y así lo dice esa sección.
