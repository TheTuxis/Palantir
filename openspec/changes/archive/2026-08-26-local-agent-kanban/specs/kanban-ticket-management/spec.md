## Purpose

Permite organizar el trabajo asistido por agentes como tickets locales y visualizar su avance en un tablero Kanban.

## ADDED Requirements

### Requirement: Creación y configuración de tickets
El sistema SHALL permitir crear tickets simples y tickets de planificación con título, descripción, agente, modelo, nivel de razonamiento y carpeta local. Los campos de ejecución SHALL poder editarse mientras el ticket no esté en ejecución.

#### Scenario: Crear un ticket simple
- **WHEN** la persona crea un ticket simple con los campos obligatorios
- **THEN** el sistema SHALL guardarlo en Backlog con su configuración visible

#### Scenario: Validar datos de ejecución
- **WHEN** la persona intenta iniciar un ticket sin título, carpeta, agente o modelo
- **THEN** el sistema SHALL impedir la ejecución e indicar los campos faltantes

### Requirement: Flujo de estado Kanban
El sistema SHALL mostrar las columnas Backlog, To do, In Progress, Review y Done y SHALL registrar cada transición de un ticket. Un ticket en Done SHALL conservar su rama y sus datos de ejecución. Las únicas transiciones manuales permitidas SHALL ser Backlog a To do, y Review a Backlog o Done. Al entrar en To do, el sistema SHALL preparar la rama Git y avanzar automáticamente a In Progress al lanzar el agente; al terminar la ejecución, SHALL avanzar automáticamente a Review.

#### Scenario: Preparar un ticket para ejecución
- **WHEN** la persona mueve un ticket desde Backlog a To do
- **THEN** el sistema SHALL preparar su rama y lanzar al agente antes de mostrarlo en In Progress

#### Scenario: Impedir una transición no válida
- **WHEN** la persona intenta mover un ticket entre columnas fuera de las transiciones permitidas
- **THEN** el sistema SHALL rechazar el movimiento y mantener el estado actual

#### Scenario: Completar una revisión
- **WHEN** la persona marca un ticket en Review como Done
- **THEN** el sistema SHALL moverlo a Done sin hacer merge ni borrar su rama Git
