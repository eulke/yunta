---
number: D94
title: "Capa org por variable de entorno con default por plataforma (Contrato §2.2)"
status: accepted
revises: []
revised_by: []
---

# D94 — Capa org por variable de entorno con default por plataforma (Contrato §2.2)

`/etc/yunta/config.yaml` en POSIX y `%ProgramData%\yunta\config.yaml` en
Windows, override `YUNTA_ORG_CONFIG` — mismo patrón que `YUNTA_HOME` (D52).
Windows deja de ser una excepción no resuelta y pasa a ser el mismo mecanismo
con otro default, coherente con que Yunta es target de release en las tres
plataformas (T12.2).

Descartado: hardcodear un único path POSIX y dejar Windows sin resolución
explícita.
