---
number: D157
title: "Un artifact es un hecho del log, con sus bytes en un store por hash; el directorio deja de ser el transporte"
status: revised
revises: [D11, D19, D82, D86, D106, D107, D108, D134, D146, D149, D152, D156]
revised_by: [D159]
---

# D157 — Un artifact es un hecho del log, con sus bytes en un store por hash; el directorio deja de ser el transporte

Un artifact entra al run por una sola puerta, `accept`: los bytes van a
`objects/<sha256>`, un `artifact_accepted` afirma la identidad, el
`content_hash` y el origen —`submitted`, `ingested`, `derived`, `answered`,
`input`, `inherited`, `legacy`—, y de ahí sale la vista. Todo lector del
engine —la fuente de contexto `artifact`, el `loop` que busca su documento de
tareas, un mount, la herencia de una promoción, `on_finish.distill`, los
adjuntos de un gate externo y `yunta_check_artifact`— pregunta por la
aceptación que el log deja en pie (`ArtifactLedger`) y toma los bytes del
store; ninguno abre un artifact por nombre de archivo. Lo que identifica a un
artifact es su kind cuando el engine lo interpreta y su nombre declarado
cuando no: `artifacts.produces` es una lista de strings sueltos donde un
string que nombra un kind **es** ese kind (`ArtifactSpec::Interpreted`) y todo
otro es un nombre de archivo (`ArtifactSpec::Opaque`), un nodo produce a lo
sumo un documento de cada kind, y `(nodo, identidad)` es por lo que un run
contesta. `artifacts/` queda enteramente del engine: la única escritura que la
toca es la proyección de una aceptación, `artifacts/<nodo>/<kind>.yaml` sale
de `ArtifactId::view_name()` y la raíz guarda lo que el run adquirió sin
productor. Lo único que un nodo amplía es su staging,
`scratch/staging/<node_id>/`, que es lo que `artifact_dir` le entrega a una
sesión que declara un artifact opaco —la regla de D156 intacta— y lo que
`{{node.artifacts}}` rinde. Consecuencias: una referencia —`context: [{
artifact }]`, una entrada de `mounts:`, una de `on_finish.distill`— nombra
exactamente uno de `kind:` o `name:` (`ArtifactRefId`), `distill` nombra
además el nodo porque un kind identifica dentro de un productor, y el `as:` de
un mount renombra un opaco y se rechaza al leer el workflow al lado de un
`kind:`; una referencia con `node` alcanza solo al productor que nombra y una
sin `node` gana la última aceptación por `seq`; los tres nombres de kind
quedan reservados como nombres de archivo, con un error propio de `check`
(`ReservedArtifactName`), y declarar el mismo kind dos veces en un nodo es
error de check porque no hay segunda identidad que declarar; la tool pierde su
argumento, `yunta_submit_tasks { document }`, y `render_artifact_names` queda
acotada a los nombres opacos, que son los únicos que pueden llevar template.
El cierre de un documento pregunta al log —de dónde viene cada respuesta lo
dice un único predicado, `answered_by_the_log`, del que leen también
`record_artifacts` y `yunta_check_artifact`— y el engine deja de escribir un
YAML para releerlo él mismo; un artifact opaco no cambia: el archivo del
staging es lo que la sesión o el comando escribió y nadie más tiene, así que
el cierre lo lee, lo ingresa y lo acepta con origen `ingested`. El staging es
de la sesión y no del intento: se conserva cuando el intento continúa una
sesión y se vacía cuando no, y como la respuesta exige la política del nodo,
el log y el runner que ese nodo resuelve —un adapter sin `resume_session`
degrada a sesión nueva—, un nodo de sesión abre su staging al despachar y todo
otro kind en `node_started`, que sigue siendo el punto por el que pasan todos
sus intentos; qué kind puede continuar una sesión lo dice un único lugar,
`NodeKind::opens_resumable_session`. Las respuestas de un artifact `questions`
son una aceptación más, proyectada bajo su nodo, y la ronda de preguntas las
relee por el log. Un nodo `kind: workflow` adquiere del ledger de su hijo lo
que declara producir cuando ese hijo termina `done`, con `origin: inherited {
run, producer }`: solo lo declarado viaja, o entran todos o no entra ninguno,
un hijo promovido no entrega —la composición sigue en su sucesor— y uno
fallido tampoco, y un `findings` así adquirido deja sus hallazgos posteados en
el log del padre bajo ese nodo. Una promoción hereda lo que el log del
antecesor tiene en pie, con lo que un archivo suelto que ninguna aceptación
explica deja de heredarse, y `distill` copia los bytes que el log nombra y
registra ese mismo hash en `provenance.yaml` en lugar de rehashear un archivo.
Un input `type: document` declara el `kind` con el que su archivo se lee al
crear el run —antes del worktree y del baseline, el mismo racional con que
`path` chequea existencia—: uno inválido rechaza la creación del run con el
reporte entero, en un error tipado que lleva el `Report`; uno válido se rinde
canónico y nace como artifact del run con origen `input`, sin nodo productor,
y `{{inputs.<nombre>}}` rinde `sha256:<hash>` en vez de una ruta que el run va
a sobrevivir; siendo `tasks`, su nacimiento emite además el `task_registered`
de cada tarea, porque ningún nodo va a producir ese documento.
`resolve_inputs` devuelve `ResolvedInputs { values, documents }` y
`build_manifest` un `FrozenRun { manifest, documents }`, y `yunta check`
rechaza que un input y un nodo produzcan el mismo kind, nombrando a los dos.
`ArtifactFailure` tiene cuatro formas: `File { path, problem }` para el
archivo que el cierre abrió, `Content(Report)` para el documento que no cumple
su forma, `Unheld { run, producer, artifact }` (código estable
`artifact-unheld`) para lo que ningún run tiene, y `Undelivered { node,
artifact }` (`artifact-undelivered`) para el documento que un nodo quedó
debiendo; las dos últimas no llevan path porque ahí el cierre no abrió ningún
archivo, y con ellas quedan colapsadas `MountError::Unheld` y
`NotAcquired::Unheld`, de modo que una falla de mount y una de adquisición
llegan a `status`, al recibo y a `events.json` como entradas de artifact con
su código. Reanudar verifica: antes del `run_resumed`, el resume lee del store
el objeto de cada aceptación que el log deja en pie y lo rehashea, y uno
ausente o cuyo contenido no hashea a su propio nombre marca el run `broken`
por el camino de `steps::broken`, con su export forense y sin registrar que
reanudó; un log anterior al store nombra sus artifacts con `artifact_written`
—un hash sin objeto detrás, que nunca lo tuvo— y esos se cuentan como no
verificables, con un finding `minor` que dice cuántos son y por qué, y el run
reanuda. `yunta verify` corre las dos verificaciones y las reporta aparte,
porque un objeto corrupto no rompe la cadena de eventos ni al revés, y ambas
viven en un solo lugar, `artifacts::integrity`. Se retiran con esto la
auditoría de D152 y su `ArtifactsSnapshot`.

Racional: mientras el lector abría el archivo, el store y `artifact_accepted`
eran contabilidad paralela —dos respuestas a la misma pregunta, y la que
decidía era la que cualquiera podía reemplazar—, y un run derivado por replay
no puede tener lectores que dependan de que un directorio no haya cambiado. De
esa causa sale el resto. La auditoría de D152 existía porque `artifacts/` era
un directorio compartido, escribible y leído: con la vista por nodo y el
staging aislado, un nodo que escribe sobre el nombre de otro no alcanza nada,
y el aislamiento vuelve estructural lo que ella intentaba a posteriori. El
engine escribía un archivo para volver a leerlo él mismo —la última vuelta
donde el disco era autoritativo sobre algo que el log ya afirmaba—, y mientras
esa copia existiera el documento que decidía el nodo podía diferir del que el
run tenía aceptado. El nodo `kind: workflow` verificaba su cierre contra el
`artifacts/` del padre, que en una composición nadie escribe, así que uno con
`artifacts.produces` no podía terminar nunca. Vaciar el staging en todo
intento trataba a la reanudación como un intento nuevo: la sesión seguía donde
estaba y el archivo que ya había escrito desaparecía, de modo que un nodo que
no lo reescribía fallaba debiendo algo que sí había entregado, y «reanudar la
sesión» deja de significar lo que dice si el trabajo de esa sesión no
sobrevive. La prueba de que el nombre era un parche está en los packs, que
declaraban `findings-{{runner.role}}.yaml` únicamente porque `artifacts/` era
plano y los hermanos de un fan-out se pisaban: con la vista por nodo eso es
`produces: [findings]`, y desaparece con él la pregunta «¿qué nombre le
pongo?» que ningún autor tenía forma de contestar bien. El kind de un input es
una declaración que el engine puede honrar, y mientras no la honrara cada
workflow que recibiera un documento pagaba un nodo, una dependencia y una
copia para llegar al mismo lugar —`run-tasks.yaml` tenía un `bash` que copiaba
el input al staging para que el engine lo validara— y el rechazo de un
documento mal escrito llegaba después del worktree, del baseline y de la
primera sesión. Y el contrato (§8.1) prometía que el resume verifica la
integridad de los artifacts por hash sin que nadie la verificara, con lo cual
un run podía reanudarse y entregarle a un nodo bytes que su propia historia
nunca vio. La tolerancia con el formato viejo es la misma lectura que §3.1 ya
define para un evento desconocido: derivar lo que se puede, decir lo que no,
nunca `broken` por lo que el formato no podía dar. Los payloads se reemplazan
en el lugar, sin `node_failed_v2`, que es lo que D141 autoriza mientras no
haya tag publicado.

Descartados: conservar la auditoría de D152 como defensa en profundidad (falla
nodos por escrituras que ya no tienen efecto, y un mecanismo cuyo daño no
existe es ruido que se aprende a ignorar); resolver un nombre recorriendo la
vista en disco para recuperar identidades (devuelve el directorio al lugar del
que se lo saca); poner el nombre al lado de la identidad en
`artifact_accepted` (dos declaraciones de lo mismo, y el nombre de un
interpretado está deliberadamente fuera del evento); conservar `{name, kind}`
y derivar la identidad del kind (deja un campo que no identifica nada y que
dos nodos pueden escribir distinto para el mismo documento); permitir varios
documentos de un kind por nodo distinguidos por nombre (rompe la derivación de
findings y devuelve el nombre a la identidad); hacer de los nombres reservados
un error de parseo en vez de uno de check (el mensaje que sirve nombra el nodo
y el sitio, que es lo que `check` tiene y el parser no); conservar
`artifacts/` como raíz escribible distinguiendo vista de escritura por
convención de nombres (es la auditoría de D152 con otro nombre); un staging
único para todo el run (no aísla nada, que es el problema); vaciar el staging
al despachar la sesión en vez de al abrir el intento (deja fuera a los nodos
de comando, que son justamente los que escriben su archivo a mano); decidir de
quién es el staging en `node_started` con la política y el log solamente (es
el predicado aproximado: adivina que la reanudación va a ocurrir, y deja el
archivo del intento anterior en pie cuando el adapter la degrada); vaciar en
`node_started` y volver a vaciar en la degradación (dos escrituras para una
sola decisión, y el estado del directorio depende de en qué mitad del intento
se lo mire); mover `resolve_node_runner` antes de los hooks `before` para
tener el adapter temprano (cambia qué falla primero en un nodo mal declarado,
por una razón que no es suya); dejar el archivo como caché de lectura del
cierre (dos respuestas a la misma pregunta, que es el defecto que esta
decisión existe para eliminar); que el cierre caiga al archivo cuando el log
no tiene nada (le devuelve autoridad al disco justo en el caso en que el run
afirma que no hay documento); conservar `File { Missing }` para un documento
que nadie entregó (nombra una ruta que el cierre ya no abre, y quien la abre
no aprende nada); toda causa nueva de `FileProblem` —para nombrar el run hijo,
para `Unheld`, para `Undelivered`— (D134 lo deja exhaustivamente sobre un
archivo en disco, y ninguno de los tres tiene uno); reusar `Unheld` para lo
que debe el nodo propio (el remedio es otro: un artifact `unheld` lo produce
otro run, este lo debe este nodo); dejar una falla de mount o de adquisición
como sentencia y que cada superficie la parsee (es el defecto que D130 y D133
existen para eliminar); una variante por sitio, mount y adquisición (la misma
idea escrita dos veces, y ningún lector tiene motivo para distinguirlas);
copiar el `artifacts/` del hijo al del padre (devuelve el directorio al lugar
del que se lo saca, y hereda archivos que ninguna aceptación explica); heredar
todo lo que el hijo tiene, como hace una promoción (una promoción es el mismo
trabajo que sigue, y una composición es una frontera: lo que cruza es lo que
el padre pidió); congelar la ruta de un input `document` y leer el documento
cuando alguien lo pida (el archivo del repo cambia y el run se mueve, así que
la ruta deja de nombrar lo que el run leyó, y el hecho del log dejaría de ser
reproducible); tratar ese input como un `path` y que el `loop` lea el archivo
(devuelve el disco al lugar del que se lo saca); registrar las tareas de todo
artifact de nacimiento (un run que hereda de otro pertenece a una cadena cuyos
nodos registran donde producen, y ampliar eso es una decisión de la herencia,
no de los inputs) *(Revisada por D159: el nacimiento registra también lo
heredado.)*; verificar un artifact legacy contra el archivo que su
`artifact_written` nombra (le daría a `artifacts/` dos significados
—autoritativo para un run viejo, derivado para uno nuevo— con la edad del run
decidiendo cuál, y el pliegue no conserva esa ruta: guarda identidad, no
path); marcar `broken` a todo run legacy (lo vuelve irreanudable por una
verificación que su formato no puede satisfacer, que es exactamente lo que la
tolerancia existe para evitar); verificar solo los artifacts que el próximo
paso va a leer (un run tiene decenas de objetos, no millones: el ahorro es
invisible y la garantía pasaría a depender de qué nodo toca, que es lo
contrario de una verificación); un evento propio para el resultado de la
verificación (el `broken` ya lleva el diagnóstico entero, y lo que se degradó
es un finding, que es la forma con la que el engine ya reporta lo que no pudo
hacer).
