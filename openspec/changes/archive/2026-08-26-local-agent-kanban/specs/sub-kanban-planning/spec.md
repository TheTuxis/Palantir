## Purpose

Convierte iniciativas complejas en planes revisables y subtareas organizadas en un tablero hijo independiente.

## ADDED Requirements

### Requirement: Generación de plan para tickets de planificación
Al ejecutar un ticket de planificación, el sistema SHALL solicitar al agente un análisis y un plan de trabajo. El resultado SHALL llegar a Review antes de crear cualquier subtarea.

#### Scenario: Plan pendiente de aprobación
- **WHEN** el agente termina el análisis de un ticket de planificación
- **THEN** el ticket SHALL quedar en Review con el plan propuesto y sin subtareas creadas

### Requirement: Aprobación y sub-kanban
El sistema SHALL permitir aprobar el plan de un ticket de planificación. Tras la aprobación, SHALL crear las subtareas propuestas en un sub-kanban del ticket padre; las subtareas SHALL mostrarse dentro del sub-kanban y no como tarjetas del tablero padre.

#### Scenario: Aprobar un plan
- **WHEN** la persona aprueba un plan en Review
- **THEN** el sistema SHALL crear el sub-kanban y sus subtareas con el estado inicial definido

#### Scenario: Consultar una iniciativa padre
- **WHEN** la persona visualiza el ticket padre tras aprobar el plan
- **THEN** el sistema SHALL mostrar el progreso agregado y un acceso al sub-kanban sin duplicar sus subtareas en el tablero padre

