## MODIFIED Requirements

### Requirement: Flujo de estado Kanban
El sistema SHALL mostrar las columnas Backlog, To do, In Progress, Review y Done y SHALL registrar cada transición de un ticket. Un ticket en Done SHALL conservar su rama y sus datos de ejecución. Las únicas transiciones manuales permitidas SHALL ser Backlog a To do, y Review a Backlog o Done. Al entrar en To do, el sistema SHALL preparar la rama Git y encolar el ticket; SHALL avanzar automáticamente a In Progress solamente cuando el scheduler le asigne un cupo de ejecución. Al terminar la ejecución, SHALL avanzar automáticamente a Review.

#### Scenario: Preparar un ticket para la cola
- **WHEN** la persona mueve un ticket desde Backlog a To do
- **THEN** el sistema SHALL preparar su rama Git y conservar el ticket en To do hasta que exista un cupo de ejecución

#### Scenario: Preparar un ticket para ejecución
- **WHEN** la persona mueve un ticket desde Backlog a To do
- **THEN** el sistema SHALL preparar su rama y encolarlo antes de mostrarlo en In Progress

#### Scenario: Mostrar un ticket esperando
- **WHEN** un ticket está en To do y el límite de concurrencia ya está ocupado
- **THEN** el sistema SHALL mostrarlo como pendiente en la cola y no SHALL iniciarlo todavía

#### Scenario: Impedir una transición no válida
- **WHEN** la persona intenta mover un ticket entre columnas fuera de las transiciones permitidas
- **THEN** el sistema SHALL rechazar el movimiento y mantener el estado actual

#### Scenario: Completar una revisión
- **WHEN** la persona marca un ticket en Review como Done
- **THEN** el sistema SHALL moverlo a Done sin hacer merge ni borrar su rama Git
