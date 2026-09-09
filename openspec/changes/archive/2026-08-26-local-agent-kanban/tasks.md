## 1. Inicialización de aplicación y persistencia

- [x] 1.1 Inicializar la aplicación de escritorio local y su estructura de interfaz y backend.
- [x] 1.2 Definir el esquema SQLite y las migraciones para tableros, tickets, ejecuciones, mensajes, ramas, informes y logs.
- [x] 1.3 Implementar repositorios locales y restauración del tablero e historial al reiniciar.
- [x] 1.4 Agregar pruebas para persistencia de tickets y para instantáneas inmutables de configuración de ejecución.

## 2. Dominio Kanban e interfaz de tickets

- [x] 2.1 Implementar el modelo de ticket simple y de planificación, sus validaciones y transiciones de estado.
- [x] 2.2 Construir la interfaz Kanban con columnas Backlog, To do, En ejecución, Review y Done.
- [x] 2.3 Crear el formulario y detalle de ticket con título, descripción, carpeta, agente, modelo y nivel de razonamiento.
- [x] 2.4 Implementar el marcado manual a Done sin ejecutar operaciones de merge, push o borrado de ramas.
- [x] 2.5 Agregar pruebas de flujo y validación para creación y transición de tickets.

## 3. Preparación Git y ejecución de agentes

- [x] 3.1 Implementar la inspección de repositorios Git y el registro del commit base y estado de trabajo antes de ejecutar.
- [x] 3.2 Implementar creación o selección determinista de la rama dedicada del ticket y bloquear carpetas que no sean repositorios Git.
- [x] 3.3 Definir el contrato de adaptador de agente para detección, lanzamiento, captura de sesión, logs, resultado y reanudación.
- [x] 3.4 Implementar el adaptador de Codex usando la CLI local autenticada y probar una ejecución de extremo a extremo en un repositorio de prueba.
- [x] 3.5 Diferir el adaptador de Claude fuera del alcance de la primera entrega; Codex será el único agente disponible.
- [x] 3.6 Implementar supervisión de procesos, detención, detección de fallos y reintento conservando el historial.
- [x] 3.7 Agregar pruebas para errores de CLI, repositorio inválido, creación de rama y finalización de ejecución.

## 4. Review e iteraciones de seguimiento

- [x] 4.1 Persistir y presentar logs, configuración efectiva, rama, cambios detectados e informe estructurado al finalizar una ejecución.
- [x] 4.2 Implementar el panel de Review con resumen, validaciones, decisiones, riesgos y pendientes.
- [x] 4.3 Implementar mensajes de seguimiento desde Review que reanuden la sesión cuando el adaptador lo soporte o creen una sesión contextualizada como alternativa.
- [x] 4.4 Conectar la finalización de cada seguimiento con una nueva versión de informe en Review.
- [x] 4.5 Agregar pruebas de historial, reanudación/fallback y transición de ejecución a Review.

## 5. Planificación y sub-kanban

- [x] 5.1 Implementar el modo de ejecución de planificación y el formato de plan estructurado para tickets padre.
- [x] 5.2 Mostrar el plan en Review sin crear subtareas antes de la aprobación explícita.
- [x] 5.3 Implementar la aprobación de plan, creación de sub-kanban y materialización de sus subtareas.
- [x] 5.4 Construir la navegación al sub-kanban y el progreso agregado en la tarjeta padre sin duplicar sus subtareas en el tablero principal.
- [x] 5.5 Agregar pruebas de aprobación, aislamiento visual de subtareas y progreso agregado.

## 6. Endurecimiento y verificación final

- [x] 6.1 Mostrar advertencias para repositorios con cambios previos y conflictos de ejecuciones simultáneas en una misma carpeta.
- [x] 6.2 Implementar recuperación al iniciar para ejecuciones interrumpidas y retención local configurable de logs.
- [x] 6.3 Ejecutar la suite de pruebas, verificación de formato y un flujo manual completo con tickets simple y de planificación.
- [x] 6.4 Documentar requisitos de instalación de Codex, Claude y Git, además del modelo de permisos y límites de la aplicación.
