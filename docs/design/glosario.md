# Glosario

Los términos del dominio con los que se discute el engine, cada uno con la
palabra que se usa y las que se evitan. Un término entra acá cuando el corpus
ya lo apoya en más de un lugar: definirlo tarde es lo que deja que dos partes
del sistema entiendan cosas distintas por la misma palabra.

Las reglas de escritura y la tabla de sustituciones de vocabulario
(`adapter` en lugar de `driver`, `runner:` en lugar de `role:`) viven en
[`CLAUDE.md`](../../CLAUDE.md) y no se repiten acá.

## La frontera de autoría

**YAML de autor**:
Un documento que escribe una persona antes de que exista un run: los tres
layers de config, los workflows, `pack.yaml`, los casos de test y los fixtures
del mock. `yunta check` lo alcanza antes de gastar un token.
_Evitar_: YAML de usuario, input.

**YAML de agente**:
Un documento que escribe un agente durante un run: el contenido de todo
artifact interpretado. No existe antes del run, así que ningún `check` lo
alcanza, y su primer lector es el agente que lo escribió.
_Evitar_: output estructurado, artifact de salida.

**YAML persistido**:
Lo que el engine escribe y vuelve a leer: eventos, manifest, lock de packs. Su
lector es tolerante con lo que no conoce (D70), porque responde a la
compatibilidad del log y no a la autoría.
_Evitar_: estado interno.

## Artifacts

**Artifact opaco**:
Un artifact del que el engine conoce existencia, tamaño y hash, y nada más. Es
el default: dos runs del mismo workflow pueden producir formatos distintos y
los dos son válidos.
_Evitar_: artifact sin tipo, blob.

**Artifact interpretado**:
Un artifact cuyo `kind:` declara que el engine parsea su contenido, lo valida y
lo convierte en eventos. Los kinds son `task-ledger`, `findings` y `questions`.
_Evitar_: artifact estructurado, artifact tipado.

## Fallas

**Diagnóstico**:
La descripción estructurada de una sola cosa que salió mal, con su sujeto en el
vocabulario del dominio, lo que se esperaba en su lugar y qué hacer. Se
construye una vez, viaja en el event log con su forma, y se redacta por
separado para cada lector.
_Evitar_: mensaje de error, outcome.

**Ciclo de reparación**:
El reintento de un nodo cuyo artifact interpretado no pudo leerse: la sesión se
reabre con los diagnósticos de lo que falló, contra un tope propio. Es la
contraparte del ciclo de tarea — aquél reintenta trabajo, éste reintenta una
declaración.
_Evitar_: retry, segunda pasada.
