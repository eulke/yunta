---
number: D149
title: "El directorio de artifacts del run entra explícitamente al conjunto escribible de la sesión"
status: revised
revises: []
revised_by: [D156, D157]
---

# D149 — El directorio de artifacts del run entra explícitamente al conjunto escribible de la sesión

*(Revisada por D156: `artifact_dir` viaja solo cuando el nodo declara un
artifact opaco. Revisada por D157: lo que entra al conjunto escribible es el
staging del nodo, `scratch/staging/<node_id>/`, y no el directorio de
artifacts, que es del engine.)* `SessionRequest` lleva `artifact_dir` cuando
el nodo declara artifacts, y cada adapter lo traduce a su mecanismo
(`--add-dir`, `sandbox_workspace_write.writable_roots`).

Racional: los dos CLIs confinan la escritura al directorio de trabajo, y el
directorio del run nunca está adentro — verificado vivo contra el binario
real, que responde «I don't have permission to write to that path». El nodo
moría entonces con «the document was declared by node X and never produced»:
se le pedía al agente escribir un archivo que después se le prohibía crear.
Funcionaba por accidente solo con `permissions: full`, donde al no pasarse
`--tools` queda Bash y el agente escribe con una redirección que esquiva el
chequeo de ruta.

Descartados: mover los artifacts adentro del worktree (su diff es lo que lee
el chequeo de scope, y el artifact pasaría a leerse como trabajo del agente);
que el adapter derive la ruta del layout del run (mete el layout del engine
adentro de la frontera).
