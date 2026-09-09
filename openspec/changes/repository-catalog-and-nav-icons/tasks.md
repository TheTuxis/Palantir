## 1. Catálogo local de repositorios

- [x] 1.1 Agregar el esquema y migración SQLite para repositorios y la referencia opcional desde tickets, migrando rutas existentes sin perder historial.
- [x] 1.2 Implementar comandos y validaciones para listar, dar de alta y editar repositorios con nombre y ruta obligatorios, descripción opcional y rutas únicas.
- [x] 1.3 Agregar pruebas de persistencia, campos obligatorios, deduplicación de rutas y migración de tickets existentes.

## 2. Referencia de repositorio en tickets

- [x] 2.1 Actualizar creación, edición, detalle y ejecución de tickets para seleccionar y resolver un repositorio registrado.
- [x] 2.2 Reemplazar la entrada manual de ruta por un selector de repositorio y mostrar nombre, descripción y ruta en la sección Repos y en el detalle del ticket.
- [x] 2.3 Agregar pruebas para selección de repositorio, bloqueo sin referencia válida y preservación de snapshots de ejecución.

## 3. Navegación visual y verificación

- [x] 3.1 Reemplazar los iconos de Tablero, Sesiones y Repos por SVG lineales representativos, manteniendo texto y etiquetas accesibles.
- [ ] 3.2 Ejecutar pruebas de Rust, verificación de formato, compilación del frontend y validación manual del alta y selección de repositorio.
