---
number: D56
title: "Conocimiento multi-proyecto por capas + knowledge pack org con flujo de promoción curado (RFC-0003 §4)"
status: accepted
revises: []
revised_by: []
---

# D56 — Conocimiento multi-proyecto por capas + knowledge pack org con flujo de promoción curado (RFC-0003 §4)

La fuente `knowledge` resuelve repo > usuario > org con scope declarable; la
capa org es un pack versionado (RFC-0002 íntegro: vendoring, lockfile,
congelado por run); la promoción repo→org es un workflow de Yunta con gate
curador — jamás automática.

Descartados: RAG automático como capa implícita (contaminación no auditable;
el RAG queda como fuente `mcp` explícita) y sincronización automática del
knowledge org (el conocimiento compartido cambia por decisión, como todo).
