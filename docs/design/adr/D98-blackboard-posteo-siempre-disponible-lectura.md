---
number: D98
title: "Blackboard: posteo siempre disponible, lectura restringida a después del join (Contrato §5.9)"
status: accepted
revises: []
revised_by: []
---

# D98 — Blackboard: posteo siempre disponible, lectura restringida a después del join (Contrato §5.9)

Revisión de D49: se mantiene el blackboard para grupos cooperativos (el caso
de anclaje que motiva `independent` como default es específico de grupos
evaluativos, D49 sigue intacto ahí), pero `yunta_get_blackboard` no devuelve
posteos de hermanos mientras el grupo corre — solo tras `join`, consolidado
como artifact/evento de cierre consumible por un nodo posterior al `parallel`.

Razón: dos corridas del mismo workflow con inputs idénticos pueden tener
timing real distinto entre sesiones; lectura en caliente haría que el
resultado dependiera de en qué orden llegaron los posts, no solo de su
contenido — el mismo tipo de no determinismo que D65 evita en el paralelismo
de tareas, aplicado aquí a grupos `parallel` genéricos. `yunta_post_finding`
no se restringe: postear nunca depende de si alguien más ya terminó.

Descartado: sacar el blackboard por completo (el caso cooperativo es real y
D80 ya prueba que el posteo universal no alcanza para coordinación entre
hermanos) y dejar lectura en caliente sin restricción (acepta no determinismo
evitable).
