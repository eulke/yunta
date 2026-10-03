---
number: D68
title: "Distribución por binarios precompilados, con instalador, cargo, Homebrew y Action oficial"
status: accepted
revises: []
revised_by: []
---

# D68 — Distribución por binarios precompilados, con instalador, cargo, Homebrew y Action oficial

Prioridad: releases de GitHub (matriz Linux musl/macOS/Windows, con checksums)
→ instalador one-liner → `cargo install yunta` (con los crates de librería
publicados, lo que además habilita embeber el engine) → tap propio de Homebrew
→ GitHub Action `setup-yunta` (canal de adopción por CI ajeno) → imagen de
contenedor → winget/scoop según demanda. Versionado semver del binario,
independiente del versionado del schema de workflows (`yunta_schema`,
compatibilidad N y N-1). Ningún release sale sin que los packs de fábrica
pasen `check` y corran con `mock`, y sin instalación verificada en contenedor
limpio por plataforma.
