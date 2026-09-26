---
number: D126
title: "Gate interno sin superficie humana: el nodo queda sin estado y la razón de pausa nombra al gate"
status: accepted
revises: []
revised_by: []
---

# D126 — Gate interno sin superficie humana: el nodo queda sin estado y la razón de pausa nombra al gate

Cuando un gate interno no tiene quién lo resuelva, el run pausa con
`run_paused` cuya razón nombra al gate; el nodo no gana un estado propio.
`status` muestra esa razón y un caso de test la afirma con la aserción
`events:` (D89).

Racional: un gate que nadie resolvió ni terminó ni falló; darle un estado
inventaría un hecho. La razón de pausa ya está en el log y basta exponerla
donde se lee el run.

Descartado: un estado de nodo `waiting-for-nobody`; un evento nuevo para lo
que `run_paused` ya registra.
