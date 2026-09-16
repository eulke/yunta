---
number: D138
title: "El ciclo de reparación es de la frontera; lo tiene todo nodo que resuelve un runner"
status: retired
revises: []
revised_by: [D156]
---

# D138 — El ciclo de reparación es de la frontera; lo tiene todo nodo que resuelve un runner

*(Retirada por D156.)* La corrección de un artifact interpretado ilegible es
una sesión propia, no otra corrida del nodo: se despacha sobre el runner del
nodo, su prompt son los bloques de forma que ese nodo ya monta más los
problemas de la lectura, y su único trabajo es reescribir los archivos
declarados. `runner:` y `artifacts:` son claves de nodo que cualquier kind
puede llevar, así que un solo camino sirve a todos. El archivo mal escrito
sigue en disco, con lo cual la sesión lee su propia salida rota y la corrige —
que es exactamente lo que hace que el ciclo alcance al ledger de un `kind:
loop`, escrito por una sesión de tarea que el nodo ya no tiene. Un nodo que
declara artifacts y no resuelve ningún runner — `bash`, `check`, `gate` — no
tiene ciclo y lo dice en su tipo: es un límite del sistema, no un olvido,
porque no hay a quién instruir y volver a correr un comando de shell es un
reintento y no una corrección.

Racional: la obligación nace de la frontera y no del kind — un artifact
interpretado se declara en `artifacts.produces`, que cualquier nodo puede
escribir —, y un ciclo escrito adentro de un ejecutor es un ciclo que los
demás no tienen: un `loop` que produce un ledger ilegible fallaba sin
apelación mientras el `prompt` de al lado se reparaba. Una sesión dedicada es
además más barata y más precisa que rehacer el nodo entero: el trabajo ya está
hecho y lo que falta es una transcripción. La forma viaja una sola vez: ya
está montada en el segmento estable del contexto (D129), así que el prompt de
la corrección nombra los problemas y no repite la gramática.

Descartados: reejecutar el prompt del nodo con la reparación al final (paga el
nodo completo para arreglar una transcripción, y solo existe en el kind cuyo
prompt es el nodo); copiar el ciclo en cada ejecutor que abre sesión (tercera
copia de una política de reintento, cada una con su propio tope); una clave de
workflow que lo active por nodo (segunda declaración de lo que
`artifacts.produces` ya declara).
