---
number: D02
title: "Rust, binario único estático"
status: revised
revises: []
revised_by: [D77]
---

# D02 — Rust, binario único estático

*(Revisada por D77: la lista de crates ya no incluye axum, porque `serve`
salió de Yunta por completo.)* Sin runtime que instalar: `curl | sh` y listo —
argumento de venta para equipos. Crates base: tokio, serde/serde_norway,
petgraph, rusqlite, clap, rmcp.
