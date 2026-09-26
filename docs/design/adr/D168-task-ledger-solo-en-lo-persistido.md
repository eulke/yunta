---
number: D168
title: "`task-ledger` se acepta solo al leer lo persistido; en YAML de autor y en el CLI se rechaza nombrando `tasks`"
status: accepted
revises: [D110, D132]
revised_by: []
---

# D168 — `task-ledger` se acepta solo al leer lo persistido; en YAML de autor y en el CLI se rechaza nombrando `tasks`

El alias `task-ledger` de `ArtifactKind` deja de vivir en el derive del tipo
y pasa a la lectura de lo persistido y versionado: manifest y event log lo
leen como `tasks`. Un workflow, un pack, una config o un argumento de CLI con
`task-ledger` se rechaza con un diagnóstico que nombra `tasks`.
`compatibility.md` describe exactamente eso.

Racional: D110 y CLAUDE.md fijan que la tolerancia vive solo en lo
persistido; un alias en el derive hace válido para siempre, en superficies de
autor, un nombre retirado y prohibido por la tabla de vocabulario — y ningún
ADR lo había registrado.

Descartados: mantener el alias en todas las puertas (contradice D110);
retirarlo también de lo persistido (rompe la lectura de logs y manifests ya
escritos, contra I2).
