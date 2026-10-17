---
number: D183
title: "Una sola palabra para el árbol que una unidad no aísla: `none` queda, `inherit` se retira"
status: accepted
revises: []
revised_by: []
---

# D183 — Una sola palabra para el árbol que una unidad no aísla: `none` queda, `inherit` se retira

## Contexto

Hay dos enums para un concepto: `Isolation { Worktree, None }`, que gobierna si
un run trabaja en árbol propio o en el checkout de quien lo invocó, y
`WorkflowIsolation { Worktree, Inherit }`, que gobierna lo mismo para un hijo
`kind: workflow`. `run/workflow_exec/mod.rs:295` es la prueba: una función que
traduce `WorkflowIsolation::Inherit` a `Isolation::None`, dos vocabularios con un
diccionario entre ellos.

M32 pone `isolation:` en el nodo en general —toda unidad de trabajo tiene un
árbol, y la pregunta por unidad es si es propio o el de quien la parió—. Dejar
las dos palabras después de eso sería la declaración dispersa (V4) generada por
el propio mecanismo que viene a consolidar: un nodo podría llevar `isolation:` con
un vocabulario y un nodo `kind: workflow` con otro.

La herramienta la usan hoy tres personas que la están probando y reportando.
Retirar una palabra cuesta un mensaje, no una migración. Esta decisión se toma
bajo esa condición y la deja escrita: con usuarios afuera, el cálculo sería otro.

## Decisión

1. **Queda `none`; `inherit` y `WorkflowIsolation` se retiran.** La razón no es
   precedencia sino alcance: `none` es cierta en todos los niveles y `inherit`
   sólo en algunos. El rustdoc de `Isolation` ya lo había dicho — «`inherit`
   (sub-runs only) isn't a value here — a first-level run has no parent to
   inherit from» —: un run de primer nivel no tiene unidad padre, tiene un
   checkout, así que una palabra que nombra una relación de parentesco no puede
   ser la general. `none` nombra lo que efectivamente pasa en todos los casos:
   esta unidad no se aísla del árbol que recibió.

2. **El rechazo va en el YAML de autor; la tolerancia, en lo persistido.**
   Un `.yunta/config.yaml` o un workflow que diga `isolation: inherit` no parsea,
   y el error nombra el reemplazo — que es lo que este repo hace con toda clave
   desconocida de autor. Un manifest ya congelado que lleve `inherit` se lee como
   `none`, porque un manifest es persistido y versionado y ahí la tolerancia sí
   corresponde. Las dos mitades de «parsear es validar» aplicadas donde va cada
   una, en vez de una sola regla estirada a los dos lados.

3. **El campo sigue llamándose `isolation:`.** `worktree` nombra el mecanismo y
   `none` la ausencia, lo que es asimétrico leído en frío; en la práctica
   `worktree` es el default que nadie escribe y `none` es la única que un autor
   teclea, así que la asimetría no aparece en ningún documento real. Renombrar el
   campo movería más superficie de la que arregla.

## Racional

Un lugar: una convención vive en un único sitio y todo lo demás la consume. Dos
enums con un traductor en el medio es exactamente la forma que ese principio
existe para prohibir. Parsear es validar: lo retirado es irrepresentable en la
entrada de autor, y lo que ya se persistió sigue legible.

## Alternativas descartadas

**Que quede `inherit`.** Es la palabra más descriptiva para un hijo, y bajo el
encuadre «toda unidad tiene un padre» parecería la general. Ese encuadre es falso
en el borde: el padre de un run de primer nivel es el checkout de una persona, no
una unidad de trabajo. Una palabra que hay que explicar en su propio caso límite
no es la que unifica.

**Una palabra nueva para las dos** (`shared`, `ambient`). Retira dos palabras que
funcionan para instalar una tercera que nadie escribió todavía, y deja los
documentos existentes sin ninguna de las dos formas conocidas.

**Renombrar el campo a `tree:`** para que valor y campo concuerden
gramaticalmente. Gana precisión de lectura y cuesta `defaults.isolation`, el tipo
`Isolation`, `resolved_isolation` y cada mención en documentación — superficie
que no está rota.
