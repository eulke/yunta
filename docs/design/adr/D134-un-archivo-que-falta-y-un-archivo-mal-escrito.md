---
number: D134
title: "Un archivo que falta y un archivo mal escrito son dos fallas, no una"
status: revised
revises: []
revised_by: [D157]
---

# D134 — Un archivo que falta y un archivo mal escrito son dos fallas, no una

*(Revisada por D157: la composición por log agrega una tercera respuesta a la
misma pregunta, `Unheld`, para el artifact que ningún run tiene, y el cierre
por log una cuarta, `Undelivered`, para el documento que el nodo quedó
debiendo.)* La falla de un artifact declarado tiene dos variantes: `File {
path, problem }`, con `FileProblem` exhaustivo — `Missing` (con el nodo que lo
declaró), `Empty`, `Oversized` (con los dos números sobre la mesa) y
`Unreadable` (con lo que dijo el filesystem) —, y `Content(Report)`, los
problemas del contenido de un documento cuya kind fija su forma. El ciclo de
reparación (D131) toma exactamente `Content`.

Racional: la pregunta que separa las dos es la que decide si vale la pena
pagar otra sesión, y ninguna reescritura del contenido alcanza a un archivo
que no existe o que el filesystem no entrega. Con un solo tipo y un código de
texto adentro, esa pregunta había que hacérsela con un predicado que cada
llamador podía olvidar y que ningún compilador defendía; con dos variantes la
contesta el `match`, y una quinta causa de archivo vuelve a abrir la decisión
en cada lugar que decide.

Descartados: un `Problem::File { code: String }` con un predicado que
clasifica por código (un conjunto abierto de cadenas gobernando una decisión
de control de flujo); un booleano `repairable` en el payload (dato derivable
que puede terminar contradiciendo a la falla que acompaña).
