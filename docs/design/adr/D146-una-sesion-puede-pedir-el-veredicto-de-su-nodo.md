---
number: D146
title: "Una sesión puede pedir el veredicto de su nodo antes de terminar"
status: revised
revises: []
revised_by: [D156, D157]
---

# D146 — Una sesión puede pedir el veredicto de su nodo antes de terminar

*(Revisada por D156: lo interpretado entra por su propia herramienta. Revisada
por D157: la herramienta confirma un archivo que la sesión escribió o lee el
documento que el run ya tiene, llamando a las mismas dos funciones que el
cierre.)* El servidor MCP por sesión ofrece `yunta_check_artifact { name? }`,
que corre `artifacts::verify_one` — la verificación del cierre, no una segunda
lectura de ella — sobre lo que el nodo declara, con los nombres ya
renderizados. En éxito informa lo que el engine leyó (`3 task(s) registered:
…`), no solo que el archivo parsea.

Racional: el seam entre escribir y juzgar era el límite de la sesión, así que
todo error costaba una sesión entera; adentro, cuesta una llamada. Quedan tres
capas de costo creciente —el contrato antes de escribir (D143, D144), la
verificación antes de cerrar, la reparación después de fallar— y el
presupuesto de reintentos pasa a respaldar lo que las dos primeras no
atraparon en vez de ser el mecanismo de corrección. Que sea el mismo código
que el cierre es la condición que lo hace valer: un veredicto que difiere del
cierre enseña confianza equivocada, que es peor que no ofrecer ninguno. El
cierre sigue siendo el único juez; la herramienta es idéntica pero consultiva.

Descartados: montar `document_shape` en la sesión (devuelve lo que el agente
ya tiene en el prompt); dejar que la herramienta valide un texto que el agente
le pasa en vez de leer el archivo del disco (verificaría un borrador, no el
artifact que el cierre va a leer).
