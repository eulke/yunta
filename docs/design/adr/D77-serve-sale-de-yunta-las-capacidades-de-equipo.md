---
number: D77
title: "`serve` sale de Yunta: las capacidades de equipo son un proyecto separado"
status: accepted
revises: []
revised_by: []
---

# D77 — `serve` sale de Yunta: las capacidades de equipo son un proyecto separado

Yunta no tendrá daemon, feature `serve`, módulo ni configuración reservada: es
tooling sin infraestructura, sin excepciones (D05 se vuelve absoluto). Estado
compartido, gates remotos, dashboard en tiempo real, notificaciones de equipo,
triggers sin padre común y registry gestionado pertenecen a un proyecto con
repositorio, ciclo de vida y modelo de negocio propios — MVP que comparte y
coordina pero **no ejecuta** (el trabajo sigue corriendo en máquinas de
personas o CI), y ejecución gestionada solo si la demanda real lo justifica.
Acoplamiento mínimo por diseño: el servidor ingiere el event log que el engine
ya produce (`events.jsonl`, append-only y versionado), la publicación es
opt-in, y la caída del servidor jamás impide que un run avance. Compensaciones
dentro de Yunta: los gates remotos ya funcionan sin servidor vía pull request
(D66), y **`yunta stats` incorpora visualización gráfica en terminal** (barras
por nodo/rol, sparklines históricos, comparación entre modos, `--json` para
herramientas) como reemplazo del dashboard.

Descartados: `serve` como feature del binario (contradice la identidad y
arrastra supuestos de servicio al engine) y un servidor que además ejecute en
el MVP (es un orquestador distribuido, otro producto).
