---
number: D143
title: "Una regla se enuncia donde se aplica, y desde ahí se publica"
status: accepted
revises: []
revised_by: []
---

# D143 — Una regla se enuncia donde se aplica, y desde ahí se publica

Cada kind interpretado declara una lista `RULES` de `Rule { code, demand }` al
lado de las funciones que la hacen cumplir: `demand` es lo que la regla exige,
en el vocabulario de quien escribe el documento, y viaja en el contrato que la
sesión recibe antes de escribir nada. La cadena que lo sostiene tiene tres
eslabones, los tres verificados: una regla no puede existir sin un `RuleCode`
porque `Problem::rule` lo exige; todo `RuleCode` pertenece a la lista de algún
documento; y el contrato que cada puerta entrega contiene la exigencia de cada
entrada.

Racional: el ledger tenía nueve reglas y el ejemplo publicado enunciaba cuatro
en prosa suelta, una de ellas falsa — un run real gastó su presupuesto de
reparación entero descubriendo por fallo una regla que el engine ya conocía.
Un presupuesto de reintentos existe para lo imprevisible; que pague por lo que
el sistema sabía y no dijo es el sistema cobrándole al usuario su propio
silencio.

Descartados: mantener las reglas en la prosa del ejemplo y atarlas con un test
(un test puede afirmar que una oración existe, no que dice la verdad, que es
exactamente cómo la afirmación sobre `manual_review` quedó contradiciendo a
`rules.rs`); publicar el JSON Schema junto al ejemplo (expresa cero de las
nueve reglas — `schemars` no emite `dependentRequired`, así que el par
`manual_review`+`justification` es irrepresentable, y sobre `justification`
afirma la negación de la regla, por estar fuera de `required`); una tabla de
tipos generada (mismo límite: los tipos no son las reglas, y el formato del
ejemplo ya publica un requisito condicional que una tabla no puede).
