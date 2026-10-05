---
number: D156
title: "Un artifact interpretado entra por una herramienta tipada; el engine escribe el archivo"
status: revised
revises: [D86, D129, D131, D138, D140, D144, D146, D149, D151]
revised_by: [D157]
---

# D156 — Un artifact interpretado entra por una herramienta tipada; el engine escribe el archivo

*(Revisada por D157: el engine guarda el objeto y proyecta la vista en lugar
de escribir el YAML en `run_dir/artifacts/<name>`, y la tool pierde su
argumento `name` porque el kind que el nodo declara es la identidad.)* Un nodo
de sesión que declara `artifacts.produces: [{name, kind}]` recibe, además de
la forma (D129), una herramienta `yunta_submit_<kind>` por cada kind
entregable que declara, con `name` restringido por `enum` a los nombres
renderizados y `document` igual al schema publicado de esa kind
(`core/schemas/`, servido por `schema::json`). La entrega deserializa al mismo
tipo y corre el mismo `check()` que el cierre; un rechazo vuelve en la misma
llamada con todos los problemas de regla cuando el documento parseó, o con el
problema estructural y su ruta cuando no —un valor del tipo equivocado corta
la lectura, y ninguna regla vale sobre un documento que no parseó—, y una
aceptación escribe el YAML canónico en `run_dir/artifacts/<name>` —temporal en
`scratch/`, `rename` atómico— y responde lo que el engine leyó. Findings no se
entrega: ninguna de sus reglas cruza entradas, así que la unidad de validación
es el hallazgo, que entra por `yunta_post_finding` —ahora estricto, contra
`FindingEntry` y no contra el tipo persistido y tolerante— y se corrige con
`yunta_update_finding` o se retira con motivo con `yunta_withdraw_finding`,
solo por el nodo que lo posteó y con el retiro como estado final; el artifact
de un nodo `prompt` o `loop` es la proyección de lo que ese nodo reportó,
escrita al cierre, con lo que un hallazgo sobrevive a la sesión que lo
encontró. El conjunto efectivo —último estado por `(nodo, id)`, sin retirados,
en orden de primer posteo— lo calcula un único pliegue,
`events::findings::FindingLedger`, del que leen la derivación del archivo,
`derive()`, `inherited_findings`, `distill` y `stats`. Los cuatro eventos
nuevos —`artifact_submitted` con `Accepted { content_hash }` o `Refused {
report }`, `finding_updated`, `finding_withdrawn`, `finding_refused` con su
`operation`— dejan el rechazo como dato del run y no como algo que solo vio la
sesión.

Racional: cada capa que rodeaba al archivo —forma en el prompt,
`yunta_check_artifact`, recorrido que nombraba todos los problemas, sesión de
reparación, `--add-dir`— corregía a posteriori una única causa, que el canal
de salida del agente admitía lo inválido; con el schema en la definición de la
herramienta, el modelo llena un objeto en lugar de escribir un formato, la
clase entera de malformaciones desaparece y un error cuesta una llamada en vez
de una sesión. Consecuencias: se eliminan el ciclo de reparación (D131, D138)
y `limits.max_artifact_repairs` (D151); el recorrido de diagnóstico desaparece
y `Problem` queda con `Parse { path, message }` y `Rule { code, detail }`;
`artifact_dir` (D149) viaja solo cuando el nodo declara un artifact opaco, de
modo que un nodo que solo declara interpretados no recibe escritura fuera de
su worktree; `yunta_check_artifact` (D146) queda para confirmar un archivo
escrito o leer el que el engine escribió; y un nodo de sesión cuyo adapter no
monta run tools falla antes de despachar, como un blackboard sin capacidad. Un
nodo de comando sigue pudiendo declarar un artifact interpretado y escribir su
archivo —`ledger-task` monta así un ledger escrito a mano— porque ahí no hay
sesión a la que instruir y el cierre lo lee con el mismo código. No hay
compatibilidad hacia atrás para manifests con `max_artifact_repairs`: el
proyecto no tiene usuarios.

Descartados: una sola herramienta con `anyOf` (unión discriminada donde dos
nombres estáticos alcanzan); un `yunta_submit_findings` (duplica
`yunta_post_finding` con peor granularidad y conserva la doble vía); aceptar
tool o archivo para un nodo de sesión (conserva íntegro el mecanismo que se
retira); conservar el recorrido de diagnóstico (su justificación era la
economía de la reparación, y el error con ruta alcanza para el residual); una
regla estática que prohíba el artifact interpretado en un nodo de comando (su
premisa —que no puede alcanzar `run.dir`— es falsa, y `ledger-task` es el
contraejemplo).
