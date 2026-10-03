---
number: D173
title: "Un nodo que pregunta, pregunta: el hecho es `questions_asked`, el nodo espera entre preguntar y responder, lo que depende de las respuestas es del nodo siguiente, e `interactive` se retira"
status: accepted
revises: [D86]
revised_by: []
---

# D173 — Un nodo que pregunta, pregunta: el hecho es `questions_asked`, el nodo espera entre preguntar y responder, lo que depende de las respuestas es del nodo siguiente, e `interactive` se retira

## Contexto

D86 fija que un nodo que necesita información de una persona entrega un
artifact de preguntas y termina, que las respuestas son un artifact que el
nodo siguiente consume como contexto, y que `interactive: true` es un dato de
presentación. El engine no lo hace verdadero por tipo ni por hecho: el nodo
del pack de referencia `grill` declara `produces: [questions, brief.md]` con
un prompt que dice "Once answered, write the brief", y `yunta check` lo acepta.
Un run real termina así:

```
paused — node `plan` failed: context `artifact:grill/brief.md` on node
`plan`: the artifact `brief.md` (declared by node `grill`) was never
produced — this run's log holds no such artifact
```

El log de `grill` dice la verdad y la derivación la descarta: el cierre
escribe `node_failed { Artifacts [brief.md Missing] }`, la derivación lee
`Waiting` de cualquier `node_failed` de un nodo con un artifact `questions`
aceptado, y la ronda de respuestas emite `node_started`, `questions_answered`
y `node_finished` por su cuenta, sin pasar por `close_node`. El nodo queda
`finished` debiendo `brief.md`, y el diagnóstico aparece un nodo después,
prestado. El log registra que alguien respondió (`questions_answered`) y
nunca que el nodo preguntó. `interactive: true` viaja hasta la única
superficie y ella lo descarta: es el resto del nodo conversacional que D86
descartó, y nadie le dio nunca la presentación que D86 le reservó.

Un panel de tres diseños independientes —el hecho en el log, la regla en el
tipo, y la continuación con una segunda sesión— juzgado por tres lentes
(fidelidad al plan, verificación contra el código, el autor de workflows)
respalda esta decisión; los tres coinciden en la regla y difieren en el
hecho; gana el hecho explícito.

## Decisión

1. **Un nodo que pregunta, pregunta.** Un nodo que declara `questions` es
   `kind: prompt`, no declara ningún otro artifact, no vive dentro de un
   `parallel`. `interactive` se retira del nodo, del trait `HumanInteraction`
   y del schema: un YAML de autor que lo escriba recibe el rechazo de clave
   desconocida nombrándola. `yunta check` rechaza lo demás nombrando el corte: el artifact que
   dependía de las respuestas se produce en un nodo que sigue al que
   pregunta y monta sus preguntas y sus respuestas como contexto.
2. **Preguntar es un hecho del log.** El kind `questions_asked` —hash del
   documento, ids pendientes, tokens de la sesión que preguntó— es el par
   de `questions_answered`, en el dominio `gates`. El nodo cierra entero
   antes de registrarlo (hooks `after`, scope, artifacts) y espera entre los
   dos como un gate interno, sin segundo `node_started`; el `node_finished`
   llega después de la respuesta. `node_failed` es siempre un fallo.
3. **Una puerta para responder, muchas superficies.** La consola pregunta en
   el lugar cuando está; sin consola, el run se estaciona con sus preguntas
   registradas y espera a `yunta resume`, a la tool MCP `answer_questions` o
   a un pull request (A-14). La superficie disponible decide cómo se
   presentan las preguntas; el nodo no lo declara. Toda respuesta
   entra por la misma función, que valida contra las preguntas y registra la
   aceptación y el `questions_answered`; un nodo respondido termina sin
   sesión, también al reanudar tras un corte y también cuando la respuesta
   llegó por MCP.
4. **Un nodo que no preguntó nada no espera** y deja un documento de
   respuestas vacío, derivado por el engine, para que el nodo siguiente monte
   siempre lo que declaró.

## Racional

D86 ya lo decía: el nodo termina al preguntar. Lo que faltaba era que el tipo
lo impidiera y que el log lo dijera. Una espera deducida de un fallo es la
forma del defecto: el mismo evento significa dos cosas según otro dominio.
Con `questions_asked` la derivación lee un hecho y no una coincidencia, la
crónica tiene un momento para "preguntó", `status` sabe qué espera, y una
forja que publique preguntas por pull request tiene el par que publica y lee,
como `gate_waiting`/`gate_resolved`.

La segunda sesión —"contestadas las preguntas, el nodo sigue"— es la lectura
que el prompt de `grill` suponía. Cuesta decidir qué contexto lleva, qué
intento es, cuántas rondas caben y cómo se reanuda esa sesión; y no compra
nada que el nodo siguiente no dé ya: una sesión fresca con las respuestas en
su contexto, sin capacidad de adapter. El autor escribe dos nodos donde
escribía uno, y `check` le dice cuáles.

`interactive` no sobrevive: un nodo que declara `questions` ya dijo todo lo
que hay que decir, y el único caso que un flag distinguiría —mirar el run y
no querer ser interrumpido— se resuelve no mirando. Un flag que el autor
escribe y nada lee es una clave inerte, y D120/D121 fijan su destino: se
implementa o se retira.

No hay tag publicado (D141): un log escrito antes de esta decisión, con
`node_failed { "asked N question(s)…" }` tras la aceptación de `questions`,
deriva `Failed`; no se lee de las dos formas.

## Alternativas descartadas

- **La segunda sesión con las respuestas en contexto**: contradice D86 y
  trae un mecanismo de continuación que el plan no nombra.
- **Derivar la espera de la aceptación de las respuestas, sin kind nuevo**:
  `node_finished` dejaría de significar terminado, la señal seguiría cruzando
  del ledger de artifacts al estado del nodo, y un corte entre la aceptación
  y `questions_answered` perdería en silencio quién respondió.
- **`Failure::Questions` dentro de `node_failed`**: preguntar no es fallar;
  `Failure` se conserva, y cada lector de `node_failed` tendría que saber que
  ese fallo no lo es.
- **Darle a `interactive` el consumidor que nunca tuvo**, la consola
  preguntando en el lugar sólo con el flag y `check` rechazándolo sin
  `questions`: un flag más para declarar lo que `questions` ya dice.
- **Retirar `Channel::Mcp`**: D167 lo conserva, y la tool que lo produce es
  una segunda superficie de una puerta que ya existe.
