---
number: D95
title: "CPTV en tokens siempre; moneda opcional y nunca fuente de verdad (Contrato §8.4)"
status: accepted
revises: []
revised_by: []
---

# D95 — CPTV en tokens siempre; moneda opcional y nunca fuente de verdad (Contrato §8.4)

El precio por token cambia con proveedor y modelo; el engine no debe saber de
pricing para funcionar. Campo `pricing:` opcional en config (`{model:
cost_per_1k_tokens}`): si está, `stats` y el recibo agregan una línea de
estimado en moneda **junto a** los tokens; si no está, todo se expresa en
tokens y no se inventa nada. Corrige el ejemplo de recibo de RFC-0003 §1, que
mostraba "\$X" sin que existiera de dónde salía.

Descartado: que CPTV sea nativamente monetario (acopla el engine a tablas de
precios que cambian sin aviso).
