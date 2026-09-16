---
number: D137
title: "El diagnóstico no lleva posición: el sujeto es la ubicación"
status: accepted
revises: []
revised_by: []
---

# D137 — El diagnóstico no lleva posición: el sujeto es la ubicación

Un diagnóstico es sujeto y problema; el par línea/columna y el campo que lo
llevaba salen del tipo y del `schemas/events.json` publicado.

Racional: la lectura de dos pasadas trabaja sobre un valor YAML ya parseado,
que no conserva posiciones, así que el campo viajaba nulo en todos los
diagnósticos escritos, y llenarlo exige otro parse — una feature con su propio
diseño, no un hueco que se deja abierto en un schema publicado. Además el
sujeto en vocabulario del documento (`task \`t1\`, criterion 1`) ubica mejor
que un número de línea a quien va a reescribir el archivo, que es el lector
para el que el diagnóstico existe. D120 y D121 fijan el par de salidas que
tiene una clave inerte: se implementa o se retira, nunca se documenta como
inerte.

Descartados: conservarla nula hasta que exista el parser posicional (es la
clave que no hace nada que D121 sacó de la config); cambiar el parser de toda
la frontera por uno que conserve posiciones (un rediseño de la lectura para un
dato que el sujeto ya da).
