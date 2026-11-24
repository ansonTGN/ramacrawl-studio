# Rama Controlled Extractor 0.3.0

## Incorporado

- Subcomando `serve` con panel HTML local.
- Formulario completo para editar todos los parámetros del crawler.
- Validación inmediata en JavaScript y validación definitiva en Rust.
- Guardado atómico del YAML activo.
- Inicio y cancelación de escaneos desde el navegador.
- Progreso en tiempo real: páginas, registros, errores y bytes.
- Generación automática del dashboard al terminar.
- Opción segura para limpiar salidas bajo `output/`.
- Assets HTML/CSS/JS embebidos en el binario.
- Política CSP y límite JSON de 1 MiB.
- Bind loopback obligatorio salvo `--allow-remote`.
- Docker Compose preparado para `127.0.0.1:8090`.

## Inicio

```bash
cargo run --release -- serve --config config/example.yaml
```

## Migración

La versión añade la dependencia `axum = 0.8.9`. La primera ejecución de Cargo actualizará `Cargo.lock`.

Antes de sustituir el proyecto, conserva una copia de `config/` y `output/`.
