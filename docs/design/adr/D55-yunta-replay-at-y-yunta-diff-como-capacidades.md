---
number: D55
title: "`yunta replay --at` y `yunta diff` como capacidades de producto post-v1 temprano (RFC-0003 §2)"
status: accepted
revises: []
revised_by: []
---

# D55 — `yunta replay --at` y `yunta diff` como capacidades de producto post-v1 temprano (RFC-0003 §2)

Reconstrucción exacta de lo que cada agente vio en cada momento (hashes de
segmento D42 + secuencia de eventos); diff entre corridas por primer punto de
divergencia de contexto. La data ya se persiste completa desde v1 — la
decisión es no recortar nada de esa persistencia "por eficiencia" para no
cerrar esta puerta.
