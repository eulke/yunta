---
number: D75
title: "`yunta graph` en v1; constructor visual solo con demanda demostrada"
status: accepted
revises: []
revised_by: []
---

# D75 — `yunta graph` en v1; constructor visual solo con demanda demostrada

`yunta graph <workflow|run_id>` emite el DAG como diagrama (Mermaid por
default, DOT opcional), con aristas de dependencia y de re-ruta diferenciadas,
y — dado un `run_id` — los estados derivados del log. Es derivación pura: el
grafo ya está en memoria, no requiere eventos nuevos ni participación de
agentes, y la salida se renderiza en GitHub, en un PR o en documentación sin
herramientas extra. Nombre `graph` sobre `show`/`view`/`map`: es el término
del dominio, es lo que alguien buscaría en `--help`, y no solapa con `status`
(cómo viene) ni con `list` (qué hay). Sobre el **constructor visual de
workflows**: no entra al plan. Para un dev, arrastrar cajas es más lento que
escribir YAML con autocompletado, git y diff; el segmento donde tendría valor
real es el usuario no técnico, y para ese usuario lo difícil no es la
topología sino escribir criterios verificables — que ninguna interfaz
resuelve. Si alguna vez se construye: web, y con la regla dura de que el
**YAML sigue siendo la fuente de verdad** (el editor genera y lee YAML, sin
formato propietario) — en cuanto la interfaz tenga estado que el YAML no
exprese, se rompen el versionado, los packs y el modelo entero.
