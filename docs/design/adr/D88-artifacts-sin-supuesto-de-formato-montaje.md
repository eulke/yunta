---
number: D88
title: "Artifacts sin supuesto de formato; montaje por referencia para todo lo que no sea texto corto (Contrato §4 y §9.1)"
status: accepted
revises: []
revised_by: []
---

# D88 — Artifacts sin supuesto de formato; montaje por referencia para todo lo que no sea texto corto (Contrato §4 y §9.1)

El engine verifica existencia y hash de cualquier archivo: PDFs, planillas,
imágenes y dumps son artifacts legítimos. El montaje de contexto se define por
naturaleza, no por tamaño solamente: **inline solo texto bajo
`limits.inline_context_bytes`; todo lo demás por referencia** — el engine pasa
la ruta y el adapter decide (los CLIs modernos leen esos formatos
nativamente). Cuando un formato necesita conversión para ser útil (PDF a
texto, planilla a CSV), es un **nodo previo** determinístico y memoizable,
visible en el DAG. `limits.max_artifact_bytes` actúa como guardia contra
accidentes, no como prohibición: al excederse el nodo falla con diagnóstico.

Descartados: un campo `binary: true` que prohibiera montar (mataría casos
reales — un pliego en PDF que el workflow debe leer es el punto de partida de
flujos enteros fuera del software) y extracción automática por el engine
(magia opaca, específica por formato, no memoizable ni visible; es trabajo
determinístico y pertenece a un nodo).
