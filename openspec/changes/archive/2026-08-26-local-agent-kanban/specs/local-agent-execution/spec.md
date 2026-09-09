## Purpose

Ejecuta agentes de código locales con la configuración elegida y aísla el trabajo de cada ticket en una rama Git.

## ADDED Requirements

### Requirement: Ejecución mediante CLI local
El sistema SHALL lanzar únicamente la CLI local autenticada de Codex. SHALL pasarle el título, descripción y configuración efectiva del ticket, y SHALL informar cuando Codex no esté disponible o no pueda iniciar. Claude queda diferido fuera del alcance de esta entrega.

#### Scenario: Iniciar una ejecución de Codex
- **WHEN** la persona inicia un ticket en To do con Codex disponible
- **THEN** el sistema SHALL mover el ticket a En ejecución y registrar la sesión y los logs de la ejecución

#### Scenario: CLI no disponible
- **WHEN** la persona inicia un ticket cuyo agente no está instalado o autenticado
- **THEN** el sistema SHALL mantener el ticket fuera de En ejecución e informar el error sin modificar el repositorio

### Requirement: Aislamiento por rama Git
Antes de iniciar una ejecución, el sistema SHALL verificar que la carpeta del ticket sea un repositorio Git válido y SHALL crear o seleccionar una rama dedicada al ticket. El agente SHALL ejecutarse dentro de esa rama.

#### Scenario: Repositorio válido
- **WHEN** un ticket ejecutable apunta a un repositorio Git válido
- **THEN** el sistema SHALL iniciar al agente en una rama dedicada e identificar dicha rama en el ticket

#### Scenario: Carpeta sin repositorio Git
- **WHEN** un ticket ejecutable apunta a una carpeta que no es un repositorio Git
- **THEN** el sistema SHALL bloquear la ejecución e indicar que la carpeta debe ser un repositorio Git

### Requirement: Resultado de ejecución
Al terminar o fallar una ejecución, el sistema SHALL guardar el estado terminal, logs, configuración efectiva, cambios detectados y un resumen de resultado. Una ejecución terminada SHALL mover el ticket a Review; una ejecución fallida SHALL permitir reintentarla sin perder el historial.

#### Scenario: Ejecución finalizada
- **WHEN** el agente finaliza correctamente
- **THEN** el ticket SHALL moverse a Review con su informe y cambios asociados

#### Scenario: Ejecución fallida
- **WHEN** el proceso del agente termina con error
- **THEN** el sistema SHALL conservar los logs y ofrecer una acción de reintento
