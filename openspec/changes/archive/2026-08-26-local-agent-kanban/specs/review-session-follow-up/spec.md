## Purpose

Permite revisar resultados de agentes y solicitar iteraciones posteriores manteniendo el contexto verificable de la ejecución original.

## ADDED Requirements

### Requirement: Revisión de resultados
Un ticket en Review SHALL mostrar el informe de la ejecución, su configuración efectiva, rama, logs y cambios detectados. El informe SHALL incluir un resumen del trabajo realizado, validaciones ejecutadas, decisiones y pendientes o riesgos reportados.

#### Scenario: Consultar un ticket en Review
- **WHEN** la persona abre un ticket en Review
- **THEN** el sistema SHALL mostrar el informe y la información de ejecución correspondiente

### Requirement: Instrucciones de seguimiento
El sistema SHALL permitir enviar una instrucción de seguimiento desde Review para corregir, completar o validar trabajo. SHALL intentar reanudar la sesión original del agente; si su CLI no permite reanudarla, SHALL iniciar una nueva sesión con el historial y estado necesarios como contexto.

#### Scenario: Corregir trabajo desde Review
- **WHEN** la persona envía una instrucción de seguimiento en Review
- **THEN** el sistema SHALL iniciar una nueva iteración en la rama del ticket y registrar su relación con la ejecución anterior

#### Scenario: Finalizar una iteración de seguimiento
- **WHEN** el agente termina una instrucción de seguimiento
- **THEN** el ticket SHALL volver a Review con un informe actualizado y el historial preservado

