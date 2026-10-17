---
number: D33
title: "Secretos: solo env vars declaradas en el manifest; el event log jamás los contiene"
status: accepted
revises: []
revised_by: []
---

# D33 — Secretos: solo env vars declaradas en el manifest; el event log jamás los contiene

(redacción por el engine, digests en eventos de adapter — I12/O3, defensa en
profundidad).

Consecuencia: el redactor toma los valores de `secret_names` más los tokens de
sesión; un secreto de menos de cuatro caracteres impide arrancar el run, y un
payload que no se lee de vuelta tras redactarse se rechaza.
