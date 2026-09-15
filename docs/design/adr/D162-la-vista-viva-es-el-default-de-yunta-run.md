---
number: D162
title: "La vista viva es el default de `yunta run`, y no toma la terminal"
status: revised
revises: [D45, D46]
revised_by: [D164]
---

# D162 — La vista viva es el default de `yunta run`, y no toma la terminal

*(Revisada por D164: el contenido común es la crónica derivada; una terminal
conserva lo que cerró y muestra lo abierto en la región, y sin terminal salen
todos los momentos.)* `yunta run` dibuja sobre una terminal una región anclada
de varias líneas de texto plano en stderr —`indicatif` para la región,
`dialoguer` para los prompts de gates y preguntas—, con el trabajo terminado
subiendo al scrollback real del usuario, sin alternate screen, sin raw mode y
sin captura de mouse; `--follow` se elimina y `--quiet` reduce la salida a la
línea del run id y a la advertencia de presupuesto que §8.6 declara
accionable; sin terminal (pipe, CI, `TERM=dumb`, `NO_COLOR`) el mismo
contenido sale como líneas append-only, una por evento, y la primera anuncia
la degradación con `live view off (<reason>): one line per event`; el engine
expone una frontera de observación de solo presentación que le entrega cada
evento a quien dibuja en el momento en que lo escribe, así el CLI ya no relee
el log que él mismo acaba de escribir; el visor navegable de pantalla completa
con `ratatui` queda diferido detrás de un flag.

Racional: el que mira un run quiere saber que avanza, y el único lugar donde
eso ya se sabe es el proceso que lo está corriendo — el flag dejaba mudo el
caso frecuente y, para entregar lo que ese proceso ya tenía en la mano,
montaba un poller que le preguntaba a SQLite cada 500 ms lo que él mismo había
escrito un instante antes, como lo declara el rustdoc de `spawn_follower` en
`crates/cli/src/commands/run.rs`, mientras §8.5 del Contrato publicaba esa
superficie como "consumiendo el stream de eventos". Tomar la terminal, además,
apaga `ISIG` en raw mode y con eso el `Ctrl-C` tipeado deja de llegar a
`tokio::signal::ctrl_c` —probado sobre un pty— justo en la herramienta cuya
propiedad central es que todo subproceso muere con su árbol en cada camino de
cancelación; la región nativa no necesita raw mode ni puente. Sacar el flag no
le debe un ciclo de deprecación a nadie: D141 dejó la evidencia de que no hay
binario publicado.

Descartados: conservar `--follow` como flag opt-in (deja el default mudo y
conserva el poller que relee sus propias escrituras); `ratatui` para la vista
por default (toma la terminal, y de ahí se siguen los cuatro peligros medidos
—`Ctrl-C` que no llega, una consulta bloqueante de posición de cursor de hasta
2 s en la construcción y en cada resize, la historia del run borrada al
angostar la ventana sin configuración que lo evite, y el scrollback y la
selección de texto confiscados por la captura de mouse—, ninguno defecto de la
librería, y suma 42 crates reales y dos entradas de política contra los 8
crates y cero cambios de `deny.toml` de la región nativa); un formulario que
se redibuja para gates y preguntas (medido peor para un lector de pantalla
—cada palabra en una coordenada absoluta, sin orden de lectura, contra una
oración— y obliga a una segunda redacción de la misma pregunta para el camino
plano); escribir la región anclada a mano (más barato en bytes y en crates,
pero pasan a ser propios el redibujo consciente del wrap y una terminal en
memoria para testearlo).
