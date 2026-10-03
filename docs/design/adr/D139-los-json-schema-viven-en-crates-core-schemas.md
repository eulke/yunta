---
number: D139
title: "Los JSON Schema viven en `crates/core/schemas/` y el binario los sirve embebidos"
status: accepted
revises: []
revised_by: []
---

# D139 — Los JSON Schema viven en `crates/core/schemas/` y el binario los sirve embebidos

`cargo xtask schema` escribe los nueve archivos ahí, CI falla cuando lo
comiteado difiere de lo que emiten los tipos, y `yunta-core` los incluye en
compilación, así que `yunta schema <kind> --json` imprime exactamente los
bytes que CI verificó.

Racional: un directorio en la raíz del workspace no pertenece a ningún package
— el día que los crates se publiquen no viaja con `yunta-core`, que es el
único que los necesita —, y derivar el JSON Schema al invocar el comando
obliga a linkear la generación de schemas en el binario que se distribuye,
unos 100 KB contra el techo declarado de D124, para producir un archivo que ya
existe y ya está verificado.

Descartados: conservar `schemas/` en la raíz (queda afuera del package que lo
usa y del paquete publicado); generarlo en runtime (paga tamaño de binario
permanente y abre la puerta a que lo que imprime el comando no sea lo que CI
revisó).
