## Purpose

Centraliza la configuración operativa de Palantir para que la persona pueda controlar el comportamiento de ejecución y preparar perfiles de conexión sin editar cada ticket.

## ADDED Requirements

### Requirement: Ventana de configuración accesible
El sistema SHALL ofrecer una ventana de Configuración accesible desde el extremo inferior de la navegación. La barra lateral SHALL permitir alternar entre estar visible y oculta completamente mediante un control superior persistente, como la navegación de Codex.

#### Scenario: Abrir configuración desde la navegación
- **WHEN** la persona selecciona Configuración en la navegación
- **THEN** el sistema SHALL abrir una ventana con los ajustes operativos disponibles

#### Scenario: Ocultar y restaurar la navegación
- **WHEN** la persona activa el control superior de la barra lateral
- **THEN** el sistema SHALL ocultar completamente la navegación o restaurarla sin perder la sección activa, y SHALL conservar etiquetas accesibles para el control

### Requirement: Política de ejecución administrada centralmente
El sistema SHALL permitir configurar los agentes habilitados, modelos permitidos y niveles de razonamiento permitidos. El sistema SHALL mostrar únicamente combinaciones permitidas y SHALL rechazar una ejecución que no respete la política vigente.

#### Scenario: Modelo no permitido
- **WHEN** un ticket solicita un modelo que dejó de estar permitido
- **THEN** el sistema SHALL impedir el inicio e indicar que debe elegirse una configuración permitida

### Requirement: Perfiles Local y Remoto
El sistema SHALL permitir guardar y seleccionar un perfil Local o Remoto. El perfil Remoto SHALL almacenar los datos de conexión necesarios para una API futura y SHALL indicar claramente que la ejecución remota no está disponible mientras no exista un servidor configurado.

#### Scenario: Seleccionar el perfil remoto sin servidor disponible
- **WHEN** la persona selecciona un perfil Remoto sin una conexión validada
- **THEN** el sistema SHALL conservar la selección y SHALL informar que no se pueden iniciar ejecuciones remotas todavía
