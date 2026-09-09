# palantir-server

Instancia headless de Palantir: repositorios, tickets, cola de ejecución y agentes (Codex/Claude) por HTTP, con su propia base SQLite. Pensada para correr en la máquina que tenga los repositorios y las CLIs instaladas, y que la app de escritorio (o cualquier cliente HTTP) pueda usar como perfil remoto.

No portado todavía desde la app de escritorio: tickets de planificación/sub-kanban, archivado, seguimiento/reanudar sesión, reintentar y detener ejecución.

## Ejecutar

```bash
cd app/server
cargo run -- --port 8787 --db ./palantir-server.sqlite3 --token "elegí-un-token-largo"
```

Si no pasás `--token` (ni la variable de entorno `PALANTIR_TOKEN`), se genera uno al azar y se imprime en el arranque — usalo así, no lo pierdas.

Todos los endpoints excepto `GET /health` requieren `Authorization: Bearer <token>`.

## Endpoints

| Método | Ruta | Descripción |
| --- | --- | --- |
| GET | `/health` | Chequeo de vida, sin auth. |
| GET | `/repositories` | Listar repositorios. |
| POST | `/repositories` | Crear (`name`, `description`, `path`). |
| PUT | `/repositories/:id` | Editar. |
| DELETE | `/repositories/:id` | Borrar (falla si tiene tickets asociados). |
| GET | `/tickets` | Listar tickets. |
| POST | `/tickets` | Crear (`title`, `description`, `agent`, `model`, `reasoning`, `repositoryId`). |
| PUT | `/tickets/:id` | Editar. |
| DELETE | `/tickets/:id` | Borrar. |
| POST | `/tickets/:id/move` | Mover (`status`: `todo`\|`backlog`\|`done`). Backlog→To do prepara la rama Git y encola la ejecución. |
| GET | `/queue` | Estado de la cola. |
| GET | `/sessions` | Historial de ejecuciones. |
| GET`/PUT` | `/preferences` | Límite de concurrencia y agentes/modelos permitidos. |

## Notas

- No hace `merge`, `push` ni borra ramas — igual que la app de escritorio.
- Al arrancar, marca como `todo` (con reintento pendiente) cualquier ticket que haya quedado `running` de una corrida anterior interrumpida.
- Corré `cargo test` en este directorio para la suite de pruebas.
