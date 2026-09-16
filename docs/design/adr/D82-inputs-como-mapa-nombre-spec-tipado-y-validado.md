---
number: D82
title: "`inputs` como mapa nombre→spec, tipado y validado al crear el run (Contrato §2.3)"
status: revised
revises: []
revised_by: [D157]
---

# D82 — `inputs` como mapa nombre→spec, tipado y validado al crear el run (Contrato §2.3)

*(Revisada por D157: `type` suma `document`, que se lee por la puerta de su
`kind` y nace como artifact del run, y cuyo valor congelado es el hash y no la
ruta.)* Reemplaza la lista `[{name, required}]`: el nombre como clave
garantiza unicidad por formato y sigue el idioma del resto del schema. Campos:
`type` (`string|number|boolean|enum|path`), `required`/`default` mutuamente
excluyentes, `description` (alimenta `list_workflows` y `--help` — sin ella el
catálogo es una lista de nombres sin sentido), y validación por tipo
(`values`, `pattern`, `min_length`, `min`, `max`). **`path` valida existencia
siempre**, sin flag `exists` y sin distinguir archivo de directorio: el
filesystem va a fallar igual, y hacerlo al crear el run convierte un error
tardío y caro en uno inmediato; quien necesite nombrar un archivo inexistente
está pidiendo un `string`. Todo se valida antes del primer token; los defaults
se resuelven ahí y se congelan en el manifest (resolverlos por nodo sería
estado no determinista); `check` verifica que todo `{{inputs.x}}` refiera a un
input declarado.

Descartados: lista con `name` adentro (unicidad por validación en vez de por
formato), `exists: false` (no tiene casos reales: un path de salida es un
string), y `type: dir` (caso poco frecuente que un criterio o hook ya cubre).
