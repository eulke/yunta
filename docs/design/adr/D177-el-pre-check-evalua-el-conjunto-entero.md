---
number: D177
title: "El pre-check evalúa el conjunto entero y nombra cada sorpresa; el orden aprendido decide cuándo llega la evidencia"
status: accepted
revises: [D62, D59]
revised_by: []
---

# D177 — El pre-check evalúa el conjunto entero y nombra cada sorpresa; el orden aprendido decide cuándo llega la evidencia

## Contexto

D59 y D62 prometen un corto-circuito del pre-check —«basta que un
criterio no-`guard` falle para que la fase concluya»— que el Contrato
repite en §5.2 y §5.4. La fase existe para validar al validador: I6 exige
que **todo** criterio no-`guard` esté en rojo antes del trabajo, y §8.7
mide «criterio nunca en rojo en pre-check» sobre cada corrida. Una fase
que parara en el primer rojo nunca correría el criterio trivial ordenado
después, y dejaría a §8.7 sin datos. El código corre todos los criterios
—D167 construyó el orden aprendido— pero devuelve la primera sorpresa en
ese orden: con un criterio trivial y un guard roto a la vez, qué variante
y qué comando vuelven depende del orden, que es exactamente lo que D62
llama «un bug de determinismo» (plan de raíz, §11 L-95).

## Decisión

1. **El pre-check evalúa el conjunto entero.** Cada criterio corre; el
   veredicto es una función pura de lo que corrió —`surprises(task, runs)`,
   en el orden en que la tarea declara sus criterios— y nombra cada
   criterio trivial y cada guard roto que encontró (`Surprise`), nunca
   sólo el primero; viaja tipado hasta el borde que lo dice
   (`TaskOutcome::Blocked { surprises }`).
2. **El orden aprendido decide cuándo llega la evidencia.** Los criterios
   corren de menor a mayor duración histórica, tomada del log, de modo que
   la evidencia barata llega primero; el orden no decide qué se verifica
   ni qué se reporta.
3. **El corto-circuito se retira** de D59, D62 y del Contrato §5.2 y §5.4.

## Racional

La documentación gana al código salvo decisión registrada: esta es la
decisión. Un veredicto que depende del orden de evaluación no es un
veredicto; uno que nombra todo lo que encontró le ahorra al autor una
vuelta por cada sorpresa.

## Alternativas descartadas

Construir el corto-circuito: contradice I6 y vacía §8.7. Registrarlo como
deuda: una promesa que contradice un invariante no es deuda, es un error
de la promesa.
