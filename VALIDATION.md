# Estado de validación

Fecha de generación: 2026-07-10.

Se contrastaron las APIs utilizadas con la documentación y los ejemplos oficiales de Rama 0.3.0 para:

- `EasyHttpWebClient` y `HttpPooledConnectorConfig`;
- conectores Rustls/BoringSSL y soporte de proxy;
- `UserAgentEmulateLayer`, modificadores HTTP y `EmulateTlsProfileLayer`;
- `BodyExtractExt::try_capture_json`;
- cliente WebSocket, `Message::text`, `Message::into_data` y cierre del socket.

En el entorno de generación no había `cargo`, `rustc`, `rustfmt`, Docker ni acceso de red al registro de crates, por lo que no se ejecutó una compilación local. Sí se realizaron estas comprobaciones estáticas:

- parseo correcto de `Cargo.toml`, `config/example.yaml`, `docker-compose.yml` y el workflow de CI;
- balance léxico de delimitadores en todos los módulos Rust, ignorando strings, caracteres y comentarios;
- búsqueda de marcadores de conflicto, fragmentos truncados y cadenas accidentales de `.await`;
- ausencia de llamadas `.unwrap()`/`.expect()`, bloques `unsafe` y credenciales incrustadas;
- revisión manual de redirects, SSRF, redacción de secretos, límites de tamaño y timeouts.

El repositorio incluye CI para efectuar `fmt`, `check`, `test`, `clippy` y una comprobación específica del feature `boring-emulation` al publicarlo en GitHub. La primera compilación real debe considerarse todavía una validación obligatoria antes de desplegarlo.

## Validaciones de la versión 0.3.0

- Sintaxis JavaScript comprobada con Node.js 22 (`node --check web/app.js`).
- `Cargo.toml` validado con un parser TOML.
- Configuraciones YAML validadas con PyYAML.
- Identificadores HTML comprobados: no existen duplicados ni referencias JavaScript ausentes.
- Delimitadores léxicos de los módulos Rust comprobados mediante un analizador local.
- Rutas de limpieza limitadas a descendientes directos de `output/` y cubiertas por tests unitarios.

El entorno de generación no dispone del toolchain Rust ni de Docker, por lo que debe ejecutarse en Ubuntu la secuencia final `cargo fmt`, `cargo check`, `cargo test`, `cargo clippy` y `cargo build --release`.
