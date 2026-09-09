## MODIFIED Requirements

### Requirement: Ejecución mediante CLI local
El sistema SHALL lanzar únicamente la CLI local autenticada de Codex cuando el scheduler asigne un cupo y la configuración del ticket respete la política de agentes vigente. SHALL pasarle el título, descripción y configuración efectiva del ticket, y SHALL informar cuando Codex no esté disponible o no pueda iniciar. Claude queda diferido fuera del alcance de esta entrega.

#### Scenario: Iniciar una ejecución de Codex desde la cola
- **WHEN** un ticket en To do recibe un cupo de ejecución y Codex está disponible
- **THEN** el sistema SHALL mover el ticket a En ejecución y registrar la sesión y los logs de la ejecución

#### Scenario: Iniciar una ejecución de Codex
- **WHEN** un ticket en To do recibe un cupo de ejecución y Codex está disponible
- **THEN** el sistema SHALL mover el ticket a En ejecución y registrar la sesión y los logs de la ejecución

#### Scenario: CLI no disponible
- **WHEN** el scheduler intenta iniciar un ticket cuyo agente no está instalado o autenticado
- **THEN** el sistema SHALL mantener el ticket fuera de En ejecución e informar el error sin modificar el repositorio

#### Scenario: Configuración no permitida
- **WHEN** el ticket solicita un agente, modelo o razonamiento fuera de la política vigente
- **THEN** el sistema SHALL impedir su inicio y conservar el ticket fuera de En ejecución

### Requirement: Resultado de ejecución
Al terminar o fallar una ejecución, el sistema SHALL guardar el estado terminal, logs, configuración efectiva, cambios detectados y un resumen de resultado. Una ejecución terminada SHALL mover el ticket a Review; una ejecución fallida SHALL conservar el ticket en To do, impedir un reintento automático y permitir un reintento manual sin perder el historial.

#### Scenario: Ejecución finalizada
- **WHEN** el agente finaliza correctamente
- **THEN** el ticket SHALL moverse a Review con su informe y cambios asociados

#### Scenario: Ejecución fallida
- **WHEN** el proceso del agente termina con error
- **THEN** el sistema SHALL conservar los logs, marcarlo como pendiente de reintento manual y continuar con otros tickets elegibles
