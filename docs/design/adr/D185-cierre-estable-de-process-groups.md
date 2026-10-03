---
number: D185
title: "Cerrar un process group exige una observación estable de sus miembros"
status: accepted
revises: []
revised_by: []
---

# D185 — Cerrar un process group exige una observación estable de sus miembros

## Contexto

Mandar `SIGKILL` una vez al grupo no basta para probar que murió todo el árbol.
Un proceso que crea hijos mientras el kernel entrega la señal puede dejar un
descendiente vivo. Si el shell líder termina y ese descendiente heredó stdout o
stderr, la lectura de los pipes espera indefinidamente aunque el comando que
Yunta lanzó ya haya salido. El mismo dueño de proceso aparece en comandos del
engine y en las sesiones de los adapters; dos rutinas de cierre distintas
dejarían una de esas rutas con la carrera.

Además, recoger al líder antes de terminar de señalar libera su PID. El PID del
líder también identifica al process group, así que su reutilización podría
dirigir una señal tardía a un proceso ajeno.

## Decisión

**`yunta_core::process` es dueño de un único cierre de process groups.** El
engine y las sesiones de adapters lo usan, y mantienen vivo el handle del líder
hasta que el grupo está cerrado.

1. Observar la salida del líder con `waitid(WNOWAIT)`: queda waitable y su PID
   no puede reciclarse antes de la última señal. Recogerlo recién al terminar
   el cierre.
2. Detener el grupo con `SIGSTOP` e inspeccionar sus miembros con la API nativa
   de cada plataforma. Si queda alguno ejecutándose, volver a detenerlo. Exigir
   dos observaciones seguidas con el mismo conjunto de miembros ya detenidos o
   terminados; ningún padre detenido puede crear hijos entre esas observaciones.
3. Enviar `SIGKILL` al grupo y confirmar que no queda ningún miembro ejecutable.
   Los zombies cuentan como terminados porque ya no ejecutan código. Un grupo
   vacío no necesita señales.
4. Leer stdout y stderr concurrentemente y drenarlos después de cerrar el
   grupo. Mantener cancelación y plazo activos durante el cierre y el drenaje;
   si se disparan, repetir el cierre antes de recolectar al líder.
5. Propagar el error original de inspección, señal, lectura o espera. Una
   inspección fallida dispara un intento de `SIGKILL` de emergencia y nunca se
   interpreta como grupo vacío. Errores de drenaje conservan las salidas que ya
   se capturaron.

La consulta del estado waitable y el envío de señales usan bindings seguros;
la inspección usa `procfs` en Linux y `libproc` en macOS. Los lectores y handles
permanecen bajo el dueño hasta que se recolectan o se abortan y esperan. D181
sigue definiendo las dos etapas de Ctrl-C: el primer Ctrl-C detiene el trabajo y
el segundo aborta lo que esa limpieza todavía sostiene.

## Racional

El conjunto estable después de detener a todos sus miembros elimina la ventana
en la que un padre sigue creando descendientes mientras se mata el grupo. La
identidad del grupo permanece atada al hijo no recolectado, y cerrar los
descendientes antes de esperar el fin de los pipes resuelve el caso en que el
shell sale primero. Un helper en core asegura que engine y adapters aplican la
misma propiedad de árbol.

## Alternativas descartadas

**Enviar `SIGKILL` una vez y esperar al líder.** No prueba que un hijo nacido
durante la entrega de la señal recibió el cierre, y el líder puede salir antes
que un descendiente que conserva los pipes.

**Recolectar el líder antes de cerrar el grupo.** Permite que el PID se recicle
mientras sigue siendo el identificador del grupo al que se enviarán señales.

**Tratar un error al enumerar procesos como grupo vacío.** Convierte falta de
permisos, errores del kernel o fallas del binding en una falsa garantía de
limpieza. El cierre falla con su causa y hace el intento de emergencia.

**Mantener una rutina distinta por consumidor.** Duplica el mismo cierre en
engine y adapters, y deja que sus carreras y errores diverjan con el tiempo.
