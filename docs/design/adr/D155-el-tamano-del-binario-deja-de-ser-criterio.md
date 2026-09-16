---
number: D155
title: "El tamaño del binario deja de ser criterio de decisión mientras el sistema se estabiliza"
status: accepted
revises: [D124, D142]
revised_by: []
---

# D155 — El tamaño del binario deja de ser criterio de decisión mientras el sistema se estabiliza

El techo de CI pasa a 32 MiB (33554432 bytes), contra los 18,75 MiB que mide
hoy el musl estático: sigue fallando, pero solo ante una dependencia enorme, y
deja de arbitrar si una dependencia entra. La regla de dependencias se
invierte: una librería mantenida que resuelve el problema gana a una
implementación propia, porque lo que no se escribe no se mantiene ni se
audita.

Racional: el techo derivado de la medición del día (D142, medido más 10 %)
protege un valor que hoy compite con uno mayor — que el sistema funcione de
punta a punta. Su costo real ya se vio: el escapado de TOML se escribió a mano
en parte para no pagar una dependencia, y esa versión era incorrecta en casos
que sus propios tests aprobaban, porque asumía que toda cadena TOML es un
basic string; la librería cuesta 25 KB, el 0,13 % del binario. Un techo cuyo
margen es del 10 % convierte cada dependencia en una negociación de bytes, y
esa negociación no es hoy la que decide si el proyecto sirve. Lo que no
cambia: el binario de Linux sigue siendo estático —verificado en el mismo
job—, porque eso es cómo se distribuye y no cuánto pesa; y `cargo deny` sigue
gobernando licencias y superficie de auditoría, que son seguridad, no tamaño.
CI sigue publicando el tamaño en cada corrida, de modo que el historial existe
para el día en que un techo ajustado vuelva a valer la pena: reponerlo es
re-medir, no reconstruir el instrumento.

Descartados: medir sin bloquear (deja de avisar de una dependencia enorme, que
es lo único que el guard todavía tiene para aportar); eliminar el paso (borra
también el historial, y reponer el techo pasaría a ser reconstruirlo desde
cero); seguir derivando el techo de la medición más un margen (es la regla que
produjo esta misma discusión dos veces, D142 y esta).
