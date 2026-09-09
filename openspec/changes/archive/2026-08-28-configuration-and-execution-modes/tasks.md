## 1. Persistencia de preferencias y políticas

- [x] 1.1 Crear la migración SQLite para preferencias de aplicación y metadatos de cola, incluyendo valores iniciales compatibles con los tickets existentes.
- [x] 1.2 Implementar comandos Rust para leer y actualizar el límite de concurrencia, catálogo de agentes/modelos/razonamiento y perfil Local/Remoto.
- [x] 1.3 Validar en el backend límites positivos y políticas permitidas, y bloquear ejecuciones con perfil remoto hasta que exista el transporte remoto.
- [x] 1.4 Agregar pruebas de migración, valores por defecto, validación de preferencias y aplicación de política.

## 2. Cola y planificador de ejecuciones

- [x] 2.1 Cambiar el pasaje de Backlog a To do para preparar la rama y encolar el ticket, sin iniciar Codex directamente.
- [x] 2.2 Implementar el planificador FIFO con reserva atómica de cupos según el límite de concurrencia persistido.
- [x] 2.3 Activar el planificador al encolar, finalizar, detener o reintentar una ejecución, liberando el cupo de forma segura si el inicio falla.
- [x] 2.4 Mantener los tickets fallidos en To do esperando un reintento manual y permitir que los tickets posteriores elegibles continúen.
- [x] 2.5 Cubrir con pruebas el límite uno, el orden FIFO, la liberación de cupos y el comportamiento ante fallos y reinicios.

## 3. Configuración y experiencia de navegación

- [x] 3.1 Incorporar en el cliente frontend los tipos y comandos de preferencias, política de ejecución y estado/posición de cola.
- [x] 3.2 Crear la ventana modal de Configuración accesible desde la zona inferior de la navegación, con concurrencia, agentes/modelos/razonamiento y perfiles Local/Remoto.
- [x] 3.3 Mostrar claramente que Remoto es una configuración preparada pero no ejecutable aún, sin mezclarlo con el modo Local operativo.
- [x] 3.4 Reemplazar el menú compacto por un control superior persistente que oculte o muestre completamente la barra lateral, preservando la sección activa.
- [x] 3.5 Exponer en To do el estado y la posición de cola, y presentar errores de política de manera comprensible.

## 4. Verificación

- [x] 4.1 Ejecutar formato y pruebas Rust, además del build del frontend.
- [x] 4.2 Verificar manualmente la configuración, ocultar/mostrar navegación, ejecución FIFO con límite uno, fallo/reintento y el bloqueo del perfil Remoto.
