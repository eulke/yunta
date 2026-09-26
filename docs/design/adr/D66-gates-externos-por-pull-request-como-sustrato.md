---
number: D66
title: "Gates externos por pull request como sustrato multi-persona de v1 (Contrato §5.6)"
status: accepted
revises: []
revised_by: []
---

# D66 — Gates externos por pull request como sustrato multi-persona de v1 (Contrato §5.6)

El estado del engine es local (D52), así que un gate que otra persona debe
resolver no tiene dónde vivir hasta que exista `serve`. En vez de adelantar el
daemon, el gate delega en la forja: publica artifacts + PR con `run_id`, y la
aprobación del PR es el evento que lo resuelve. Propiedades: el aprobador no
necesita Yunta ni acceso a la máquina; resolución por **pull** (consulta al
despertar, sin webhooks ni daemon — el modelo sin infraestructura queda
intacto); evidencia mecánica (usuario, timestamp y SHA de la API, con
detección de cambios post-aprobación); comentarios como `finding_posted` y
contexto del nodo correctivo; degradación explícita a consola sin
credenciales. Límite documentado: desbloquea runs, no los avanza — avanzar
requiere el estado local, y el caso "cualquiera corre cualquier run" sigue
siendo `serve`.

Descartados: adelantar un `serve` mínimo solo para gates (infraestructura para
un caso acotado), y webhooks (daemon escuchando contradice D05).
