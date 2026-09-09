## Context

El proyecto parte sin aplicación existente. Palantir es la herramienta de escritorio local definida por la propuesta; controla procesos CLI ya autenticados y repositorios Git elegidos por la persona usuaria. Las especificaciones del cambio describen las conductas observables.

## Goals / Non-Goals

**Goals:**

- Proveer una interfaz Kanban rápida para iniciar, observar, revisar y continuar trabajo delegado a agentes locales.
- Aislar cada ticket ejecutable mediante una rama Git creada antes de iniciar su agente.
- Mantener el historial de configuración y ejecuciones para que los resultados sean auditables y continuables.
- Soportar el flujo de planificación padre/sub-kanban sin mezclar subtareas con tarjetas del tablero principal.

**Non-Goals:**

- Gestionar credenciales, cuentas, cuotas o claves de proveedores de IA.
- Hacer merge, push o borrado de ramas de forma automática.
- Ejecutar agentes remotamente o sincronizar tableros entre equipos.
- Ofrecer compatibilidad universal con cualquier CLI de agentes en la primera versión.

## Decisions

### Aplicación de escritorio con backend local separado de la interfaz

Palantir se implementará con Tauri 2, React y TypeScript para la interfaz, y Rust para el proceso local responsable de almacenamiento, Git y procesos hijo. Separar estas responsabilidades evita que la UI ejecute comandos arbitrarios directamente y permite transmitir estados y logs de forma controlada.

Alternativas consideradas: una SPA en navegador requeriría un servicio local adicional y permisos del sistema menos claros; una aplicación puramente de línea de comandos no satisface el objetivo de Kanban visual.

### Adaptadores por CLI con un contrato común

Codex y Claude se integrarán a través de adaptadores que traduzcan la configuración común a argumentos, formato de prompt, detección de sesión, reanudación y salida específicos de cada CLI. El dominio principal no dependerá de una sintaxis concreta de ninguna herramienta.

Alternativas consideradas: invocar ambas CLIs desde la UI con prompts ad hoc simplifica el inicio, pero imposibilita manejar correctamente diferencias de sesión, errores y capacidades.

### Ejecución controlada por una máquina de estados

Los tickets y ejecuciones tendrán estados persistidos. El servicio local será la única autoridad que inicia procesos, crea ramas y realiza transiciones automáticas entre En ejecución y Review. Las transiciones manuales de Kanban se validarán contra ese estado.

Alternativas consideradas: representar estados solo en la UI crearía inconsistencias al reiniciar o si el proceso termina mientras la ventana está cerrada.

### SQLite como almacenamiento local inicial

Los datos de tablero, tickets, ejecuciones, mensajes y resúmenes se almacenarán en una base local SQLite. Los logs de gran tamaño podrán guardarse como archivos locales referenciados por la base de datos.

Alternativas consideradas: archivos JSON son simples, pero hacen más frágiles las relaciones padre-hijo e historiales; una base remota contradice el objetivo local-first.

### El branch es creado antes de cada primera ejecución de ticket

El servicio verificará Git y creará una rama con identificador estable antes de lanzar el agente. Los seguimientos desde Review trabajan en la misma rama. La aplicación no realizará merge, push ni borrado de ramas.

Alternativas consideradas: pedirle al agente que cree la rama deja una garantía crítica sujeta a interpretación del prompt.

### Planes revisables antes de materializar subtareas

Un ticket de planificación genera un artefacto de plan en Review. Su aprobación crea un tablero hijo y las subtareas; el tablero principal conserva solo la tarjeta padre y su progreso agregado.

Alternativas consideradas: materializar subtareas inmediatamente impide corregir alcance antes de iniciar trabajo y sobrecarga el tablero principal.

## Risks / Trade-offs

- [Las CLIs cambian sus argumentos o semántica de sesión] → Encapsularlas en adaptadores, registrar versiones detectadas y ofrecer errores accionables.
- [Una ejecución deja cambios sin commit o el repositorio estaba sucio] → Mostrar el estado Git antes de iniciar y asociar al ticket el commit base y el diff resultante.
- [Un proceso queda colgado o termina con la app cerrada] → Persistir PID y estado de ejecución, verificarlo al iniciar y permitir detener o reintentar de forma explícita.
- [Los logs contienen información sensible] → Mantenerlos solo en almacenamiento local y hacer visible su ubicación y política de retención.
- [Dos tickets usan la misma carpeta a la vez] → Detectar ejecuciones activas por repositorio y bloquear o advertir sobre conflictos de worktree/rama antes de iniciar.

## Migration Plan

1. Inicializar la aplicación local y su esquema de almacenamiento versionado.
2. Crear la interfaz y el flujo de tickets antes de habilitar ejecución real.
3. Añadir el adaptador de Codex y validar el ciclo completo sobre un repositorio de prueba.
4. Añadir el adaptador de Claude y las capacidades de seguimiento por sesión.
5. Añadir planificación y sub-kanban, manteniendo migraciones de datos reversibles.

Para rollback, la aplicación deberá poder deshabilitar adaptadores o volver a una versión anterior del esquema mediante migraciones versionadas, sin alterar ramas ni repositorios gestionados.

## Open Questions

- Qué modelos y niveles de razonamiento concretos expone cada CLI instalada se resolverá mediante detección de capacidades y configuraciones disponibles durante la implementación.
