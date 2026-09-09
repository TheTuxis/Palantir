## Purpose

Hace reconocibles y accesibles las secciones principales de la navegación mediante iconos visuales consistentes.

## ADDED Requirements

### Requirement: Iconos representativos de navegación
El sistema SHALL mostrar un icono vectorial lineal representativo para Tablero, Sesiones y Repos en la navegación principal. Cada icono SHALL conservar una etiqueta textual o accesible que identifique su sección.

#### Scenario: Identificar una sección desde la navegación
- **WHEN** la persona visualiza la navegación principal
- **THEN** el sistema SHALL presentar un icono distinguible y la etiqueta de Tablero, Sesiones o Repos para cada acceso

#### Scenario: Usar la navegación con tecnologías asistivas
- **WHEN** una tecnología asistiva inspecciona un acceso de navegación
- **THEN** el sistema SHALL exponer el nombre de la sección sin depender exclusivamente del emoji
