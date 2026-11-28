# Estado de validación

Actualizado: 2026-11-28. Entorno: Linux, Rust/Cargo 1.96.0 y Docker 28.3.3.

## Comprobaciones ejecutadas

| Comprobación | Resultado |
|---|---|
| `cargo fmt --all -- --check` | Correcto |
| `cargo test --locked --all-targets` | 23 pruebas correctas |
| `cargo clippy --locked --all-targets -- -D warnings` | Sin advertencias |
| `cargo check --locked --all-targets --no-default-features --features boring-emulation` | Correcto |
| `cargo build --locked --release` | Correcto |
| `docker compose build` | Imagen reconstruida correctamente |

También se comprobaron la sintaxis JavaScript y Bash, la configuración de
Compose y el arranque del panel en un contenedor de solo lectura, sin capacidades
Linux adicionales. Dentro del contenedor se validaron la configuración de ejemplo,
la ayuda de `watch`, los controles incrementales y `/api/history` sin base previa.
Las pruebas de monitorización descritas abajo utilizaron el binario release local.

## Regresiones y pruebas funcionales

- Los selectores CSS inválidos y los JSONPath inválidos se rechazan antes de
  guardar o escanear. Una configuración inválida no reemplaza el YAML existente.
- La API devuelve HTTP 400 al validar, guardar o intentar iniciar un escaneo
  con selectores inválidos.
- `crawler::tests::redirects_obey_destination_robots` cubre cinco escenarios:
  bloqueo en el mismo origen, bloqueo al cambiar de origen, fallo de
  `robots.txt` con fail-closed, redirección permitida y respeto de robots
  desactivado. Comprueba las peticiones emitidas y los metadatos de SQLite.
- Tres pruebas HTTP con el binario release y servidores locales confirmaron
  que no se solicita `/private` tras una redirección del mismo origen ni de
  otro origen autorizado; una redirección permitida a `/public` sigue funcionando.
- Se verificaron extracción HTML/JSON/NDJSON, persistencia SQLite/JSONL,
  generación del dashboard, JSON streaming, WebSocket, límites, reintentos y
  operaciones de la API, incluida la cancelación.

Las pruebas funcionales utilizaron datos sintéticos y servidores locales;
no se ejecutaron rastreos sobre sitios externos.

## Monitorización periódica e incremental

Las nuevas pruebas Rust verifican:

- Reutilización de `ETag` y `Last-Modified`, respuestas 304, comparación SHA-256
  sin validadores y conservación exacta de cuerpos no UTF-8.
- Exploración de enlaces conservados cuando el índice responde 304, extracción
  solo de contenido nuevo o modificado y separación de caché al cambiar la configuración.
- Ausencia de validadores al redirigir a otro destino, rechazo de un 304 sin
  base previa y conservación de la base válida tras errores HTTP o de extracción.
- Migración del esquema SQLite anterior conservando páginas y registros,
  lectura de resúmenes antiguos y deduplicación antes de consumir el presupuesto.

La prueba HTTP con el cliente Rama real ejecutó tres rastreos consecutivos:
2 páginas nuevas; después 1 modificada y 1 sin cambios; finalmente 2 sin cambios
y 0 bytes de cuerpo descargados. El JSONL conservó los 3 registros extraídos.
La API devolvió el historial ordenado, rechazó `incremental + clean_output`
sin modificar la configuración ni los datos y completó un cuarto rastreo incremental.

`watch` completó tres ciclos sin solapamiento, mantuvo el retardo por host de
1200 ms entre ciclos con intervalo de 1 segundo y terminó correctamente con
Ctrl+C. El intervalo cero fue rechazado. Se comprobó en navegador el historial
de cuatro ejecuciones y la activación/desactivación de Inicio limpio según el
modo incremental, con inspección visual de la tabla.

La detección compara cuerpos y URL final; no calcula diferencias semánticas de
campos ni bajas de páginas. La caché y el historial no tienen retención automática.
Se admite un único proceso escritor por base; no hay coordinación entre procesos
ni garantía de escritura atómica conjunta entre SQLite y los archivos de salida.

## Dependencias y limitaciones pendientes

La comprobación RustSec del 2026-11-28, con `cargo audit --no-yanked`, encontró
0 vulnerabilidades y 0 avisos de soundness en `Cargo.lock`. No se comprobó
el estado de retirada de crates del registro (yanked).

Permanecen cuatro avisos de dependencias sin mantenimiento, sin versiones
corregidas indicadas por RustSec; no se han ocultado:

| Dependencia | Aviso |
|---|---|
| `chrono` | RUSTSEC-2026-0543 |
| `fxhash` | RUSTSEC-2025-0057 |
| `pin-utils` | RUSTSEC-2026-0441 |
| `unsafe-libyaml` | RUSTSEC-2024-0449 |

Siguen pendientes una frontier persistente con reanudación, el enfriamiento
compartido según `Retry-After` y el fijado de conexiones a las IP validadas frente a DNS
rebinding, documentado en `SECURITY.md`, las pruebas de carga y la
interoperabilidad TLS/proxy con servicios externos.

El informe inicial del 2026-07-10 solo incluía revisión estática porque aquel
entorno no disponía de Rust ni Docker. Las comprobaciones anteriores sustituyen
esa limitación de validación local.
