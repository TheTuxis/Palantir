## Context

Palantir usa comandos Tauri sobre una base SQLite y dispara procesos de Codex desde el flujo de transición. La nueva cola, preferencias y política atraviesan la interfaz, persistencia y ejecución. Véase `proposal.md` para la motivación y los deltas de especificación para el contrato observable.

## Goals / Non-Goals

**Goals:**
- Mantener el modo local existente y hacer que el límite de concurrencia se aplique de forma atómica.
- Centralizar preferencias para que la UI solo ofrezca configuraciones válidas y el backend las valide de nuevo.
- Crear un perfil remoto persistible sin introducir aún una API o worker remoto.

**Non-Goals:**
- No implementar sincronización, autenticación remota ni ejecución en servidor.
- No añadir prioridades manuales, reintentos automáticos ni planificación por cron.
- No modificar el historial de ejecuciones ya existente.

## Decisions

### Cola basada en tickets To do y registros de espera

To do seguirá siendo el estado visible de espera. Se añadirá una marca de ingreso a cola y un estado de bloqueo/reintento para determinar qué tickets son elegibles. El scheduler será una función central invocada después de encolar, finalizar, detener o reintentar una ejecución. Esta decisión conserva el modelo Kanban y evita agregar una columna visual adicional.

La alternativa de iniciar directamente desde el frontend se descarta porque no protege contra acciones simultáneas ni respeta una política común.

### Reserva atómica de cupos en SQLite

El scheduler contará ejecuciones `running`/`stopping`, seleccionará el siguiente ticket FIFO elegible y reservará su transición a ejecución dentro de una transacción antes de crear el proceso. La reserva evita que dos eventos disparen más procesos que el límite.

La alternativa de confiar en polling de la UI se descarta porque falla al reiniciar o con múltiples ventanas.

### Preferencias como configuración versionada local

Las preferencias se guardarán en SQLite con valores por defecto: modo Local, límite 1, Codex habilitado y un catálogo inicial de modelos/razonamientos. Las ejecuciones seguirán guardando un snapshot efectivo para preservar auditoría aunque cambie la política.

El perfil Remoto almacenará endpoint y estado de validación. Mientras no haya cliente API, el backend rechazará iniciar ejecuciones si el perfil Remoto está activo.

### Adaptador de cliente en frontend

La pantalla consumirá comandos de preferencias y capacidad en lugar de tener los modelos codificados como autoridad. Inicialmente seguirán siendo `invoke` de Tauri; su agrupación detrás de funciones de cliente prepara el posterior adaptador HTTP.

### Navegación ocultable y configuración modal

El control superior alternará una clase que oculta por completo la navegación y mantiene el control disponible en el borde de la aplicación, igual que Codex. La entrada Configuración permanecerá anclada al extremo inferior cuando la navegación esté visible. La configuración usará un modal para no desplazar el tablero ni mezclar ajustes globales con tickets.

## Risks / Trade-offs

- [El proceso no puede iniciarse tras reservar cupo] → revertir la reserva y registrar el fallo antes de volver a evaluar la cola.
- [Un ticket con política obsoleta bloquea la cola] → marcarlo como no elegible, mostrar el motivo y continuar con el siguiente.
- [Reinicio durante una ejecución] → las ejecuciones sin proceso recuperable se restaurarán como fallidas y requerirán reintento manual.
- [Perfil remoto puede sugerir una capacidad inexistente] → distinguir visualmente configuración guardada de conexión disponible y bloquear el inicio.

## Migration Plan

1. Crear tablas de preferencias y metadatos de cola con valores por defecto que preserven el comportamiento actual de una sola ejecución.
2. Migrar tickets existentes a una posición de cola basada en su marca temporal; no alterar tickets en ejecución o históricos.
3. Activar el scheduler al modificar la cola y verificar límite 1 como comportamiento por defecto.
4. Si es necesario revertir, mantener las preferencias y dejar el límite en 1; los tickets siguen siendo recuperables desde To do.
