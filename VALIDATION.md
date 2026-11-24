# Estado de validación

Actualizado: 2026-11-24. Entorno: Linux, Rust/Cargo 1.96.0 y Docker 28.3.3.

## Comprobaciones ejecutadas

| Comprobación | Resultado |
|---|---|
| `cargo fmt --all -- --check` | Correcto |
| `cargo test --locked --all-targets` | 14 pruebas correctas |
| `cargo clippy --locked --all-targets -- -D warnings` | Sin advertencias |
| `cargo check --locked --all-targets --no-default-features --features boring-emulation` | Correcto |
| `cargo build --locked --release` | Correcto |
| `docker compose build` | Imagen reconstruida con las correcciones |

También se comprobaron la sintaxis JavaScript y Bash, la configuración de
Compose y el arranque del panel con `config/example.yaml`, tanto en el host
como dentro de Docker.

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

## Dependencias y limitaciones pendientes

La comprobación RustSec del 2026-11-24, con `cargo audit --no-yanked`, encontró
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

Siguen pendientes el fijado de conexiones a las IP validadas frente a DNS
rebinding, documentado en `SECURITY.md`, las pruebas de carga y la
interoperabilidad TLS/proxy con servicios externos.

El informe inicial del 2026-07-10 solo incluía revisión estática porque aquel
entorno no disponía de Rust ni Docker. Las comprobaciones anteriores sustituyen
esa limitación de validación local.
