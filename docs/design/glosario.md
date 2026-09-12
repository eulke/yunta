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
Un documento cuyo contenido nace en un run: el de todo artifact interpretado. No
existe antes del run, así que ningún `check` lo alcanza. Una sesión lo entrega
como objeto y el engine rinde el YAML; un nodo de comando escribe el archivo.
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

**Kind de artifact**:
El conjunto cerrado de documentos que el engine interpreta, y el tipo que lo
nombra en todas partes: el `kind:` de un workflow, el argumento de
`yunta schema`, el catálogo de la tool `document_shape` y el documento del que
habla un reporte son el mismo conjunto y el mismo tipo (D132).
_Evitar_: DocumentKind, tipo de documento, formato.

**Entrega** (*submission*):
Un documento entero que una sesión le pasa al engine por su tool
`yunta_submit_<kind>`, como objeto estructurado y nunca como archivo. El engine
lo valida con el tipo y las reglas del cierre y, si lo acepta, escribe él el YAML
canónico (D156).
_Evitar_: subida, escritura del artifact, guardado.

**Posteo**:
Un hallazgo que una sesión reporta solo, en el momento en que lo ve, por
`yunta_post_finding`. La unidad de validación es el hallazgo: un rechazo alcanza
a ese y a ninguno de los ya reportados. `yunta_update_finding` lo reemplaza entero
por id y `yunta_withdraw_finding` lo retira con motivo, definitivamente (D156).
_Evitar_: entrega de findings, envío.

**Archivo derivado**:
El artifact `findings` de un nodo `prompt` o `loop`: lo escribe el engine al
cierre como proyección de lo que ese nodo reporta, no la sesión. Un nodo que no
reporta nada obtiene una lista vacía, que es el resultado de una revisión sin
hallazgos.
_Evitar_: artifact de salida, volcado.

**Conjunto efectivo**:
Los hallazgos que un log deja en pie: el último estado de cada par `(nodo, id)`,
sin los retirados, en el orden en que cada uno se posteó por primera vez. Lo
calcula un único pliegue, `events::findings::FindingLedger`, del que leen la
derivación, la herencia entre nodos, la destilación y las estadísticas — con tres
eventos por hallazgo, un segundo pliegue es una segunda respuesta.
_Evitar_: findings vigentes, lista final.

## Documentos y su lectura

**Documento**:
Un artifact interpretado visto desde el tipo que lo lee. Cada kind reúne en un
lugar todo lo que sabe de sí misma: su forma publicada, cómo explica una
lectura fallida y las reglas que solo valen sobre el documento entero (D136).
Leer un documento corre las dos cosas; no hay otra puerta.
_Evitar_: Shaped, documento con forma, artifact parseado.

**Forma publicada**:
El ejemplo completo y anotado campo por campo de una kind, escrito una sola vez
y servido por las cuatro puertas de D129. Es lo que se le da a quien tiene que
escribir el archivo.
_Evitar_: template, schema — el JSON Schema es otra cosa, la salida de
`yunta schema <kind> --json`.

**Regla**:
Lo que solo se puede afirmar con el documento entero a la vista — un id usado
dos veces, una dependencia hacia una tarea que nadie declaró, dos tareas que
alcanzan los mismos archivos. Cada una tiene un código estable, tomado de un
conjunto cerrado, y se cuenta junto con la kind del documento que la violó
(D135).
_Evitar_: constraint, chequeo semántico, validación extra.

## Fallas

**Falla de nodo**:
Por qué un nodo no cerró, como dato y no como frase: o una falla que el engine
enuncia en una oración, o los artifacts declarados que no cerraron. Es lo que
persiste `node_failed`; el texto lo produce cada superficie al leerlo (D133).
_Evitar_: outcome, mensaje de error, motivo.

**Falla de artifact**:
Por qué un artifact declarado no cerró. Hay dos y solo dos: el archivo —
ausente, vacío, por encima de `limits.max_artifact_bytes`, rechazado por el
filesystem — o su contenido, que es un reporte. La distinción vive en el tipo y
no en un predicado, así que ninguna superficie la deduce de la prosa (D134).
_Evitar_: is_repairable, artifact inválido a secas.

**Reporte**:
Todos los problemas de un mismo documento juntos, con la kind que fija su forma
y el path donde se abre. Un nodo que declara varios artifacts interpretados
falla con un reporte por archivo, nunca con una lista sin dueño.
_Evitar_: lista de diagnósticos.

**Diagnóstico**:
La descripción estructurada de una sola cosa que salió mal: su sujeto y su
problema, con un código estable por clase de problema. Se construye una vez,
viaja en el event log con su forma, y se redacta por separado para cada lector.
_Evitar_: mensaje de error, outcome.

**Sujeto**:
De qué parte del documento habla un diagnóstico, nombrada como la nombra el
documento — ``task `t1`, criterion 1`` — o por su posición cuando el id es
justamente lo que no se pudo leer — `the first task`. El sujeto es la
ubicación: un diagnóstico no lleva línea ni columna (D137).
_Evitar_: span, línea y columna, ruta del parser (`tasks[0].criteria[1]`).

**Bloque de problemas**:
El formato único con el que un reporte se muestra a una persona: un
encabezado que nombra qué se leyó y cuántos problemas tiene, y una línea
indentada por problema (spec-ledger §4). Vive en un solo lugar, que no sabe
nada de diagnósticos, y de ahí salen también los errores del CLI.
_Evitar_: formateo por superficie, redacción por lector.

**Rechazo**:
La respuesta del engine a una entrega o un posteo que no acepta: el reporte
entero, en la misma llamada, con la instrucción de corregir y volver a
intentar. No es una falla del nodo — cuesta una llamada, y la sesión sigue.
_Evitar_: error de validación, fallo de artifact.

## Reglas y contrato

**Regla** — una condición que solo se sostiene sobre el documento entero: un `id`
repetido, una dependencia a una tarea que nadie declaró, dos tareas que se pisan.
Se enuncia una sola vez, en la lista `RULES` del kind, al lado de las funciones que
la aplican (D143).

**Exigencia** (`demand`) — lo que una regla pide, en el vocabulario de quien escribe
el documento. Es la lectura *previa* de la regla: viaja en el contrato antes de que
se escriba nada. La lectura *posterior* es el diagnóstico, con el valor concreto.

**Contrato** — lo que una puerta le entrega a quien tiene que escribir un documento:
el ejemplo publicado más las exigencias de todas sus reglas. `shape::contract(kind)`
es el único lugar donde un kind se vuelve texto, así que ninguna puerta puede
entregar un contrato distinto.

**Cobertura** — el invariante de que el ejemplo publicado escribe cada clave que el
tipo acepta, derivado del schema del propio tipo y no de una segunda lista (D144).

_Evitar_: «el esquema» para el contrato — el JSON Schema es otra cosa, y dice menos.

**Verificación en sesión** — el veredicto que una sesión pide con
`yunta_check_artifact` antes de terminar: confirma un archivo que la sesión escribió
—un artifact opaco, o el de un nodo de comando— y lee lo que el engine escribió de
un documento entregado. Corre la misma verificación que el cierre, así que su
respuesta y la del nodo no pueden diferir (D146, D156). Es consultiva: el cierre
sigue siendo el único juez.

