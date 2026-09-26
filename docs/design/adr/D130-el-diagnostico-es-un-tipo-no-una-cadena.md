---
number: D130
title: "El diagnóstico es un tipo, no una cadena"
status: accepted
revises: []
revised_by: []
---

# D130 — El diagnóstico es un tipo, no una cadena

Un `Diagnostic` es un sujeto y un problema: el sujeto en el vocabulario del
documento (`task \`t1\``, `task \`t1\`, criterion 1`, nunca `tasks[0]`), el
problema con un código estable por clase, tipado y contable (D135). Los
diagnósticos viajan en grupo y con su documento: una lectura fallida reporta
en un `Report` todos los problemas de ese archivo de una vez, con la kind que
fija su forma y el path donde se abre, y `node_failed` lleva un `Report` por
artifact que no cerró (D133). El texto para humanos se produce una sola vez,
en el borde que lo muestra, con el formato que spec-tasks §4 fija; la
instrucción para el agente que va a reescribir el archivo sale del mismo valor
y se redacta en el engine, que es donde se arma un prompt.

Racional: "con diagnóstico" sostiene I11 y aparece más de una docena de veces
en el corpus sin estar definido en ningún lado, y un concepto sin definición
no recibe un tipo. Lo que ocupaba su lugar era una cadena armada en el punto
de la falla, con la prosa cruda del deserializador adentro y el detalle de las
siete reglas del ledger reducido a un conteo, que llegaba cruda a ocho
superficies y que el recibo ya evitaba a propósito. Sin tipo no hay dos
lectores, y el ciclo de D131 necesita uno distinto del que necesita una
persona. Que sea dato lo vuelve además contable.

Descartados: limpiar la cadena en cada superficie (el estado anterior llevado
a su conclusión: cada lugar reimplementa el saneado y ninguno recupera lo que
se perdió en el `join`); conservar la prosa al lado del dato estructurado (dos
redacciones del mismo hecho que pueden discrepar, y la que el log congela es
justamente la que no se puede rehacer — D133 la reemplaza sin romper ningún
log ya escrito); traducir la prosa del deserializador por coincidencia de
texto (ata el vocabulario del producto a los mensajes internos de una
dependencia).
