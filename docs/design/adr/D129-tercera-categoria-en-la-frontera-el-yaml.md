---
number: D129
title: "Tercera categoría en la frontera: el YAML de agente, con su forma publicada en cada puerta"
status: revised
revises: []
revised_by: [D156]
---

# D129 — Tercera categoría en la frontera: el YAML de agente, con su forma publicada en cada puerta

*(Revisada por D156: la forma se publica igual en las cuatro puertas; dentro
de un run el documento se entrega o se reporta por herramienta, y el
diagnóstico deja de recorrer el documento.)* La frontera tiene tres categorías
y no dos: el YAML de autor (config, workflows, `pack.yaml`, casos de test,
fixtures) que `check` alcanza antes de gastar un token; el **YAML de agente**,
que es el contenido de todo artifact interpretado y nace a mitad del run; y el
persistido, con el lector tolerante de D70. Las tres rechazan claves
desconocidas, y la del medio suma dos obligaciones: la forma se publica a
quien la escribe, y un archivo ilegible se corrige (D131). La forma se publica
en cuatro puertas que rinden desde una sola constante por kind
—`Document::EXAMPLE`, en el directorio de esa kind (D136), con un test que la
lee de vuelta por el mismo parser— y que nombran la kind con un único tipo
(D132): el bloque que un nodo recibe como fuente de contexto derivada de su
propio `artifacts.produces` y montada en el segmento `stable`; la tool
`document_shape` del plano de control, cuyo enum de `kind` es el catálogo que
un cliente ve al conectarse; `yunta schema <kind>` (con `--json` para el
editor), que no necesita proyecto ni run; y el propio diagnóstico de una
lectura fallida, que al reportar una forma equivocada muestra la que se
espera. Dentro de un nodo la forma viaja una sola vez: está montada en el
contexto, así que el ciclo de reparación no la repite (D138). El registro de
schemas cubre las tres kinds interpretadas.

Racional: D110 clasificaba los ledgers como "escrito por una persona" cuando
el pack de referencia y el esqueleto de `yunta new --shape ledger` los
producen en una sesión de agente, con lo cual esa categoría tenía la mitad del
contrato — el rigor sin la gramática y sin la corrección — y el esqueleto que
el propio producto genera no funcionaba. Parsear sigue siendo validar: lo que
cambia es de quién es la culpa cuando el archivo sale mal. Publicar es además
lo más barato del sistema: los tipos ya llevan `schemars`, las fuentes de
contexto ya tienen clases de estabilidad, y el bloque es idéntico en cada
sesión del nodo, así que cae adentro del prefijo byte-estable del cache.

Descartados: relajar la validación de los artifacts de agente (convierte cada
divergencia en un dato perdido en silencio, la puerta de atrás que D110
cerró); dejar la gramática en manos del autor de cada workflow (es lo que
pasaba de facto, y duplica en cada workflow del ecosistema una forma que el
engine ya conoce); una clave nueva en el nodo (segunda declaración del mismo
hecho); publicar las formas como MCP resources (semánticamente correcto, pero
el soporte entre clientes es desparejo y una forma que el cliente no lista es
una puerta cerrada); publicar el JSON Schema en vez de un ejemplo comentado
(un modelo copia una forma mejor de lo que la deriva de una gramática, y el
schema pesa varias veces más en un bloque que viaja en cada sesión — sigue
siendo la salida de `--json`); y escribir en `.claude/` o en `CLAUDE.md` para
alcanzar a un agente cliente (`init` ya trata ese archivo como ajeno).
