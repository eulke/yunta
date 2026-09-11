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

**Kind de artifact**:
El conjunto cerrado de documentos que el engine interpreta, y el tipo que lo
nombra en todas partes: el `kind:` de un workflow, el argumento de
`yunta schema`, el catálogo de la tool `document_shape` y el documento del que
habla un reporte son el mismo conjunto y el mismo tipo (D132).
_Evitar_: DocumentKind, tipo de documento, formato.

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

**Recorrido**:
La pasada sobre un documento que no deserializó, que junta todos sus problemas
en orden en vez de detenerse en el primero, cargando la entrada que está
mirando para que ninguna llamada tenga que repetirla.
_Evitar_: visitor, segundo parser.

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
filesystem — o su contenido, que es un reporte. La distinción es la que decide
si una reescritura puede arreglarlo (D134).
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

**Ciclo de reparación**:
El reintento de un nodo cuyo artifact interpretado no se pudo leer: la sesión
se reabre con los problemas de lo que falló, contra un tope propio. Es la
contraparte del ciclo de tarea — aquél reintenta trabajo, éste reintenta una
declaración.
_Evitar_: retry, segunda pasada.
