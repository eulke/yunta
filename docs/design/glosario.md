# Glosario

Los términos del dominio con los que se discute el engine, cada uno con la
palabra que se usa y las que se evitan. Un término entra acá cuando el corpus
ya lo apoya en más de un lugar: definirlo tarde es lo que deja que dos partes
del sistema entiendan cosas distintas por la misma palabra.

## La frontera de autoría

**YAML de autor**:
Un documento que escribe una persona antes de que exista un run: los tres
layers de config, los workflows, `pack.yaml`, los casos de test y los fixtures
del mock. `yunta check` lo alcanza antes de gastar un token.
_Evitar_: YAML de usuario, input.

**YAML de agente**:
Un documento cuyo contenido nace en un run: el de casi todo artifact
interpretado. No existe antes del run, así que ningún `check` lo alcanza. Una
sesión lo entrega como objeto y el engine rinde el YAML; un nodo de comando
escribe el archivo. La excepción es el documento que una persona escribe y el
run recibe como input `type: document`: lo lee la misma puerta, y su rechazo
llega al crear el run en lugar de al cerrar un nodo.
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
los dos son válidos. Lo identifica su nombre, que es todo lo que tiene.
_Evitar_: artifact sin tipo, blob.

**Artifact interpretado**:
Un artifact cuyo kind declara que el engine parsea su contenido, lo valida y
lo convierte en eventos. Los kinds son `tasks`, `findings` y `questions`, y lo
identifica el kind: un nodo produce a lo sumo uno de cada uno.
_Evitar_: artifact estructurado, artifact tipado.

**Identidad de artifact**:
Por qué se pregunta cuando se pide un artifact: el kind para un interpretado,
el nombre para un opaco. Es lo que el log afirma al aceptarlo y lo que todo
lector —una fuente de contexto, un mount, una destilación, un gate externo—
usa para resolverlo; dentro de un run, un artifact lo identifica el par
`(nodo, identidad)`. Un nombre de archivo no es identidad: la vista bajo
`artifacts/<nodo>/` se nombra desde la identidad (`<kind>.yaml` para un
interpretado) y nadie la lee para resolver nada (D157).
_Evitar_: nombre del artifact, path del artifact.

**Objeto**:
Los bytes de algo que el run tiene, guardados bajo `objects/<sha256>` y nombrados
por su propio hash. Es el único transporte: todo artifact y todo contenido
efectivo de una fuente de contexto vive ahí, y leer un objeto lo rehashea, así que
un contenido que ya no hashea a su nombre es corrupción que el run reporta. Dos
escrituras del mismo contenido son un solo objeto.
_Evitar_: blob, archivo del artifact, caché.

**Vista**:
El directorio `artifacts/` que el engine proyecta desde el store para que una
persona lea lo que el run tiene: los artifacts de un productor bajo su nodo
(`artifacts/<nodo>/<kind>.yaml` para un interpretado, el nombre declarado para un
opaco), lo adquirido sin productor en la raíz. La escribe una sola función y
ningún lector del engine la abre; borrarla no cambia lo que el run tiene.
_Evitar_: directorio de artifacts como fuente, salida del nodo.

**Staging**:
`scratch/staging/<node_id>/`, el único directorio que un nodo amplía: lo que
`artifact_dir` le entrega a una sesión que declara un artifact opaco, lo que
`{{node.artifacts}}` rinde, y de donde el cierre lee el archivo que ese nodo
escribió. Es de la sesión y no del intento: un intento que continúa una sesión
conserva lo que esa sesión escribió, y todo otro lo abre vacío.
_Evitar_: directorio de trabajo del nodo, salida, artifacts del nodo.

**Kind de artifact**:
El conjunto cerrado de documentos que el engine interpreta, y el tipo que lo
nombra en todas partes: el `kind:` de un workflow, el argumento de
`yunta schema`, el catálogo de la tool `document_shape` y el documento del que
habla un reporte son el mismo conjunto y el mismo tipo (D132).
_Evitar_: DocumentKind, tipo de documento, formato.

**Tasks** (*documento de tareas*):
El artifact interpretado de `kind: tasks`: la lista de tareas verificables que un
nodo de planificación entrega o que un input trae escrita a mano, con `tasks:`
como única clave. Cada tarea pasa al event log (`task_registered`) donde el
documento entra —el cierre del nodo que lo produce, o el nacimiento del run que
lo recibe como input— y el documento queda congelado. Nombra al documento y a su
kind en el workflow, en `yunta schema`, en la tool de entrega
(`yunta_submit_tasks`) y en la fuente de contexto `tasks:`.
_Evitar_: ledger, task-ledger, plan (que es un nombre de archivo, no un kind).

**Ledger**:
Un pliegue del event log: la estructura que se deriva aplicando en orden los
eventos de un tipo y responde qué quedó en pie —`FindingLedger` para los
hallazgos. Nombra siempre algo derivado del log, nunca un documento que alguien
escribe.
_Evitar_: usar la palabra para el documento de tareas.

**Entrega** (*submission*):
Un documento entero que una sesión le pasa al engine por su tool
`yunta_submit_<kind>`, como objeto estructurado y nunca como archivo. El engine
lo valida con el tipo y las reglas del cierre y, si lo acepta, guarda el YAML
canónico como objeto del run y lo afirma en el log (D156, D157).
_Evitar_: subida, escritura del artifact, guardado.

**Posteo**:
Un hallazgo que una sesión reporta solo, en el momento en que lo ve, por
`yunta_post_finding`. La unidad de validación es el hallazgo: un rechazo alcanza
a ese y a ninguno de los ya reportados. `yunta_update_finding` lo reemplaza entero
por id y `yunta_withdraw_finding` lo retira con motivo, definitivamente (D156).
_Evitar_: entrega de findings, envío.

**Documento derivado**:
El artifact `findings` de un nodo `prompt` o `loop`: lo deriva el engine al cierre
como proyección de lo que ese nodo reporta, y lo acepta como cualquier otro. Un
nodo que no reporta nada obtiene una lista vacía, que es el resultado de una
revisión sin hallazgos.
_Evitar_: archivo derivado, artifact de salida, volcado.

**Conjunto efectivo**:
Los hallazgos que un log deja en pie: el último estado de cada par `(nodo, id)`,
sin los retirados, en el orden en que cada uno se posteó por primera vez. Lo
calcula un único pliegue, `events::findings::FindingLedger`, del que leen la
derivación, la herencia entre nodos, la destilación y las estadísticas — con tres
eventos por hallazgo, un segundo pliegue es una segunda respuesta.
_Evitar_: findings vigentes, lista final.

## Lo que el log deriva

**Frame**:
Dónde está cada cosa del run ahora mismo, derivado del log: qué nodos trabajan,
en qué estado quedó cada uno, cuántas tareas cerraron, qué hijos nacieron. Es
una foto, y se vuelve a sacar entera cada vez que algo cambia.
_Evitar_: snapshot, estado de la UI, progreso.

**Standing** (`NodeStanding`):
Dónde está parado cada nodo que el workflow declara, en una sola respuesta para
los tres casos que el DAG congelado y el log producen juntos: afuera, porque el
modo del run lo deja fuera y no corre; incluido y sin empezar; o alcanzado, con
el estado que el log derivó para él. El estado de un nodo responde solo por los
nodos que el log tocó; el standing responde por todos, así que ninguna
superficie tiene que leer una ausencia para saber si un nodo no corre o todavía
no arrancó.
_Evitar_: estado del nodo, pendiente, nodo sin estado.

**Needs you**:
La palabra de un run detenido en una persona —una decisión, una pregunta, un
presupuesto, un scope—, en todas las superficies y en el JSON; su marca, `◆`, es
la única que dice "una persona", y no marca nada más. Sale con código 3.
_Evitar_: paused, parked (como palabra de superficie).

**Waiting**:
La palabra de un run pausado hasta que se reinicia el límite de una cuenta, con esa
hora en su pausa (`awaiting_reset`). Nadie tiene nada que contestar: el run se reanuda
solo a esa hora, así que no es una parada de persona y nunca sale con código 3. Sale
con código 5, en todas las superficies y en el JSON.
_Evitar_: needs you (para un límite que se reinicia), parked.

**Pausa** (`PauseReason`):
Por qué un run se detuvo y espera: un presupuesto agotado, una escalación, nodos
huérfanos cuyo resultado es incierto, o un límite de cuenta que espera la hora de su
reset, sin una persona. Es un hecho tipado que guarda `run_paused`; la
frase que lee una persona la produce cada superficie al leerlo, igual que el texto de
una falla de nodo. Un log escrito antes guarda la frase, y se lee tal cual.
_Evitar_: frase de pausa, motivo de pausa.

**Reported**:
La palabra de un run que terminó sosteniendo findings bloqueantes: el trabajo
está hecho y nadie lo aceptó. Sale con código 4.

**Correcting**:
Un nodo que falló y fue mandado a corregir —por su propio `on_failure`, o porque
una persona eligió reintentarlo— en un run que sigue abierto: su falla se está
trabajando y no se cuenta como una falla en pie. Deja de estarlo cuando vuelve a
correr, o cuando el run cierra sin haber vuelto a él.
_Evitar_: reintentando, en reparación.

**Stalled**:
Un run cuyo log dice que avanza y cuyo engine ya no existe: el registro del run
(`scratch/engine.json`) nombra un proceso que terminó, o un pid que el host le dio
después a otro proceso. Es una lectura, no un evento; sin registro no se afirma,
porque un run que pasa de un proceso a otro tampoco tiene uno por un instante.
_Evitar_: colgado, huérfano, zombie.

**Palabras de una tarea**:
`pending`, `ready`, `running`, `done`, `blocked` y `failed`: dónde está una tarea
del ledger, el mismo token que escribe el log, en todas las superficies, en el
JSON y en lo que lee un agente. Es otro vocabulario que el de un nodo a propósito:
una tarea está `done` cuando sus criterios pasan y su trabajo se integró, y un
nodo está `finished` cuando corrió; un loop `finished` puede dejar tareas
`blocked`.
_Evitar_: finished (para una tarea), completed.

**Handle**:
Cómo una persona llama a un run: los últimos seis caracteres de su id, la parte
al azar de un ULID. Toda línea que lee una persona lo nombra así, y todo comando
que recibe un run lo acepta junto con el id entero, cualquier parte que lo
empiece o lo termine, `last` y `needs`. Un documento para un programa y el plano
de control nombran el id entero.
_Evitar_: alias, id corto, apodo.

**Crónica**:
Qué pasó, en orden, derivada del mismo log y con los mismos tipos que el frame:
un **momento** por evento. Lo que el frame dice que un nodo *es*, un momento
dice que *se volvió*. Es pura y monótona en el log — la crónica de un prefijo
es un prefijo de la crónica — así que un lector que sigue el run en vivo ve lo
mismo, en el mismo orden, que uno que lo lee terminado.
_Evitar_: historial, feed de eventos, timeline.

**Momento**:
Una cosa que pasó, ubicada: cuándo, a qué nodo, y qué. Se lee contra el estado
*de ese instante*, nunca contra el del final — el tiempo que un intento trabajó
y los hijos que arrastró son los que tenía ahí. Un nodo que cierra dos veces son
dos momentos.
_Evitar_: línea, entrada del log, evento — un evento es lo persistido; un
momento es cómo se lee.

**Gasto asentado** (`Spend::settled`):
Lo que un run ya dio por gastado: el terminal de cada nodo que cerró, la ronda de
preguntas que asentó el uso de su sesión y el gasto de los hijos que cerraron. Lo
asentado no vuelve a estar en vuelo. Un run cerrado graba su gasto total: lo que
quedó en vuelo al cerrar pasa a ser gasto grabado.
_Evitar_: gasto confirmado, gasto final.

**En vuelo** (`Spend::in_flight`):
El uso que una sesión abierta ya reportó y que ningún terminal ni ronda asentó. Se
suma a lo asentado en la única cifra de gasto del run (`Spend::so_far`), así que un
nodo que espera a una persona muestra lo que ya gastó.
_Evitar_: gasto pendiente, gasto parcial, uso abierto.

**Tramo de tiempo** (`Timeline`):
Un trozo del reloj del run entre dos eventos. Cada tramo es el tiempo de una sola
cosa, la que estaba abierta a lo largo de él: un nodo trabajando, una persona
preguntada, un chequeo, un servicio caído, un run detenido. Si nada estaba abierto,
es del motor entre un paso y el siguiente. Un tramo que el host pasó dormido no
cae en ningún balde.
_Evitar_: intervalo, lapso, periodo.

## Gates

**Escalación** (`Escalation`):
Lo que un gate publica cuando detiene el run: la afirmación de quien escala, la
evidencia contra la que se audita —hechos que el engine adjunta derecho del
log— y un menú de opciones que no puede estar vacío. Es la única forma en que
nace un `gate_waiting`: el corte entre la afirmación y la evidencia se decide
en un solo lugar, una afirmación que repite un hecho del registro se rechaza al
construirla, y un menú vacío —una escalación que rechaza toda respuesta que
recibe— es imposible por tipo.
_Evitar_: pregunta del gate, pedido de ayuda, pausa.

**Tradeoff** (`GateOption.tradeoff`):
Qué cuesta tomar una opción del menú, escrito por quien escribió la opción y
obligatorio en las opciones que el engine escribe con sus palabras: toda opción que
amplía el trabajo declara contra qué lo cambia, y elegirla autoriza esa ampliación
(D50). Las opciones de un menú de falla o de desvío no traen palabras: la superficie
que dibuja el menú las escribe desde sus datos. Viaja con la opción y se
muestra como la línea debajo de su etiqueta, así que quien decide lee el costo
donde elige.
_Evitar_: nota de la opción, advertencia, consecuencia.

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

## El cerco

**Confinación** (`Confinement`):
Lo que una sesión puede escribir y alcanzar, resuelto una vez al abrirla: su
perfil (`read-only`, `edit` o `full`), el sandbox de `edit` (`workspace-write`,
`read-only` o `danger-full-access`) y su red. Sus comandos escriben el worktree
según el perfil y el sandbox de `edit`, las raíces del run y el directorio
temporal (`$TMPDIR` y `/tmp`), que toda sesión puede escribir. El directorio de
artefactos lo escriben todas las sesiones, y los directorios compartidos, las que
no son `read-only`. La red la fija el `network:` del nodo o, si no lo dice, el del
proyecto, y por defecto queda abierta; solo la tiene una sesión que escribe su
worktree. Una sesión que no lo escribe —un nodo `read-only`, o `edit` con sandbox
`read-only`— no la tiene nunca, y una sesión sin confinar exige red abierta. Cada
adapter la aplica exacta o la rechaza, y el log la guarda en
`agent_session_opened.confinement`, con sus campos `profile`, `commands`,
`worktree` y `network`: lo que la sesión pudo hacer, no lo que hizo.
_Evitar_: sandbox (es la forma que toma en un CLI), permisos de sesión.

**Cerco** (`Fence`):
Lo que una sesión puede escribir bajo el worktree con sus herramientas: los
globs —el scope declarado más las ampliaciones ya concedidas— que el riel juzga
escritura por escritura. Las raíces del run y los árboles protegidos enteros
(`dir/**`) los juzga la confinación. Un archivo protegido, un patrón que cubre
parte de un directorio y uno que no nombra directorio los juzgan el riel y el
cierre: denegar su directorio negaría paths que nadie protege. Viaja siempre (D172).
_Evitar_: edit_constraints, restricciones de edición, permisos de escritura.

**Nivel** (`FenceLevel`):
Lo que un adapter sabe construir del cerco: nada, un juicio por llamada de
herramienta, o un sandbox de filesystem. Es del adapter, constante, y viaja en
`capabilities.fence`. Un transporte distinto —hook, extensión, programa
delegado— no es un nivel distinto.
_Evitar_: edit_hooks, modo de enforcement.

**Cobertura** (`Coverage`):
Cuánto de una sesión concreta cercó lo que su adapter construyó: exacta,
ensanchada a raíces (con el directorio temporal de los comandos), o solo
herramientas. Se calcula de lo construido, nunca se declara, y viaja en
`agent_session_opened.fence`.
_Evitar_: nivel de la sesión, enforcement efectivo.

**Canal**:
Por dónde una sesión escribe: las herramientas de archivo del CLI, o todo lo
demás —shell, MCP, tareas delegadas—. La cobertura de una sesión es la del
canal más débil.

**Cercado** (`Fenced`):
Lo que un canal tiene: cerco exacto, o raíces.

**Juez** (`Fence::judge`):
La función pura que decide si un path está dentro del cerco. No toca disco:
resuelve `.` y `..` lexicalmente contra el worktree y responde. Es una sola
para todo el workspace: el engine la pregunta por cada escritura que el riel
de una sesión le consulta, y el mock por los efectos de un fixture sin riel
(D249).
_Evitar_: validador de scope, checker.

**Riel** (`RailCommand`, `/rail`):
Lo que el CLI de una sesión le pregunta al engine mientras trabaja — antes de
escribir y antes de terminar. El hook es este binario, su subcomando `rail`, el
adapter cuyo codec lee la llamada y el momento (`before-write`,
`after-write`, `before-stop`); la respuesta la da el engine por el listener de
esa sesión, con el mismo token que sus herramientas, y deja en el log lo que
juzgó o hizo (D249, D254, D255). `before-stop` juzga el trabajo con el mismo
juicio que el cierre del intento, en seco: si el cierre no lo tomaría, la
sesión vuelve a él.
_Evitar_: fence hook, FenceHook, yunta fence.

**Codec** (`RailCodec`):
La traducción entre el riel y el hook de un CLI: qué pregunta la llamada que
llegó por stdin, y en qué forma ese CLI lee la respuesta. Un adapter escribe
solo el codec; lo que se responde es del engine.

**Rechazo** (`write_refused`):
El hecho de que una escritura fue rechazada antes de ocurrir. Su texto para el
modelo nace una sola vez, empieza por un marcador fijo y dice la salida: pedir
ampliación, o reportar un finding.
_Evitar_: bloqueo, edición bloqueada.

**Parada devuelta** (`stop_refused`):
La sesión quiso terminar con trabajo que su cierre no tomaría, y el engine la
devolvió a él diciendo qué falta. Lo juzga el mismo juicio que el cierre, una
vez por estado del trabajo (D254). No la devuelve un pedido de ampliación que
el cierre puede conceder o escalar: lo decide el cierre.
_Evitar_: stop bloqueado, reintento.

**Negativa inmediata**:
La negativa de un pedido de ampliación de una tarea que el engine contesta en la
llamada, con su razón, porque el proyecto lo decide con el pedido solo y ningún
trabajo lo cambia: el modo `deny` o una ruta que ninguna persona puede conceder.
Su respuesta empieza con `request denied`, y el log registra el pedido y la
negativa en ese momento (`scope_expansion_requested` y `scope_expansion_denied`).
Las negativas que dependen del trabajo —un criterio que ya pasa, el tope de
archivos, un pedido `rules` sin criterio, que una salida en rojo todavía puede
conceder— se deciden al cerrar.
_Evitar_: rechazo diferido, denegación al cierre.

**Sujeto de una concesión**:
Lo que una decisión de alcance amplía o deniega: una tarea, o el nodo al que pertenece
la sesión. Una concesión sin tarea ni nodo no se escribe. Una denegación de nodo no
nombra tarea, porque su sujeto es el nodo del evento; y un hecho de sesión sin nodo
corta el replay, porque no tiene a quién atribuirse.
_Evitar_: destino de la concesión, alcance del run.

**Retiro** (`scope_expansion_withdrawn`):
Lo que una tarea deja al cerrar: el pedido de ampliación que nadie contestó
cuando el cierre llegó. No es una concesión ni una denegación: no escribe
finding, no cuenta para `max_per_run` y nadie lo decide. Una tarea que no cierra
no retira su pedido: lo escala una persona.
_Evitar_: cancelación, descarte.

## Fallas

**Falla de nodo**:
Por qué un nodo no cerró, como dato tipado y no como frase: la causa de un
comando, de una sesión rechazada, de un límite, de las tareas que paró un loop, de
los artifacts declarados que no cerraron, de una sesión que el engine no pudo parar
o de un archivo congelado que ya no coincide con su hash. Es lo que persiste
`node_failed`. Las palabras las escribe cada superficie al leerla, y los remedios
que ofrece se deducen de la falla y del log. El engine solo enuncia una oración en
`Failure::Message`, para una falla sin datos propios; esa oración se guarda y se
lee tal cual (D133).
_Evitar_: outcome, mensaje de error, motivo.

**Falla de artifact**:
Por qué un artifact declarado no cerró. Hay cuatro y solo cuatro: el archivo —
ausente, vacío, por encima de `limits.max_artifact_bytes`, rechazado por el
filesystem —, un documento que el nodo quedó debiendo, su contenido, que es un
reporte, y un artifact que ningún run tiene. La distinción vive en el tipo y
no en un predicado, así que ninguna superficie la deduce de la prosa (D134,
D157).
_Evitar_: is_repairable, artifact inválido a secas.

**Reporte**:
Todos los problemas de un mismo documento juntos, con la kind que fija su forma
y el path donde se abre. Un nodo que declara varios artifacts interpretados
falla con un reporte por documento, nunca con una lista sin dueño.
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
indentada por problema (spec-tasks §4). Vive en un solo lugar, que no sabe
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
se escriba nada. La lectura *posterior* es el diagnóstico, con el valor concreto. No
confundir con la **línea de exigencia**, que es la fila donde una superficie dice si
el run necesita a alguien.

**Contrato** — lo que una puerta le entrega a quien tiene que escribir un documento:
el ejemplo publicado más las exigencias de todas sus reglas. `shape::contract(kind)`
es el único lugar donde un kind se vuelve texto, así que ninguna puerta puede
entregar un contrato distinto.

**Cobertura** — el invariante de que el ejemplo publicado escribe cada clave que el
tipo acepta, derivado del schema del propio tipo y no de una segunda lista (D144).

_Evitar_: «el esquema» para el contrato — el JSON Schema es otra cosa, y dice menos.

**Verificación en sesión** — el veredicto que una sesión pide con
`yunta_check_artifact` antes de terminar: confirma un archivo que la sesión escribió
—un artifact opaco, o el de un nodo de comando— y, para un documento, lee el que el
run ya tiene. Llama a las mismas dos funciones que el cierre, así que su respuesta y
la del nodo no pueden diferir —incluido «nadie lo entregó»— (D146, D156, D157). Es
consultiva: el cierre sigue siendo el único juez.

## La frontera y la superficie

**Puerto** (`yunta_core::port`):
Lo que el engine llama y lo que le responde: el puerto de sesión, el CLI de
agente que un adapter maneja, y el puerto de forja, el pull request sobre el
que se decide un gate externo. Los dos viven en core, así que el engine depende
de la interfaz y nunca de una implementación de ella, y el compilador sostiene
esa frontera (D165).
_Evitar_: interfaz del adapter, capa de integración, puerto de red.

**Política de degradación** (`Policy`, `PolicyApplied`):
Lo que una sesión dejó de tener cuando faltó una capacidad de su adapter: uno de siete
nombres de un conjunto cerrado (`post_check_only`, `no_token_budget`, `no_skills`,
`no_run_tools`, `network_open`, `fresh_session`, `cap_at_session_end`), que
`capability_degraded` guarda como dato. La frase que lo explica la produce el crate
`render`. Un log escrito antes guarda un texto: si no es el nombre de una política, se
lee tal cual.
_Evitar_: policy_applied, frase de degradación.

**Descarte de runner** (`RunnerRejection`):
Por qué un candidato del runner no se usó: el run fijó otro adapter con `--adapter`, el
adapter del candidato no está disponible aquí, o es la frase que guardó un log escrito
antes. `runner_resolved` lo guarda en cada candidato descartado, como dato tipado.
_Evitar_: rechazo del runner, candidato fallido.

**Línea de exigencia** (`view::demand_line`):
La línea que contesta "¿este run me necesita?": o no necesita a nadie, o algo
lo necesita y ahí está el comando que lo mueve, con aquello sobre lo que está
parado. Se produce en toda fase, incluso las que no necesitan a nadie, porque
una línea que aparece solo cuando hay algo que decir vuelve ambigua su
ausencia; el comando va adelante, así que una fila cortada al ancho de la
terminal pierde el sujeto y nunca lo que hay que correr. Las mismas frases
llegan al bloque que cierra el run, a `yunta status`, al JSON y al diagnóstico
del comando que no pudo terminar su trabajo. No confundir con **Exigencia**,
que es lo que una regla le pide a quien escribe un documento.
_Evitar_: barra de estado, sugerencia, mensaje de la vista.
