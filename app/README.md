# Palantir

Aplicación de escritorio local para administrar tickets Kanban ejecutados por agentes de código. Los datos, los logs y la ejecución permanecen en la computadora de la persona usuaria.

## Requisitos

- Node.js 22+ y npm.
- Rust estable y Cargo (para Tauri).
- Git disponible en `PATH`.
- La CLI `codex`, `claude` y/o `opencode` instalada y autenticada localmente, según el/los agente(s) habilitado(s). Palantir usa `codex exec`, `claude --print` y `opencode run`; no recibe, almacena ni transmite claves, sesiones de cuenta o credenciales.

## Desarrollo y comprobación

```bash
cd app
npm install
npm run tauri dev

# comprobaciones separadas
npm run build
cd src-tauri && cargo test
```

## Flujo y permisos

Un ticket comienza en Backlog. Al moverlo a To do, Palantir verifica que la carpeta sea un repositorio Git, registra el commit base, crea o reutiliza una rama `palantir/...` y ejecuta el agente configurado (Codex, Claude u OpenCode) dentro de esa carpeta. La aplicación muestra una advertencia si el árbol de trabajo ya tenía cambios y bloquea ejecuciones simultáneas sobre la misma carpeta.

Palantir no hace `merge`, `push`, borrado de ramas ni configuración de credenciales. El agente recibe los permisos que determine su CLI local; revisá sus argumentos y autenticación antes de iniciar trabajo sobre un repositorio sensible.

Las ejecuciones terminadas pasan a Review con su informe, logs, cambios detectados y configuración inmutable. Desde Review se puede enviar un seguimiento: se intenta reanudar la sesión del agente del ticket y, si no hay sesión reutilizable, se inicia una nueva sesión con la instrucción y contexto del ticket en la misma rama.

Los tickets de planificación producen un plan estructurado en Review. Aprobarlo crea las subtareas en un sub-kanban independiente, sin duplicarlas en el tablero principal.

## Datos y límites locales

La base SQLite se guarda en el directorio de datos de la aplicación de Tauri. Al iniciar, una ejecución que quedó en curso se marca como interrumpida y su ticket vuelve a To do para permitir un reintento explícito. Los logs se conservan localmente y el inicio retiene por defecto los 2.000 registros más recientes; el backend expone una preferencia de retención para reducir o ampliar ese límite.

Los logs y los diffs pueden contener información sensible de los repositorios. No se sincronizan ni se envían desde Palantir, pero deben protegerse con las mismas medidas que el resto de los datos locales.

## Servidor remoto (`palantir-server`)

`server/` es una instancia headless de Palantir: mismo tablero, cola y ejecución de agentes, pero sin interfaz y con su propia base SQLite, pensada para correr en otra máquina (la que tenga los repositorios y las CLIs instaladas). Se levanta y se prueba por separado; ver `server/README.md`.

Desde la app, Configuración → Perfil → Remoto permite guardar el endpoint y el token de un `palantir-server` y verificar la conexión con "Probar conexión". Con el perfil en Remoto, el tablero, repos, cola y sesiones pasan a operar contra esa instancia (vía el comando Tauri `remote_request`, que evita el bloqueo de CORS que tendría un `fetch` directo desde el frontend) en vez de la base SQLite local. Todavía no soporta ahí: tickets de planificación/sub-kanban, archivado, reintentar, detener ejecución ni seguimiento — esas acciones muestran un aviso de "no disponible en modo remoto" mientras ese perfil esté activo.
