---
number: D151
title: "El presupuesto de reparación por defecto es dos, medido"
status: retired
revises: []
revised_by: [D156]
---

# D151 — El presupuesto de reparación por defecto es dos, medido

*(Retirada por D156: sin ciclo no hay presupuesto; la medición que la sostenía
es la evidencia de D156.)* Racional: era uno, sobre el razonamiento de que una
reescritura que tiene los diagnósticos en mano converge en el primer intento o
no converge. Medido contra sesiones reales: seis ledgers rechazados,
reescritos con su diagnóstico, convergieron cinco de seis en la primera vuelta
y seis de seis en la segunda. Un documento además puede deber dos vueltas por
construcción — un valor del tipo equivocado corta el parseo, así que las
reglas que solo valen sobre un documento parseado no pueden reportarse en la
misma pasada, y quien escribe se entera de ellas una vuelta después. La sesión
de reparación pasa además a recibir las herramientas per-run, que es donde más
valen: está reescribiendo un documento que el engine ya rechazó una vez.

Descartados: subirlo más (ninguna medición lo respalda); colapsar las dos
capas reportando reglas sobre un parseo tolerante (inventa un documento que
nadie escribió y produce diagnósticos sobre él).
