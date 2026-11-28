# Seguridad

## Modelo de amenazas

El extractor procesa URLs y contenido remoto no confiable. Los riesgos principales son SSRF, redirects hacia redes internas, DNS rebinding, cuerpos desmesurados, compresión maliciosa, HTML/JSON patológico, fuga de credenciales de proxy y sobrecarga involuntaria de terceros.

## Controles incluidos

- allowlist obligatorio de dominios y puertos;
- validación de cada redirect;
- comprobación de `robots.txt` y límites por host antes de cada petición de contenido, incluidos redirects y reintentos;
- bloqueo de credenciales embebidas en URL;
- resolución DNS previa y bloqueo de rangos privados/especiales;
- bloqueo de IPv4 mapeada en IPv6;
- límites de tamaño, timeout de solicitud/handshake, timeout de cuerpo, páginas, profundidad y concurrencia;
- demora mínima y semáforo por host;
- `robots.txt` con modo fail-closed;
- sin ejecución de JavaScript;
- redacción de cabeceras sensibles conocidas antes de persistirlas;
- eliminación de usuario, contraseña, query y fragmento del identificador de proxy guardado;
- SQLite con consultas parametrizadas;
- proceso Docker sin privilegios.

## Limitación DNS rebinding

La comprobación DNS y la conexión del cliente no están fijadas a la misma respuesta DNS. Existe una ventana TOCTOU. Para producción, ejecute el contenedor con política de egress que solo permita los destinos autorizados o sustituya el resolver de Rama por uno que devuelva y fije las IP validadas.

## Secretos

No guarde contraseñas de proxy, tokens ni cookies en archivos versionados. Genere la configuración en despliegue, restrinja sus permisos y evite registrar cabeceras sensibles. El ejemplo no incluye autenticación de sitios ni gestión de cookies deliberadamente.

## Persistencia de contenido

El modo incremental añade validadores HTTP, hashes y enlaces a la base SQLite.
La huella de configuración separa las bases de comparación sin copiar las
cabeceras de autenticación ni las credenciales de proxy a la caché. Las URLs y
los validadores almacenados pueden contener datos sensibles. Los validadores se
envían únicamente a la URL final que los emitió; una redirección a otro destino
fuerza una petición sin esos validadores. Los errores HTTP o de extracción no
sustituyen la última base válida.
Proteja y respalde la base de datos, el JSONL y los cuerpos: el historial de
monitorización crece con el tiempo y puede contener datos sensibles. La limpieza
desde el panel se rechaza cuando el modo incremental está activo. Use un único
proceso de rastreo escritor por base de datos.

`save_bodies: true` conserva el cuerpo completo recibido y puede almacenar información sensible aunque las cabeceras estén redactadas. Desactive esta opción salvo que exista una necesidad operativa clara, cifre el volumen de salida y aplique retención limitada.

Los errores producidos por dependencias de red podrían contener detalles del endpoint. No active trazas de máxima verbosidad en entornos con secretos sin revisar antes la salida.

## Reporte

Revise primero que el problema pertenezca a este proyecto y no a Rama. Incluya versión, feature TLS, configuración anonimizada y pasos reproducibles, sin secretos ni datos personales.

## Panel web local

El subcomando `serve` escucha por defecto en `127.0.0.1:8090`. La aplicación rechaza un bind no loopback salvo que se indique explícitamente `--allow-remote`.

El panel no incorpora autenticación porque está diseñado para uso local. No debe exponerse directamente a Internet ni a una red que no sea de confianza. Si se habilita acceso remoto, debe situarse detrás de una capa autenticada y TLS, y limitarse mediante firewall.

La API:

- acepta únicamente JSON para las operaciones de configuración y escaneo;
- limita el cuerpo de las peticiones a 1 MiB;
- vuelve a ejecutar `AppConfig::validate()` antes de guardar o escanear;
- impide dos escaneos simultáneos;
- no permite limpiar rutas absolutas, rutas con `..` ni el propio directorio `output/`;
- sirve los assets con CSP, `nosniff` y sin caché.

El guardado mediante la interfaz serializa de nuevo el YAML. Los comentarios del archivo original no se conservan. Las credenciales presentes en cabeceras, proxies o WebSocket permanecen en el archivo de configuración local.
