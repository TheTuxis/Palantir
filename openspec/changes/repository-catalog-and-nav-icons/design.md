## Context

La aplicación guarda actualmente la ruta de trabajo directamente en cada ticket y la sección Repos se deriva de esos tickets. El cambio introduce una entidad local reutilizable y afecta persistencia, comandos de backend y formularios de React.

## Goals / Non-Goals

**Goals:**

- Centralizar nombre, descripción y ruta de los repositorios locales.
- Mantener compatibles los tickets e historiales que ya contienen una ruta.
- Sustituir los iconos de navegación sin añadir dependencias visuales.

**Non-Goals:**

- No se inspeccionará Git al momento de registrar un repositorio; esa validación continúa al ejecutar un ticket.
- No se borrarán repositorios ni se sincronizarán entre dispositivos en este cambio.
- No se cambiarán las reglas de ramas, permisos o ejecución de agentes.

## Decisions

### Catálogo SQLite con referencia opcional durante migración

Se agregará una tabla de repositorios con identificador, nombre, descripción, ruta y marcas temporales; los tickets guardarán `repository_id`. La ruta existente permanecerá disponible como dato compatible e histórico, y la migración creará un repositorio por cada ruta distinta de tickets existentes antes de enlazarlos.

La edición actualiza el registro compartido. Los tickets conservan la referencia y tomarán la ruta nueva en ejecuciones futuras; los snapshots ya guardados no se modifican.

Alternativa considerada: reemplazar la ruta directamente y hacer obligatorio el nuevo campo en una sola migración. Se descarta porque podría dejar tickets históricos sin una asociación recuperable.

### Formulario de tickets basado en selección

El formulario cargará los repositorios disponibles y exigirá seleccionar uno. La vista de detalle mostrará nombre y ruta resuelta. La instantánea de ejecución seguirá almacenando la ruta concreta para que los cambios posteriores al catálogo no alteren el historial.

Alternativa considerada: permitir introducir una ruta libre junto al selector. Se descarta para evitar volver a introducir duplicación y referencias inconsistentes.

### SVG lineales con texto accesible

Se usarán iconos SVG inline de trazo fino (tablero, sesiones y repositorios) dentro de los accesos existentes, manteniendo el texto visible y un nombre accesible. La navegación conservará la escala compacta actual, con una única familia visual de líneas redondeadas.

Alternativa considerada: incorporar una biblioteca de iconos o usar emojis. Se descarta porque los tres iconos no justifican otra dependencia y los emojis no conservan una apariencia consistente entre sistemas.

## Risks / Trade-offs

- [Rutas históricas que difieren sólo por normalización] → Normalizar y comparar rutas durante la migración, manteniendo la ruta original para ejecución.
- [Repositorio eliminado o movido después de registrarlo] → Informar el error de Git al iniciar el ticket; no eliminar la referencia ni el historial.
- [Un ticket sin asociación tras una migración incompleta] → Mostrar un estado de selección requerida y bloquear la ejecución hasta asignar un repositorio.

## Migration Plan

1. Crear la tabla de repositorios y la referencia opcional de ticket.
2. Generar registros por las rutas distintas existentes y enlazar tickets conservando sus campos actuales.
3. Cambiar formularios y comandos para usar el identificador seleccionado.
4. Verificar que los snapshots existentes siguen mostrando su ruta original.

El rollback conserva la ruta histórica en tickets, por lo que una versión anterior puede seguir operando sobre los datos existentes.
