---
number: D76
title: "Coherencia interna de modo: error de `check`, no warning (Contrato §10.1)"
status: accepted
revises: []
revised_by: []
---

# D76 — Coherencia interna de modo: error de `check`, no warning (Contrato §10.1)

Un modo que incluye un nodo cuyo `on_failure.goto` — o cuya opción de gate —
apunta a un nodo excluido de esa variante es error de validación.

Racional: que el destino exista en el archivo pero no en el modo que se va a
correr es la misma referencia rota que un `goto` hacia un nodo inexistente (ya
rechazado en T1.3); tratarlo como warning significa que el flujo revienta
cuando el lint falla — después de gastar tokens, por algo detectable antes del
primer token — y contradice tanto el propósito de la validación estática como
I11. Los warnings quedan para lo genuinamente dudoso, donde el autor puede
tener razón (p. ej. push directo a la rama base, D48); acá no la hay: si un
modo no incluye el nodo de corrección, ese modo tampoco debe incluir la
re-ruta hacia él. El mensaje del error nombra las dos salidas posibles
(incluir el destino, o quitar la re-ruta en esa variante). Nota: esto no
restringe la libertad de los modos (D44 sigue intacto: nombres, cantidad y
contenido son del autor) — valida coherencia, no contenido.
