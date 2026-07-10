# Migración desde 0.1.1 a 0.3.0

## Copia de seguridad

Desde el directorio padre del proyecto:

```bash
cp -a rama-controlled-extractor \
  "rama-controlled-extractor-backup-$(date +%Y%m%d-%H%M%S)"
```

La copia conserva la configuración y los resultados existentes.

## Aplicar el parche

Copia `rama-controlled-extractor-web-v0.3.0.patch` a la raíz del proyecto y ejecuta:

```bash
patch --dry-run -p1 < rama-controlled-extractor-web-v0.3.0.patch
patch -p1 < rama-controlled-extractor-web-v0.3.0.patch
```

El parche elimina archivos de correcciones antiguas que ya no son necesarios.

## Actualizar dependencias y verificar

```bash
cargo fmt --all
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo build --release
```

Cargo incorporará Axum 0.8.9 y actualizará `Cargo.lock` durante la primera comprobación.

## Iniciar el panel

```bash
./target/release/rama-controlled-extractor \
  serve \
  --config config/salutmental.yaml
```

El navegador se abrirá en `http://127.0.0.1:8090/`.

## Alternativa: sustituir por el proyecto completo

Descomprime el ZIP en un directorio nuevo, copia tu configuración autorizada y, solo si deseas conservarlos, los resultados:

```bash
cp -a proyecto-anterior/config/salutmental.yaml proyecto-nuevo/config/
cp -a proyecto-anterior/output/. proyecto-nuevo/output/
```
