## Purpose

Conserva localmente la información necesaria para reabrir, auditar y continuar el trabajo de los tableros y agentes.

## Requirements

### Requirement: Persistencia local del espacio de trabajo
El sistema SHALL persistir localmente tableros, tickets, configuraciones, transiciones, ramas, ejecuciones, sesiones, logs e informes. SHALL persistir también las preferencias de ejecución, la política de agentes/modelos, el perfil Local o Remoto seleccionado y los metadatos necesarios para ordenar la cola. Al reiniciar la aplicación, SHALL restaurar el estado de los tickets, su historial y esas preferencias.

#### Scenario: Reabrir la aplicación
- **WHEN** la persona reinicia la aplicación después de registrar tickets, ejecuciones y preferencias
- **THEN** el sistema SHALL mostrar el tablero, los historiales y la configuración previamente almacenada

### Requirement: Preservación de configuración efectiva
El sistema SHALL conservar una copia inmutable de la configuración efectiva por ejecución, incluyendo agente, modelo, razonamiento, carpeta y rama, aunque el ticket se edite posteriormente.

#### Scenario: Editar un ticket tras una ejecución
- **WHEN** la persona modifica la configuración de un ticket luego de una ejecución finalizada
- **THEN** el historial SHALL seguir mostrando la configuración original de esa ejecución
