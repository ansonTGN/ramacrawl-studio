# Panel web de control

## Inicio

```bash
cargo run --release -- serve --config config/salutmental.yaml
```

El navegador se abre en `http://127.0.0.1:8090/`.

## Flujo de trabajo

1. Edita las URL semilla y la lista de dominios autorizados.
2. Revisa `robots.txt`, las rutas denegadas y los filtros regex.
3. Ajusta límites y concurrencia.
4. Edita las reglas de extracción como un array JSON.
5. Pulsa **Validar** para ejecutar la validación del backend.
6. Pulsa **Guardar** para reemplazar el YAML de forma atómica.
7. Pulsa **Iniciar escaneo**.
8. Consulta el progreso y abre **Ver resultados** al finalizar.

## API local

| Método | Ruta | Función |
|---|---|---|
| GET | `/api/config` | Carga la configuración activa |
| PUT | `/api/config` | Valida y guarda la configuración |
| POST | `/api/validate` | Valida sin guardar |
| POST | `/api/scan` | Guarda e inicia un escaneo |
| POST | `/api/scan/cancel` | Cancela el escaneo activo |
| GET | `/api/status` | Devuelve estado, progreso y resumen |
| GET | `/dashboard` | Muestra los resultados generados |

## Seguridad

- El bind predeterminado es `127.0.0.1:8090`.
- Escuchar en otra interfaz requiere `--allow-remote`.
- La API acepta JSON con un máximo de 1 MiB.
- El navegador y el backend validan la configuración.
- El frontend aplica una política CSP y no carga librerías externas.
- Las configuraciones pueden contener secretos; protege el YAML y evita subirlo a Git.
