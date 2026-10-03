---
number: D120
title: "`defaults.on_failure: abort | continue` se implementan"
status: accepted
revises: []
revised_by: []
---

# D120 — `defaults.on_failure: abort | continue` se implementan

`abort` cierra el run como fallido tras el primer nodo fallido sin re-ruta;
`continue` marca como omitidos los nodos que dependen del fallido, sigue con
el resto y cierra el run como fallido al final. Ambos son pasos del scheduler
puro, emiten el evento correspondiente con el nodo causante y se prueban por
replay.

Racional: un valor aceptado y no aplicado es una promesa vacía; los dos
comportamientos tienen semántica obvia y son útiles — `abort` en CI,
`continue` en runs exploratorios.

Descartado: retirar los valores del schema.
