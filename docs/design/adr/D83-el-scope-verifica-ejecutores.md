---
number: D83
title: "El scope verifica ejecutores no determinísticos; los nodos determinísticos no lo necesitan (Contrato §6)"
status: accepted
revises: []
revised_by: []
---

# D83 — El scope verifica ejecutores no determinísticos; los nodos determinísticos no lo necesitan (Contrato §6)

Se documenta la razón, que estaba implícita y podía llevar a "arreglar" un
no-problema: un agente puede hacer algo distinto de lo que se le pidió y solo
la comparación lo revela; un comando hace lo que su autor escribió, y
verificarlo es overhead sin información. Consecuencias: el scope es
obligatorio en tareas del ledger, opcional en nodos (el engine lo verifica si
se declara), y **no declararlo no produce error ni warning** — exigirlo
saltaría en casi todo nodo legítimo (`git push`, `gh pr create`, correr una
suite) y un aviso que salta siempre es ruido que se aprende a ignorar, con el
mismo razonamiento de D71. Quien quiera exigirlo usa `permissions` (§6.1),
donde es una política elegida por una organización y no un default molesto.
Los hooks quedan bajo el scope del nodo (I13) por compartir su diff, no porque
se desconfíe de ellos.
