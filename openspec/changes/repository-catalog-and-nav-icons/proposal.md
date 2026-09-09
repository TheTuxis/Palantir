## Why

Repetir una ruta local en cada ticket es propenso a errores y vuelve difícil reutilizar un mismo repositorio. Además, los iconos actuales de navegación no comunican claramente las secciones principales de la aplicación.

## What Changes

- Incorporar un catálogo local de repositorios accesible desde la sección Repos, con alta y edición de nombre, descripción opcional y ruta local.
- Exigir únicamente nombre y ruta al registrar un repositorio, y validar que cada ruta sea única.
- Permitir que los tickets seleccionen un repositorio existente; la ruta de trabajo se obtiene de esa referencia, sin volver a cargarla en cada ticket.
- Mostrar información del repositorio referenciado en el ticket y preservar la configuración efectiva de ejecuciones previas.
- Reemplazar los tres iconos de navegación por iconos vectoriales lineales, representativos y accesibles para Tablero, Sesiones y Repos.

## Capabilities

### New Capabilities

- `repository-catalog`: Registro local reutilizable de repositorios y referencia de tickets a su repositorio.
- `navigation-visuals`: Iconos de navegación representativos y accesibles para las secciones principales.

### Modified Capabilities

- Ninguna.

## Impact

- Afecta el esquema SQLite, comandos Tauri y formularios React de Repos y tickets.
- Migra la asociación actual de tickets basada en ruta a una referencia de repositorio, manteniendo los datos e historiales existentes.
- No añade dependencias externas: se usarán SVG inline con etiquetas accesibles.
