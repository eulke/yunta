# CLAUDE.md — Yunta

Yunta es un motor de workflows determinista para agentes de código: un workflow se
declara en YAML, el engine lo ejecuta, verifica cada resultado mecánicamente y deriva
todo el estado por replay de un log de eventos append-only. Lo que el engine exige a
sus agentes, este repo te lo exige a vos.

## El ciclo de una tarea

Una tarea está terminada cuando cada paso cumplió su "Listo cuando". Se recorren en
orden; ninguno se cierra antes.

1. **Ubicá la tarea.** Qué pide, qué archivos toca, qué criterio la cierra, qué
   decisión registrada la respalda.
   Listo cuando enunciás las cuatro cosas con evidencia del repo, sin adivinar.

2. **Levantá lo que cambia el plan.** Una contradicción, una imposibilidad, un diseño
   mejor, una dependencia oculta o un alcance mayor del previsto frenan la tarea:
   describís el problema con evidencia, las alternativas y tu recomendación, y el
   humano decide.
   Listo cuando avanzás sobre una decisión registrada, o la duda quedó descartada con
   evidencia.

3. **Ponelo en rojo.** El test que falla por la razón exacta que la tarea corrige.
   Listo cuando corre, falla, y falla por esa razón.

4. **Construí lo ideal.** La solución completa tal como está planificada, con el
   diseño que tendría en un repo nuevo, tocando todo lo que haga falta.
   Listo cuando el diff no contiene ningún atajo, duplicado ni pieza "para después".

5. **Ponelo en verde.** El criterio de la tarea y los mismos checks que corre CI,
   ejecutados por vos.
   Listo cuando viste pasar cada uno; un resultado que no ejecutaste no existe.

6. **Dejá el texto en presente.** Rustdoc, ayuda, guía y mensajes describen lo que el
   código hace ahora, para un tercero.
   Listo cuando ningún texto del diff nombra la tarea, un plan, lo que había antes ni
   lo que vendrá.

7. **Commiteá un tema.** Mensaje convencional; el porqué cuando el diff no lo dice.
   Listo cuando el commit se entiende sin esta conversación.

## Juicio

Los criterios con los que se decide. Entre dos opciones, gana la que los cumple
mejor.

- **Evidencia.** Hecho es lo que corre y pasa. El agente nunca marca su propio
  trabajo; vos tampoco.
- **Ideal.** Arquitectónicamente correcto, ergonómico, con separación de capas,
  idiomático, escalable. El esfuerzo y la cantidad de archivos no son criterio: un
  atajo es una tarea sin terminar.
- **Levantar.** Lo que ningún diseño ni decisión registrada fija (un umbral, un
  nombre, un comportamiento, un alcance) se levanta con alternativas y
  recomendación; decide el humano. Un comentario que admite "no hay número en ningún
  lado" es una decisión que alguien tomó solo.
- **Replay.** Todo estado se deriva del event log. Un evento registra lo que pasó con
  el valor real; el estado en memoria es una cache de una sola invocación.
- **Degradación explícita.** Lo que no se puede hacer se dice con un evento y un
  diagnóstico que nombran qué faltó y qué se hizo en su lugar. Un archivo inválido
  se reporta como inválido. Una capacidad ausente falla.
- **Parsear es validar.** Lo inválido es irrepresentable por tipo: identificadores
  con newtype y constructor validado, enums exhaustivos, YAML de autor que rechaza
  claves desconocidas nombrándolas. La tolerancia vive solo en lo persistido y
  versionado, donde un lector viejo lee a un escritor nuevo y marca lo que no
  entendió.
- **Núcleo puro.** Decidir (derivar estado, elegir el próximo paso) es una función
  pura; ejecutar es la cáscara. Reloj, ids y azar entran inyectados.
- **Frontera.** El engine conoce a un adapter solo por lo que declara; un path, un
  flag o un nombre de CLI dentro del engine es una capacidad que falta. Un adapter
  declara exactamente lo que construyó.
- **Dueño.** Cada subproceso nace en su process group, queda registrado y muere con
  el árbol completo en todo camino de cancelación; cada task de tokio conserva su
  handle.
- **Secreto.** La config nombra variables de entorno; los valores viven solo en el
  entorno del hijo; `Debug` los redacta; el prompt viaja por stdin.
- **Un lugar.** Cada convención, umbral, mensaje y helper vive en un único sitio y
  todo lo demás lo consume. La segunda copia señala el lugar que falta, y se crea en
  el mismo PR.
- **La documentación gana** al código cuando difieren, salvo decisión registrada en
  contra. Su silencio es un paso 2.

## Fronteras

- Las dependencias entre crates van estrictamente hacia abajo; el engine desconoce
  SQLite y a cada CLI concreto, y el compilador lo impone.
- Un trait existe donde hay una frontera real con más de una implementación.
- Una dependencia nueva se defiende en el PR (compilación, tamaño del binario,
  superficie de auditoría) y entra en el crate que la necesita: el binario estático
  chico es una feature.

## Código

- Errores tipados por módulo con la causa conservada; el texto para humanos se
  produce una sola vez, en el borde, y dice qué hacer. En código de producción todo
  camino de fallo devuelve un error; los tests son el único lugar donde algo entra
  en pánico.
- En código async, disco y base de datos van por la vía async o por
  `spawn_blocking`; los subprocesos son de tokio y nacen gobernados.
- Un span por run y por nodo, con `run_id` y `node_id` como campos.
- Un archivo cerca de 500 líneas o una función cerca de 50 es una señal que se
  atiende en el PR que la cruza.
- Tests que nombran el comportamiento que prueban; la infraestructura de test vive
  en el crate de soporte; el entorno (home, PATH, proxy) se inyecta; la sincronización
  es explícita, nunca un sleep; property tests para replay, idempotencia y resume.

## Expresión

Todo texto del repo lo lee un tercero que no estuvo en ninguna conversación.

- **Presente.** El código dice lo que hace ahora; la documentación describe lo que el
  sistema hace ahora; lo que existe se describe como definitivo y una limitación se
  declara como límite del sistema. Dos únicas excepciones al presente: una decisión
  registrada cita la alternativa que descartó; un cambio visible para el usuario va
  al changelog.
- **Lo que el lector necesita.** Un comentario existe cuando el código no puede
  decirlo solo: la razón de un mecanismo desde primeros principios, un invariante que
  el tipo no expresa, una trampa que no se ve.
- **Autocontenido.** Un texto se entiende con lo que está en el repo.
- **Accionable.** Un error dice qué pasó y qué hacer; una ayuda dice qué hace el
  comando; un rustdoc dice qué garantiza el ítem y qué exige de quien lo usa.
- **Inglés** en todo lo que un usuario o un tercero lee; español solo en los
  documentos de diseño internos.

## Vocabulario

| Usá | En lugar de |
|---|---|
| adapter | driver, backend |
| runner (binding adapter + modelo + agente) | agente |
| agente (nombrado, del adapter) | subagente, persona |
| `runner:` | `role:` |
| pack | plugin |
| executor | plugin |

"Rol" es una palabra de prosa; en YAML, JSON y código el concepto se llama runner.
