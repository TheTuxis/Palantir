## Purpose

Gestiona los tickets preparados para ejecutar como una cola local predecible, respetando el límite de trabajo paralelo elegido por la persona.

## ADDED Requirements

### Requirement: Límite de ejecuciones concurrentes
El sistema SHALL permitir configurar un límite entero positivo de ejecuciones activas. El scheduler SHALL iniciar nuevos tickets solamente cuando la cantidad de ejecuciones activas sea menor a ese límite.

#### Scenario: Un único cupo de ejecución
- **WHEN** el límite configurado es 1 y existen cinco tickets preparados en To do
- **THEN** el sistema SHALL ejecutar como máximo uno y SHALL conservar los cuatro restantes en espera

#### Scenario: Liberar un cupo
- **WHEN** una ejecución activa termina o se detiene
- **THEN** el sistema SHALL evaluar la cola e iniciar el siguiente ticket elegible si existe un cupo libre

### Requirement: Orden FIFO de la cola
El sistema SHALL ordenar los tickets pendientes por el momento en que entraron a To do y SHALL seleccionar primero el ticket más antiguo que pueda ejecutarse.

#### Scenario: Mostrar la espera
- **WHEN** un ticket está en To do y no hay cupo disponible
- **THEN** el sistema SHALL conservarlo en To do e indicar que está en cola

### Requirement: Fallos requieren reintento manual
El sistema SHALL conservar una ejecución fallida y SHALL impedir que el scheduler la vuelva a iniciar automáticamente. La persona SHALL poder solicitar un reintento manual.

#### Scenario: Fallo de una tarea en cola
- **WHEN** una ejecución termina con error
- **THEN** el sistema SHALL registrar el fallo, conservar el ticket listo para reintento manual y continuar con el siguiente ticket elegible
