---
number: D142
title: "El techo de tamaño del binario se re-mide contra el binario de hoy"
status: revised
revises: []
revised_by: [D155]
---

# D142 — El techo de tamaño del binario se re-mide contra el binario de hoy

*(Revisada por D155: el techo pasa a 32 MiB y deja de derivarse de la medición
del día.)* El techo pasa a 21546800 bytes: el musl estático medido con el
perfil de release (19588000 bytes) más 10 %.

Racional: el guard existe, según su propio texto, para que una dependencia que
infla el binario estático aparezca en CI y no en el `curl | sh` de alguien; el
baseline anterior (17761312 bytes) era una medición vieja, y el crecimiento
ordinario del sistema había consumido su margen hasta dejar `main` en el 98,6
% del techo, de modo que el guard ya no avisaba de una dependencia sino de que
la medición había caducado. El cambio que lo cruza no agrega ninguna
dependencia y el perfil ya optimiza por tamaño todo lo que puede (`lto =
"fat"`, `codegen-units = 1`, `strip`, `panic = "abort"`), así que no había
nada que recortar salvo el trabajo mismo. Re-medir devuelve al guard su margen
y su propósito, y deja la próxima subida del 10 % como lo que el texto dice
que es: una decisión, no una deriva.

Descartados: bajar a `opt-level = "z"` para entrar bajo el techo viejo
(probablemente alcanza, y en un motor cuyo camino caliente es orquestar
subprocesos costaría poco, pero cambia el binario que se publica y merece su
propia medición y su propia decisión, no ser el efecto lateral de otro
cambio); un margen del 2 % en lugar del 10 % (obliga a tomar esta misma
decisión con una frecuencia que la vuelve trámite, que es como una decisión se
convierte en deriva); recortar código propio hasta entrar (no hay grasa
identificada, y lo que saldría es el trabajo que corrige los defectos).
