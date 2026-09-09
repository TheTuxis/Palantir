## Why

Trabajar con agentes de código locales hoy requiere coordinar manualmente prompts, modelos, niveles de razonamiento, ramas de Git y seguimiento de resultados. Palantir será una aplicación local que convierta ese proceso en un flujo Kanban visible, reproducible y revisable sin desplazar el control del repositorio ni las credenciales fuera de las CLIs ya configuradas.

## What Changes

- Incorporar Palantir, una aplicación de escritorio local con una interfaz Kanban moderna y liviana para administrar tickets de trabajo asistido por agentes.
- Permitir crear tickets con título, descripción, tipo, carpeta local, modelo y nivel de razonamiento para Codex.
- Ejecutar la CLI local autenticada de Codex dentro de la carpeta configurada; no gestionar claves ni cuentas desde la aplicación. La integración de Claude queda diferida de esta primera entrega.
- Validar que la carpeta de un ticket ejecutable sea un repositorio Git y crear una rama aislada antes de lanzar el agente.
- Llevar tickets por Backlog, To do, En ejecución, Review y Done; marcar Done sin hacer merge ni eliminar la rama del ticket.
- Conservar ejecución, logs, configuración efectiva, rama, cambios detectados y un informe final estructurado en Review.
- Permitir conversar desde Review con la sesión de agente que resolvió el ticket para solicitar correcciones, completar pendientes o repetir validaciones, devolviendo el ticket a Review al finalizar cada iteración.
- Incorporar tickets de planificación que produzcan un plan para aprobación en Review y, solo tras aprobarlo, creen un sub-kanban de subtareas independiente del tablero padre.

## Capabilities

### New Capabilities

- `kanban-ticket-management`: Crear, organizar y transicionar tickets simples y de planificación en tableros Kanban locales.
- `local-agent-execution`: Ejecutar y supervisar sesiones de Codex y Claude con configuración de ticket y aislamiento por rama Git.
- `review-session-follow-up`: Revisar los resultados de una ejecución y enviar instrucciones de seguimiento a su sesión de agente.
- `sub-kanban-planning`: Generar, aprobar y gestionar subtareas en un sub-kanban asociado a un ticket de planificación.
- `local-workspace-persistence`: Persistir localmente tableros, tickets, configuración, ejecuciones, logs e informes.

### Modified Capabilities

- Ninguna: el proyecto no contiene capacidades existentes.

## Impact

- Nuevo proyecto de aplicación de escritorio local, interfaz gráfica, almacenamiento local y proceso de backend para iniciar y controlar procesos CLI.
- Integración con instalaciones existentes de las CLIs `codex` y `claude`, y con repositorios Git locales elegidos por la persona usuaria.
- Se requerirá una estrategia de adaptadores por CLI para normalizar modelos, razonamiento, creación/reanudación de sesión y captura de resultados.
