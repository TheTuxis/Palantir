## Why

Palantir inicia hoy cada ticket al entrar en To do y concentra ajustes de ejecución en cada ticket. Esto impide controlar el paralelismo de forma predecible y no ofrece un lugar único para administrar la política de agentes ni preparar el uso local o remoto de la aplicación.

## What Changes

- Introducir una cola FIFO para tickets en To do con un límite de ejecuciones concurrentes configurable; los tickets fallidos conservarán el historial y esperarán un reintento manual.
- Añadir una ventana de Configuración accesible desde el extremo inferior de la navegación, junto con una barra lateral que pueda ocultarse o mostrarse completamente como en Codex.
- Persistir preferencias locales para el límite de concurrencia y la política de agentes/modelos/razonamiento permitidos.
- Añadir la selección de perfil Local o Remoto y los datos de conexión remota como configuración preparada para una API futura; esta entrega no ejecuta tickets contra un servidor remoto.
- Hacer que el backend aplique la política configurada al iniciar ejecuciones, sin confiar en opciones manipulables desde la interfaz.

## Capabilities

### New Capabilities

- `execution-queue`: cola FIFO y planificación de ejecuciones con un límite de concurrencia persistente.
- `application-preferences`: configuración persistida de ejecución, política de agentes y perfiles Local/Remoto desde una interfaz accesible.

### Modified Capabilities

- `kanban-ticket-management`: To do pasa a representar una cola preparada para ejecución y expone su posición/estado de espera.
- `local-agent-execution`: el inicio de agentes respeta límite de concurrencia y política permitida; los fallos no se reintentan automáticamente.
- `local-workspace-persistence`: se persisten preferencias locales, política de ejecución, perfil seleccionado y metadatos de cola.

## Impact

- Afecta `app/src/App.tsx` y estilos de navegación/configuración.
- Afecta la base SQLite, comandos Tauri y scheduler en `app/src-tauri/src/lib.rs`.
- No añade dependencias externas ni habilita todavía una API remota o workers de servidor.
