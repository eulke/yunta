# Decisiones por archivo

Cada archivo `DNNN-slug.md` es una decisión con front-matter (`number`,
`title`, `status`, `revises`, `revised_by`), su contexto, racional y
alternativas descartadas. Desde D164 las decisiones nuevas viven acá; las
anteriores siguen en [`../adrs.md`](../adrs.md), que también indexa estas.
`cargo xtask adr --check` deriva de estos archivos el índice de
[`../adrs.md`](../adrs.md), exige numeración sin huecos ni duplicados, que
toda cita `D\d+` de `docs/**/*.md` resuelva, y recíprocos
`revises`/`revised_by`.

La numeración continúa la del registro.
