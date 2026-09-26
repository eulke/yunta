---
number: D140
title: "Lo que un diagnóstico enumera está atado por un test a lo que el parser acepta"
status: revised
revises: []
revised_by: [D156]
---

# D140 — Lo que un diagnóstico enumera está atado por un test a lo que el parser acepta

*(Revisada por D156: el recorrido de forma desaparece, y con él las listas de
claves y el test que las comparaba contra el JSON Schema generado — serde
parsea y `Problem::Parse` nombra el path del valor que rechazó. La mitad que
rige es la primera: un conjunto cerrado tiene una sola escritura de sus
valores.)* Los conjuntos cerrados que un mensaje lista —las kinds de artifact
(D132), la escalera de severidad de un finding, los tipos de respuesta de una
pregunta— tienen una sola escritura de sus valores, atada variante por
variante a lo que serde deriva. Las listas de claves con las que el recorrido
de forma decide si una clave es desconocida o si falta una obligatoria siguen
siendo constantes, y un test lee el JSON Schema generado y afirma, para cada
uno de los tres tipos, que `properties` es la unión de obligatorias y
opcionales y que `required` es la lista de obligatorias.

Racional: un diagnóstico que dice "los valores válidos son estos" es una
afirmación sobre el parser, y con dos escrituras del mismo conjunto el día que
una cambia el mensaje manda a quien lo lee a escribir algo que el parser
rechaza; del mismo modo, un campo agregado al tipo y no listado hacía que el
recorrido reportara como desconocida una clave perfectamente válida. Derivar
las listas de los tipos en runtime resuelve lo mismo pero mete la generación
de schemas en el binario que se distribuye (D124, D139), mientras que un test
no ocupa un byte del binario y falla en el PR que crea la divergencia.

Descartados: derivar las listas en runtime (tamaño de binario permanente por
una verificación que se hace una sola vez en CI); sostener la correspondencia
por revisión (es la disciplina que ya había fallado).
