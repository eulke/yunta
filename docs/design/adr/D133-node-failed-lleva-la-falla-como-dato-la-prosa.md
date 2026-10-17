---
number: D133
title: "`node_failed` lleva la falla como dato; la prosa se produce al leer"
status: accepted
revises: []
revised_by: []
---

# D133 — `node_failed` lleva la falla como dato; la prosa se produce al leer

El payload de `node_failed` lleva `failure`, que es o `Message { outcome }` —
una falla que el engine enuncia en una frase — o `Artifacts { artifacts }` —
los artifacts declarados que no cerraron, cada uno con su archivo y sus
problemas (D134). Toda redacción sale de ahí, en el borde que la muestra:
`status`, el recibo y la instrucción de reparación leen el mismo valor, así
que ninguna puede contradecir a los hechos que tiene detrás. El layout del
texto vive en `yunta_core::text` — una línea, bloque colgante, indentado, y el
bloque de problemas que spec-tasks §4 fija — y no sabe nada de diagnósticos:
es el único lugar donde se decide cómo se ve un problema, y de ahí salen
también los errores del CLI. La variante `Message` va segunda en un enum sin
etiqueta, con lo cual un log escrito antes, que lleva `outcome:` solo, se lee
como `Message` sin migración ni pérdida: es la regla de tolerancia de D70 para
lo persistido.

Racional: replay — un evento registra lo que pasó con el valor real, y una
frase no es un valor. Congelada en el log, no se puede volver a redactar para
otro lector, no se puede contar y no se puede corregir cuando la redacción
mejora; mientras tanto cada superficie la re-envolvía y el CLI terminaba
parseando la frase para recuperar lo que el engine ya había tenido en tipos un
instante antes de escribirla.

Descartados: conservar `outcome` al lado del campo estructurado (dos versiones
del mismo hecho, y la única que un lector viejo ve es la que no se puede
arreglar); un `Failure` etiquetado (ningún log ya escrito lleva el `tag:`, y
la tolerancia dejaría de ser gratis); dejar el armado del bloque en cada
superficie (el CLI ya tenía su copia, que decía `3 error(s)` donde el engine
decía `3 errors`).
