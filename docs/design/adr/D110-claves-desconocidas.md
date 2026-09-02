# D110 — Claves desconocidas: rechazo en todo YAML de autor, tolerancia solo en lo persistido

**Estado:** propuesta.

## Contexto

Los tipos de config, workflow, pack, caso de test y ledger se parsean con serde sin `deny_unknown_fields`. Una clave mal escrita (`modes` por `mode`, `depends-on` por `depends_on`) se ignora en silencio y el autor descubre el error recién cuando el run se comporta distinto de lo que escribió. Los payloads del event log y el manifest, en cambio, se leen con lector tolerante (D70) para que un binario nuevo abra logs viejos.

## Decisión propuesta

Todo YAML escrito por una persona — config en sus tres capas, workflows, `pack.yaml`, casos de test y ledgers — rechaza claves desconocidas con un error que nombra archivo, clave y las claves válidas en ese nivel. Todo lo que el engine persiste y vuelve a leer — eventos, manifest, lock de packs — conserva el lector tolerante.

## Racional

Parsear es validar: un estado inválido que el tipo no puede representar no debe entrar por una clave que el tipo no conoce. El costo del rechazo es cero para el autor correcto y es la única forma de que el error aparezca en `check`, antes de gastar tokens. La tolerancia en lo persistido es una decisión distinta (compatibilidad N/N-1 del log) y no se mezcla con la frontera de autoría.

## Alternativas descartadas

- Warning en `check` por clave desconocida: un warning que nadie lee es un error diferido.
- Tolerancia uniforme: mantiene el estado actual y la fuente de errores silenciosos.
