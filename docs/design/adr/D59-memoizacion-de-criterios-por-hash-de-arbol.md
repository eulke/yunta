---
number: D59
title: "Memoización de criterios por hash de árbol + short-circuit del pre-check (Contrato §5.4)"
status: revised
revises: []
revised_by: [D177]
---

# D59 — Memoización de criterios por hash de árbol + short-circuit del pre-check (Contrato §5.4)

*(Revisada por D177: el corto-circuito del pre-check se retira; la fase
evalúa el conjunto entero y nombra cada sorpresa.)* El engine no reejecuta un comando cuyo resultado ya conoce: clave =
hash(comando + tree_hash + env declarado + versión de config); el tree_hash de
git captura todas las entradas, así que cualquier cambio — edición de agente,
hook, instalación — invalida automáticamente. Elimina la única redundancia
real (los `guard` que el post-check de una tarea acaba de correr y el
pre-check de la siguiente repetiría) sin tocar los criterios propios de cada
tarea, que siempre se ejecutan. Reglas: alcance intra-run (jamás cross-run),
**sin opt-out por criterio** — los criterios son deterministas respecto del
árbol por definición (§5.1) y lo no determinista (hora, red, servicio externo)
pertenece a un nodo `bash`, que nunca se memoiza y además queda visible en el
DAG con evento y re-ruta propios — y `criteria_checked` registra ejecución vs
reutilización, de modo que recibo y replay muestran la diferencia. Mecanismo
puramente interno: memoización en el storage propio, sin componentes externos.

Descartados: caché cross-run (suposiciones que no sobreviven a cambios de
máquina o entorno); una property `cacheable: false` por criterio (escape para
un caso que el diseño ya considera un error — habría dejado pasar criterios no
deterministas disfrazados de criterios, peor que prohibirlos); y eliminar la
fase de pre-check para ahorrar tiempo — es la fase que valida al validador
(sin ella, criterios vacuos producen runs verdes vacíos y el post-check no
distingue "lo logré" de "ya estaba").
