# Decisiones por archivo

Cada decisión del proyecto es un archivo `DNNN-slug.md` de este directorio, con
su front-matter, su contexto, su racional y las alternativas descartadas. El
índice de [`../adrs.md`](../adrs.md) sale de estos archivos: se edita la
decisión, nunca el índice.

El front-matter declara cinco campos y ninguno más:

```
---
number: D164
title: "El run se lee como una crónica derivada, y cada superficie la dispone"
status: accepted
revises: [D162, D45]
revised_by: []
---
```

`number` es el número de la decisión, escrito como lo escribe toda cita del
corpus —`D01` a `D09` con cero, de `D10` en adelante sin él— y es el que el
nombre del archivo repite. `status` es una de tres palabras:

| estado | qué dice |
|---|---|
| `accepted` | rige tal como está escrita |
| `revised` | una decisión posterior cambió parte de lo que decidió; la nota del cuerpo dice qué parte |
| `retired` | una decisión posterior la retiró: lo que decidió ya no rige |

`revises` y `revised_by` son las dos caras de la misma relación: si una
decisión nombra a otra en `revises`, la otra la nombra en `revised_by`. Lo que
cambió lo cuenta la nota del cuerpo; los dos campos son la relación que se
verifica.

`cargo xtask adr` regenera el índice y `--check` lo compara byte a byte con el
comprometido, después de exigir el front-matter completo, numeración sin huecos
ni duplicados, que toda cita `D\d+` de `docs/**/*.md` resuelva a una decisión y
que cada revisión esté anotada de los dos lados. CI lo corre.

Una decisión nueva toma el número siguiente al último.
