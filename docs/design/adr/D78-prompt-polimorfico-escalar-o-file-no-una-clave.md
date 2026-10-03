---
number: D78
title: "`prompt` polimórfico (escalar o `{file: ...}`), no una clave separada (Contrato §9.3)"
status: accepted
revises: []
revised_by: []
---

# D78 — `prompt` polimórfico (escalar o `{file: ...}`), no una clave separada (Contrato §9.3)

Los prompts serios son largos: embebidos en YAML entierran la estructura del
DAG, pelean con comillas e indentación, y producen diffs ilegibles cuando
cambia una línea. Se admite `prompt: { file: prompts/plan.md }` con la ruta
relativa al workflow, renderizado con los mismos templates y **con el
contenido dentro del hash del manifest** (editar el archivo a mitad de run no
altera ese run, I3); `check` valida existencia y no-vacuidad. Forma elegida:
un valor polimórfico en la property existente, no una clave `prompt_file:` —
es el mismo idioma con que las fuentes de contexto expresan procedencia,
mantiene una sola clave, y la forma del valor (escalar vs mapa) elimina toda
heurística de "si parece una ruta" (un prompt de una línea que se parezca a un
path jamás se interpreta como archivo). Extensiones futuras (otras
procedencias, composición por partes) son campos de ese mapa, no claves
nuevas. Beneficios laterales: prompts en archivo son contenido `stable` para
el ensamblado cache-friendly (§9.1), y quedan revisables como archivos en un
PR y en el inventario de `pack audit` (D71), no como bloques YAML.
