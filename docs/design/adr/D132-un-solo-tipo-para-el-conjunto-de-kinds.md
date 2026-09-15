---
number: D132
title: "Un solo tipo para el conjunto de kinds interpretadas: `ArtifactKind`"
status: revised
revises: []
revised_by: [D168]
---

# D132 — Un solo tipo para el conjunto de kinds interpretadas: `ArtifactKind`

*(Revisada por D168: `label()` dice `tasks document`; el alias solo lee lo
persistido.)* El `kind:` que un workflow declara, el argumento de `yunta
schema`, el enum de `kind` de la tool `document_shape` y el documento del que
habla un diagnóstico son el mismo conjunto y llevan el mismo tipo.
`ArtifactKind` reúne lo que ese conjunto sabe de sí mismo: `ALL` (las kinds en
el orden en que las puertas las listan), `label()` (cómo se nombra a un
lector: "task ledger"), `as_str()`/`Display`/`FromStr` (el valor que viaja en
YAML y en la línea de comando) y `listed()` (la frase que enumera las válidas,
escrita una sola vez y consumida por el error de kind desconocida y por la
ayuda del comando). Un test afirma, variante por variante, que `as_str()` es
exactamente lo que serde serializa y que ese valor vuelve por `FromStr`;
ninguna otra parte del workspace repite esas cadenas.

Racional: dos tipos para un conjunto obligan a una conversión en cada
frontera, y la conversión es el lugar donde las dos listas se separan sin que
el compilador diga una palabra — agregar una cuarta kind costaba trece
ediciones, cuatro de ellas puras repeticiones de los mismos tres nombres. Con
un tipo, agregarla es un `match` no exhaustivo en cada lugar que de verdad
tiene algo que decidir.

Descartados: dos enums con una conversión entre ellos (deja las listas
sincronizadas por disciplina, que es exactamente lo que falla); derivar las
cadenas del JSON Schema en runtime (mete la generación de schemars en el
binario que se distribuye, contra el techo de tamaño de D124).
