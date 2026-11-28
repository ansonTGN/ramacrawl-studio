# Panel web de control

## Inicio

```bash
cargo run --release -- serve
```

El navegador se abre en `http://127.0.0.1:8090/`.

El inicio usa `config/example.yaml`, incluido en el repositorio. Para trabajar
con una copia local, ejecuta `cp config/example.yaml config/local.yaml` y añade
`--config config/local.yaml` al comando. `make serve` y `scripts/run-web.sh`
también usan el ejemplo por defecto; aceptan `CONFIG=config/local.yaml` y
`config/local.yaml` como argumento, respectivamente.

## Flujo de trabajo

1. Edita las URL semilla y la lista de dominios autorizados.
2. Revisa `robots.txt`, las rutas denegadas y los filtros regex.
3. Ajusta límites y concurrencia.
4. Edita las reglas de extracción como un array JSON.
5. Pulsa **Validar** para ejecutar la validación del backend.
6. Pulsa **Guardar** para reemplazar el YAML de forma atómica.
7. Pulsa **Iniciar escaneo**.
8. Consulta el progreso y abre **Ver resultados** al finalizar.

El backend comprueba la sintaxis CSS de los selectores de enlaces, elementos y
campos, y la sintaxis JSONPath de las reglas JSON/NDJSON y WebSocket antes de
guardar o iniciar un escaneo. El JSONPath admitido incluye campos (`$.items`),
índices (`$[0]`), comodines (`$[*]`) y campos entre comillas (`$['display-name']`).
Usa la notación entre corchetes para nombres con espacios o signos; las
secuencias de escape y los caracteres de control no están admitidos.

## API local

| Método | Ruta | Función |
|---|---|---|
| GET | `/api/config` | Carga la configuración activa |
| PUT | `/api/config` | Valida y guarda la configuración |
| POST | `/api/validate` | Valida sin guardar |
| POST | `/api/scan` | Guarda e inicia un escaneo |
| POST | `/api/scan/cancel` | Cancela el escaneo activo |
| GET | `/api/status` | Devuelve estado, progreso y resumen |
| GET | `/api/history?limit=20` | Historial persistente de ejecuciones completadas; límite entre 1 y 100 |
| GET | `/dashboard` | Muestra los resultados generados |

## Monitorización incremental

Activa **Monitorización incremental** en Almacenamiento. El panel desactiva
**Inicio limpio** para conservar la base de comparación; la API rechaza esa
combinación antes de guardar o limpiar archivos. Los contadores indican páginas
nuevas, modificadas y sin cambios. El historial de Almacenamiento se actualiza
automáticamente mientras esa sección está abierta, incluso si `watch` realiza
las ejecuciones desde otro proceso. Evita iniciar dos escritores sobre la misma
base de datos.

Para escaneos periódicos usa `watch --config config/local.yaml --interval-secs 3600`
con `crawler.incremental: true`. El proceso reutiliza el cliente Rama, conserva
los límites por host entre ciclos y espera el intervalo tras completar cada
ejecución. Reinícialo para aplicar cambios de configuración. Solo las ejecuciones
completadas aparecen en el historial; las páginas ya persistidas se conservan
si se cancela una ejecución.

## Seguridad

- El bind predeterminado es `127.0.0.1:8090`.
- Escuchar en otra interfaz requiere `--allow-remote`.
- La API acepta JSON con un máximo de 1 MiB.
- El navegador y el backend validan la configuración.
- El frontend aplica una política CSP y no carga librerías externas.
- Las configuraciones pueden contener secretos; protege el YAML y evita subirlo a Git.
