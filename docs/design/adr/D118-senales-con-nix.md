# D118 — Señales a process groups con el crate `nix`

**Estado:** propuesta.

## Contexto

El engine y los tests de integración del CLI envían señales a process groups invocando el binario `kill`: dependen de un binario externo en el `PATH` y pierden el código de error de la llamada.

## Decisión propuesta

Depender de `nix` con las features mínimas (`signal`, `process`) en `yunta-engine` y usarlo en tests; toda señal devuelve un error tipado y `forbid(unsafe_code)` rige en el workspace entero, tests incluidos.

## Racional

Una llamada de sistema envuelta con tipos es exactamente el caso en que una dependencia se justifica: reemplaza un subproceso, conserva `forbid(unsafe_code)` y su superficie es mínima.

## Alternativas descartadas

- `libc` directo: exige `unsafe` en código propio.
- Conservar el binario `kill`: sin errores tipados y dependiente del `PATH`.
