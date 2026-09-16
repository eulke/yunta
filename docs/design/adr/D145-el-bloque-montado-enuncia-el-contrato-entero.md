---
number: D145
title: "El bloque montado enuncia el contrato entero y declara su precedencia"
status: accepted
revises: []
revised_by: []
---

# D145 — El bloque montado enuncia el contrato entero y declara su precedencia

El texto que el engine monta antes de la forma dice que lo que sigue es el
contrato completo del archivo — las claves, sus tipos y las reglas — y que
ante cualquier otra instrucción que lo describa distinto, esto es lo que el
engine aplica.

Racional: decía «any other key fails the node», nombrando claves desconocidas
como el veredicto, cuando lo que hace fallar un nodo son tres cosas y el run
que motivó el cambio murió por una regla; era el mismo defecto de «parece
autoritativo y no lo es» que hizo descartar el JSON Schema en D143. La
cláusula de precedencia resuelve además el conflicto con el prompt del autor,
que es prosa libre que puede restatar la forma y envejecer.

Descartados: detectar en `yunta check` que un prompt de autor restata una
forma (heurística sobre prosa, y un falso positivo en un check hoy exacto
cuesta confianza); dejar que gane el último texto que el modelo leyó (es no
decidir).
