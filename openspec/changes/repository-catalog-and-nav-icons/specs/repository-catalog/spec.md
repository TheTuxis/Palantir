## Purpose

Permite registrar repositorios locales una vez y reutilizarlos de forma consistente al crear tickets.

## ADDED Requirements

### Requirement: Registro de repositorios locales
El sistema SHALL permitir dar de alta y editar repositorios desde la sección Repos con nombre, descripción opcional y ruta local. SHALL exigir nombre y ruta, y SHALL rechazar una ruta que ya pertenezca a otro repositorio registrado.

#### Scenario: Registrar un repositorio
- **WHEN** la persona guarda un repositorio con nombre y ruta válidos
- **THEN** el sistema SHALL conservarlo localmente y mostrarlo en la sección Repos

#### Scenario: Validar campos obligatorios
- **WHEN** la persona intenta guardar un repositorio sin nombre o sin ruta
- **THEN** el sistema SHALL impedir el alta e indicar los campos faltantes

#### Scenario: Evitar rutas duplicadas
- **WHEN** la persona intenta registrar una ruta local que ya fue guardada
- **THEN** el sistema SHALL rechazar el alta e indicar que la ruta ya está registrada

#### Scenario: Editar un repositorio
- **WHEN** la persona actualiza el nombre, la descripción o la ruta de un repositorio registrado
- **THEN** el sistema SHALL guardar los cambios y conservar las referencias de tickets al mismo repositorio

### Requirement: Referencia de repositorio en tickets
El sistema SHALL permitir seleccionar un repositorio registrado al crear o editar un ticket que no esté en ejecución. Los tickets SHALL usar la ruta del repositorio seleccionado para preparar y ejecutar el trabajo, sin solicitar la ruta manualmente en el formulario de ticket.

#### Scenario: Crear un ticket con repositorio registrado
- **WHEN** la persona crea un ticket y selecciona un repositorio existente
- **THEN** el sistema SHALL guardar la referencia al repositorio y mostrar su nombre y ruta en el detalle del ticket

#### Scenario: Conservar ejecución histórica tras cambiar un repositorio
- **WHEN** se edita un ticket después de una ejecución terminada y se selecciona otro repositorio
- **THEN** el historial SHALL conservar la ruta y configuración efectiva de la ejecución previa

#### Scenario: Migrar tickets existentes
- **WHEN** la aplicación se actualiza con tickets que sólo contienen una ruta de trabajo
- **THEN** el sistema SHALL conservar esos tickets y asociarlos a un registro local reutilizable sin perder su historial
