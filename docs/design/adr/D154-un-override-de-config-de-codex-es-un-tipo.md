---
number: D154
title: "Un override de config de codex es un tipo, y el TOML lo escribe `toml`"
status: accepted
revises: []
revised_by: []
---

# D154 — Un override de config de codex es un tipo, y el TOML lo escribe `toml`

`ConfigOverride` construye cada `-c key=value` y rinde el valor con el crate
`toml`, que elige por sí mismo la forma que cada valor necesita — basic
string, literal string para una ruta con `\` y comillas, multilínea para un
salto.

Racional: la ruta del directorio de artifacts y la URL del listener se
interpolaban crudas entre comillas, así que una ruta con `"` o `\` cierra el
literal y el resto del valor pasa a leerse como sintaxis. El test que cubría
ese argumento re-derivaba su expectativa con el mismo `format!` sin escapar
que producción, de modo que el defecto era invisible para su propia prueba.
Los tests afirman ahora lo único que el adapter puede prometer —que el CLI lee
de vuelta el valor que se le dio— y no cómo quedó escrito, que es decisión del
renderer. `default-features = false` con `display` y `serde` deja el parser
fuera del binario: la dependencia entra con cuatro crates (`toml`,
`toml_writer`, `toml_datetime`, `serde_spanned`), todos MIT/Apache-2.0, y
cuesta 25 KB sobre 19,5 MB de binario de release. El parser entra solo por
`dev-dependencies`, que es lo que permite que la prueba sea un round-trip.
Rinde además las dos entradas de `argv` juntas, con lo que olvidarse el `-c`
deja de ser posible.

Descartados: un escapador de basic strings propio (funciona y pasa sus tests,
pero asume que toda forma es basic string, es superficie a mantener y a
auditar, y el spec de TOML no es nuestro para seguirlo a mano); reusar
`escape_dot` de la CLI (colapsa el texto a una línea, correcto para una
etiqueta de grafo y no para una ruta).
