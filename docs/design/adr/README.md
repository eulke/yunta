# Decisiones por archivo

Cada archivo `DNNN-slug.md` es una decisión con front-matter (`number`,
`title`, `status`, `revises`, `revised_by`), su contexto, racional y
alternativas descartadas. Desde D164 las decisiones nuevas viven acá; las
anteriores siguen en [`../adrs.md`](../adrs.md), que también indexa estas.
`cargo xtask adr --check` (plan de raíz, ítem 7-02) genera el índice, exige
numeración sin huecos y recíprocos `revises`/`revised_by`, y migra las
anteriores a este formato.

La numeración continúa la del registro.
