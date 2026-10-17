---
number: D41
title: "Patrones de codebase fijados en CLAUDE.md"
status: accepted
revises: []
revised_by: []
---

# D41 — Patrones de codebase fijados en CLAUDE.md

Parse-don't-validate con newtypes y enums exhaustivos; functional core /
imperative shell (replay y decisiones del scheduler como funciones puras);
determinismo inyectado (Clock, IDs, entropía — jamás directos en el core);
máquinas de estado como enums; traits solo en fronteras reales (la señal de
violación: un branch por adapter concreto en el engine); concurrencia
estructurada con JoinHandles retenidos y CancellationToken; errores thiserror
por módulo con mensajes accionables en el borde; `#![forbid(unsafe_code)]`;
dependencias justificadas por PR con cargo-deny.
