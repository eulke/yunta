# D114 — `runner` en pack manifests, payloads y JSON; `role` solo en prosa

**Estado:** propuesta.

## Contexto

El vocabulario reserva `runner:` como clave YAML y deja "rol" solo para la prosa. Sin embargo `pack.yaml` declara `requires.roles`, y payloads y JSON de `stats`/`receipt` usan `role`/`runner_role`.

## Decisión propuesta

`pack.yaml` declara `requires.runners`; los payloads y el JSON usan `runner`. Los campos nuevos son aditivos y el lector tolerante (D70) sigue leyendo los payloads ya persistidos con el nombre anterior.

## Racional

Una palabra, un significado (D27, D85, D87): que el schema y los datos usen la palabra que el vocabulario prohíbe en el schema es una contradicción que cada lector nuevo tiene que resolver por su cuenta.

## Alternativas descartadas

- Cambiar el vocabulario: `runner` ya nombra el binding en config y workflows; el cambio iría en contra de todo lo publicado.
