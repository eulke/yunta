---
number: D176
title: "El baseline es del linaje: la raíz lo mide en su primer despertar y todo run que nace de ella nace teniéndolo"
status: accepted
revises: [D18, D61, D167]
revised_by: []
---

# D176 — El baseline es del linaje: la raíz lo mide en su primer despertar y todo run que nace de ella nace teniéndolo

## Contexto

D18 fija «snapshot al abrir el run» y D167 lo construye en `create_run`:
cada run mide la suite al nacer. Un hijo `kind: workflow` nace dentro de un
`execute_run` vivo y mide de nuevo sobre un árbol que el padre ya tocó,
así que una regresión del padre le queda invisible al `baseline_compare`
del hijo; un sucesor de promoción mide de nuevo por el mismo camino; una
composición paga la suite una vez por sub-run; `yunta run --detach` y la
tool `run_workflow` pagan la suite antes de devolver el id; y un
nacimiento interrumpido deja un run sin `baseline_captured` que ningún
despertar repone. D61 promete además que `baseline_compare` y
`coverage_gate` entran en la memoización de §5.4, y ninguno de los dos
entra (plan de raíz, §11 L-91, L-92, L-93).

Lo que un usuario quiere saber es una sola cosa: qué pasaba antes de que
disparara la invocación. Un baseline medido por run no responde eso.

## Decisión

1. **El baseline es del linaje.** La raíz lo mide una vez, en su primer
   despertar, antes de su primer nodo, bajo la supervisión del run
   —registro, token y entorno—: medirlo es una decisión del scheduler
   (`Decision::MeasureBaseline`) y un paso de la cáscara, y un run que ya
   tiene la medición en su log nunca la ve. Una suite que la cancelación
   detiene no escribe nada: el paso siguiente del loop registra la pausa
   y el siguiente despertar mide. El hecho es del run —`RunEvent::BaselineCaptured`,
   plegado por `RunLedger`— y toda lectura sale del estado.
2. **Todo run que nace de otro nace teniéndola.** Un hijo `kind: workflow`
   y un sucesor de promoción reciben la medición de la raíz al nacer,
   como reciben sus artifacts: `baseline_captured` en su propio log, con
   `origin: inherited { run: <raíz> }` —la raíz, nunca el padre
   inmediato—; los bytes de la suite quedan con el run que midió. Su
   propia config no se consulta. Un log escrito antes del campo `origin`
   se lee como `measured`.
3. **Toda comparación del linaje compara contra esa única medición.**
   `baseline_compare` re-ejecuta la suite sobre el árbol actual y compara
   contra la medición de la raíz; dentro de una invocación reutiliza, por
   el `Memo` de §5.4, el resultado de la suite sobre un árbol que no
   cambió desde otra comparación, y lo dice en su cierre. `coverage_gate`
   mide cada vez: su veredicto lee la salida del comando, que el memo no
   conserva.
4. **Se mide si y sólo si la config declara `baseline.suite`.** `yunta
   check` avisa cuando ni el workflow ni los workflows que compone
   comparan: quien declaró la suite decide si agrega la comparación o
   retira la suite.

## Racional

Replay: cada run del linaje se contesta con su propio log —`status`,
`receipt` y `baseline_compare` no caminan hacia arriba—. Núcleo puro:
decidir que falta medir es una función del estado; medir es la cáscara. Dueño: la suite
corre bajo el `execute_run` que ya gobierna todo subproceso del run, y
`yunta cancel` la encuentra por `engine.json`. Un lugar: un módulo mide,
hereda y lee el baseline. Degradación explícita: el recibo y la crónica
dicen `inherited from run <raíz>`; una suite que nadie compara se avisa
en `check`, donde se lee.

## Alternativas descartadas

Heredar sólo si el fingerprint del árbol coincide con el del padre y medir
si no: mide «cómo estaba cuando me tocó nacer», que no es un baseline, y
esconde al hijo la regresión del padre. Medir sólo cuando la composición
compara: acopla el nacimiento al catálogo del árbol en ese instante y un
`use:` que resuelve distinto a mitad del run cambia si la raíz midió, sin
evento que lo diga. Dejar la medición en `create_run` y darle un token:
sigue midiendo por run y deja `--detach` esperando la suite.
