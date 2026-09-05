# Estado abierto

Lo que sigue abierto y necesita una sesión o una decisión. Lo cerrado no se registra acá: el código, sus tests y el registro de decisiones son su evidencia.

## Abierto

### Verificación en vivo

Los adapters `codex` y `claude-code`, la forja de GitHub, `yunta mcp` montado en un cliente real, el MCP por-run con un agente real y `pack add` contra un host remoto están construidos contra documentación y ejemplos reales, sin corrida en vivo. La [checklist](smoke-checklist.md) describe cada corrida y su protocolo de corrección: cada divergencia es una tarea propia con test de regresión. Requiere binarios autenticados y un token con repo descartable.

### Distribución pública

Bloqueado en decisiones explícitas porque cada paso es una acción pública o difícil de revertir:

- Publicar a crates.io, en orden de dependencia.
- Crear el repositorio del tap de Homebrew y completar `update-tap` en `release.yml`.
- Crear el repositorio de la acción compuesta `setup-yunta`.
- Sitio de documentación público y separación de los packs de fábrica en repositorios propios.
- Empujar el primer tag `vX.Y.Z`, que dispara todo lo anterior.

### Deuda consciente

Lo deliberadamente diferido —cada ítem requiere una decisión registrada antes de codearse— vive en [`deuda-consciente.md`](deuda-consciente.md): A-01…A-12. Entre ellos, A-06 registra la firma criptográfica (packs, recibos y la cadena de eventos del log) como la capa de autoría, separada de la integridad que el hash chain ya da, con su forma propuesta.

## Posturas cerradas

Restricciones que se mantienen por decisión; reabrirlas requiere una decisión registrada.

- **Gate dentro de `parallel`: rechazado.** La resolución de un gate es un round-trip humano o de forja de a uno; ninguna semántica de `join` está definida para eso. El error de `check` es la feature.
- **Consenso multi-reviewer: fuera del engine.** La última revisión decisiva gana; cuántas aprobaciones hacen falta es política de la forja (branch protection). Duplicarlo crearía dos fuentes de verdad sobre la misma pregunta.
- **`ForgeKind` con una sola variante.** El punto de extensión existe (trait más enum cerrado); un segundo forge se agrega con demanda real, no de forma especulativa.
- **`include:` de modos solo nombra nodos de primer nivel.** Un grupo `parallel` entra o sale entero; nombrar un hijo es error de `check`.
- **Paths de artifacts en gates externos** relativos a `run.dir/artifacts/`, mismo path relativo en la rama: convención documentada en la guía, no un mecanismo.
- **Clasificación de modo "nodo temprano + gate"** se expresa con composición: arrancar en el modo piso, un nodo temprano propone, un gate interno confirma, la promoción escala. No existe un tercer mecanismo que mute el modo de un run en curso.
