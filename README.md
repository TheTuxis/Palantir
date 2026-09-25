# Palantir

Palantir es una aplicación de escritorio local para gestionar tickets Kanban con agentes de código, como Codex, Claude y OpenCode. Cada ticket conserva su configuración, trabaja sobre una rama Git dedicada y finaliza en Review, sin hacer merges automáticos.

## Requisitos

- Node.js 22 o superior
- Rust estable y Cargo
- Git
- Codex, Claude y/o OpenCode instalados y autenticados localmente

## Ejecutar en desarrollo

```bash
cd app
npm install
npm run tauri dev
```

## Stack

- Tauri 2 + Rust: aplicación desktop, procesos locales, Git y SQLite.
- React + TypeScript: tablero, tickets y Review.
- SQLite: persistencia local de tickets, ejecuciones, logs e informes.

## Alcance actual

La primera página muestra el tablero Kanban, un panel de Review y creación rápida de tickets. La ejecución real de agentes, creación de ramas y persistencia se completan en las próximas tareas del cambio OpenSpec `local-agent-kanban`.
