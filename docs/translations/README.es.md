# CodexPeek – Codex Usage Monitor for Windows

**Languages:** [English (default)](../../README.md) · [한국어](README.ko.md) · [Español](README.es.md) · [Português (Brasil)](README.pt-BR.md) · [Bahasa Indonesia](README.id.md) · [日本語](README.ja.md) · [हिन्दी](README.hi.md) · [Deutsch](README.de.md) · [Français](README.fr.md) · [Tiếng Việt](README.vi.md) · [Türkçe](README.tr.md) · [العربية](README.ar.md)

Codex Usage Monitor es un pequeño widget nativo para Windows que permite consultar tu uso de Codex de un vistazo.
Muestra las ventanas de límite de uso primaria y secundaria en la barra de tareas, en un widget flotante y en la bandeja del sistema.

![Widget de Codex Usage Monitor en la barra de tareas](../images/taskbar-widget-en.png)

## Aspectos destacados

- Muestra las ventanas de uso primaria y secundaria de Codex, incluidos los horarios de restablecimiento.
- Estima cuándo puede agotarse cada ventana a partir de observaciones correctas recientes y muestra
  la estimación en los detalles de uso y en la información de la barra de tareas (novedad de esta versión).
- Usa la interfaz `app-server` del Codex CLI instalado en lugar de analizar archivos de autenticación.
- Permite elegir manualmente entre un máximo de ocho perfiles de uso aislados.
- Permite mostrar el widget en todas las barras de tareas o solo en el monitor principal.
- Recurre de forma segura a un widget flotante y a un icono de bandeja cuando no puede acoplarse a la barra de tareas.
- Admite actualización manual, intervalos de actualización automática, inicio con Windows, diagnósticos e interfaz localizada.

## Cómo funciona

El monitor inicia `codex app-server --stdio` como un proceso hijo local e intercambia mensajes JSONL por la entrada y salida estándar.
El Codex CLI instalado gestiona su propia autenticación y puede comunicarse con OpenAI según su configuración y política de red existentes.

El monitor solicita únicamente el estado de sesión iniciada y las ventanas de uso necesarias para mostrarlas.
No inicia una tarea de Codex ni llama a `codex exec`.

## Perfiles de uso

El perfil del sistema **Cuenta predeterminada de Codex**, que no se puede eliminar, usa el directorio principal
de Codex heredado al iniciar CodexPeek o el valor predeterminado del CLI si `CODEX_HOME`
no está definido. Cada perfil administrado usa un directorio principal de Codex separado
bajo `%APPDATA%\CodexPeek\profiles`. Se admiten ocho perfiles en total, incluido
el perfil del sistema.

Tú proporcionas las etiquetas de los perfiles. CodexPeek no inspecciona el correo ni el
ID de la cuenta, así que confirma en el navegador qué cuenta de ChatGPT quieres usar al
añadir un perfil o volver a iniciar sesión. La selección solo cambia el uso que CodexPeek
consulta y muestra. No cambia las sesiones del terminal, IDE, aplicación Codex, WSL,
Remote SSH ni Dev Containers.

La selección siempre es manual. CodexPeek no selecciona ni rota perfiles automáticamente
según el límite restante y no dirige trabajos de Codex mediante un perfil. Al eliminar un
perfil administrado se borran de forma irreversible sus datos locales, incluidas las
credenciales del CLI almacenadas por separado; revisa atentamente la confirmación.

CodexPeek nunca lee, analiza ni copia el `auth.json` de ningún perfil. Solo el proceso hijo
`app-server` del perfil administrado recibe su `CODEX_HOME` y la configuración de almacén
de credenciales en archivo. Los diagnósticos incluyen únicamente recuentos agregados, sin
etiquetas, rutas ni datos de cuenta.

### Administrador de perfiles

Puedes cambiar el nombre del perfil del sistema, pero no cerrar su sesión ni eliminarlo. Un
nombre personalizado del perfil del sistema solo cambia lo que muestra CodexPeek; no es una
identidad de cuenta. Solo el administrador de perfiles lo marca como la cuenta predeterminada.

El submenú de bandeja **Perfiles de uso** permite seleccionar un perfil y abrir **Administrar
perfiles de uso**; no tiene un comando para agregar. Agrega perfiles solo con `+`, debajo de la
lista del administrador. No hay botón inferior de Cerrar ni Agregar: usa la `X` de la ventana o
Escape para cerrar el administrador.

## Requisitos

- Windows 10 o Windows 11, x64.
- Un [Codex CLI](https://github.com/openai/codex) con sesión iniciada y compatibilidad con `account/read` y `account/rateLimits/read`.

## Descargar y ejecutar

Primero verifica que Codex CLI esté instalado y tenga la sesión iniciada:

```powershell
codex --version
codex login status
```

### Instalador (recomendado)

1. Descarga `CodexPeek-Setup-v<version>-x64.exe` desde la
   [última GitHub Release](https://github.com/lch5518/CodexPeek/releases/latest).
2. Ejecuta el instalador y sigue las indicaciones. No se requiere acceso de administrador.
3. Inicia **Codex Usage Monitor** desde el menú Inicio.

### Portable

1. Descarga `codex-peek-v<version>-windows-x86_64-portable.zip` desde la
   última release.
2. Extrae el ZIP por completo en una carpeta con permisos de escritura.
3. Ejecuta `codex-peek.exe` desde la carpeta extraída.

### Compilar desde el código fuente

Esta opción requiere Rust 1.85 o posterior, Visual Studio 2022 C++ Build Tools y un
Windows SDK. Ejecuta la aplicación desde el repositorio clonado y no crea un acceso
directo en el menú Inicio ni un desinstalador.

```powershell
git clone https://github.com/lch5518/CodexPeek.git
Set-Location .\CodexPeek
cargo build --release
.\target\release\codex-peek.exe
```

Para comprobar la compilación y la conexión con Codex CLI sin abrir la interfaz:

```powershell
.\target\release\codex-peek.exe --diagnose
```

### Pedirle a Codex que lo instale

Copia el prompt siguiente en Codex. Prefiere el Instalador verificado y solo recurre a
una compilación desde código fuente cuando no hay assets de Release compatibles.

```text
Instala CodexPeek en este equipo Windows x64 y completa la verificación por mí.

1. Confirma que esto es Windows x64 y luego ejecuta `codex --version` y `codex login status`.
2. Usa solo el repositorio oficial y sus Releases:
   https://github.com/lch5518/CodexPeek
3. Prefiere el `CodexPeek-Setup-v<version>-x64.exe` más reciente. Descárgalo junto con
   `SHA256SUMS.txt`, encuentra la entrada exacta del Instalador en ese archivo, calcula
   el SHA-256 del Instalador y continúa solo si los hashes coinciden. No desactives
   controles de seguridad ni ejecutes un archivo cuya suma de comprobación falte o sea diferente.
4. Instálalo para el usuario actual sin solicitar acceso de administrador. Conserva
   la configuración existente de CodexPeek y no detengas una aplicación en ejecución ni
   procesos no relacionados; dime si necesito cerrar la aplicación yo mismo.
5. Solo si no hay assets de Release compatibles disponibles, clona el repositorio oficial
   en un directorio nuevo con permisos de escritura para el usuario y ejecuta `cargo build --release`.
   Si es necesario instalar Git, Rust 1.85+, Visual Studio 2022 C++ Build Tools o un Windows SDK,
   primero explica exactamente qué cambiará y pide mi aprobación.
6. Nunca leas ni imprimas el contenido de `%USERPROFILE%\.codex\auth.json`. La autenticación
   debe gestionarse únicamente mediante el Codex CLI instalado.
7. Después de la instalación o compilación, ejecuta el `codex-peek.exe --diagnose` resultante.
   Si se completa correctamente, inicia CodexPeek.
8. Informa el método de instalación seleccionado, la versión instalada, la ubicación del ejecutable,
   el resultado de la suma de comprobación y el resultado del diagnóstico. Si algo falla, detente
   de forma segura y explica el bloqueo exacto sin exponer información sensible.
```

Las ediciones Instalador y Portable usan `%APPDATA%\CodexPeek\settings.json`, por lo que
comparten la configuración si alternas entre ellas. El instalador agrega un acceso directo al menú
Inicio, pero no habilita el inicio con Windows de forma predeterminada.

Las versiones iniciales no están firmadas con código y pueden activar Microsoft Defender SmartScreen.
Descarga únicamente desde la release oficial y verifica el archivo con `SHA256SUMS.txt`.

Consulta la [guía de instalación detallada (en coreano)](../INSTALL.md) para ver la verificación de hash,
actualizaciones, comportamiento de desinstalación, diagnósticos y solución de problemas.

## Usar el monitor

Usa el menú de la bandeja para actualizar el uso, elegir un intervalo de actualización de 1/5/10/15/30 minutos y mostrar u ocultar el widget.
También ofrece ajustes de inicio con Windows, vista inicial, actualización de autenticación, actualización automática de autenticación, idioma y diagnósticos.
Elige **Widget: all monitors** o **Widget: primary monitor only** para controlar la ubicación en varios monitores; la selección se conserva entre reinicios.

De forma predeterminada, el idioma de la interfaz sigue la configuración regional de Windows cuando coincide con un idioma compatible. También puedes elegir un idioma manualmente desde el menú de la bandeja. Los idiomas compatibles son coreano, inglés, español, portugués brasileño, indonesio, japonés, hindi, alemán, francés, vietnamita, turco y árabe.

El widget de la barra de tareas usa el tema claro/oscuro del sistema de Windows para el texto y deja que el material nativo de la barra de tareas se vea a través del fondo.

Solo se ejecuta una solicitud de uso a la vez. Las solicitudes fallidas se reintentan con retrasos crecientes mientras los últimos valores correctos permanecen visibles.

Si el widget de la barra de tareas no puede acoplarse después de reiniciar Explorer o de un cambio en el diseño de la barra de tareas, el icono de bandeja sigue disponible y el monitor reintenta de forma segura.

Cuando la predicción está activada (valor predeterminado), solo se guardan observaciones correctas
en el archivo local independiente `%APPDATA%\CodexPeek\usage-history.json`. La estimación requiere
datos recientes del mismo perfil, ventana y ciclo de restablecimiento; los datos nuevos o antiguos
se muestran como recopilación o desactualizados en lugar de presentar una estimación actual. Desde
el menú de la bandeja **Usage forecasting** puedes desactivarla o elegir **Clear usage forecast
history**; al eliminar un perfil administrado también se elimina su historial. La predicción es una
estimación local, no garantiza la política de límites de OpenAI y nunca se sube ni se sincroniza.

## Privacidad y seguridad

El monitor nunca lee ni analiza el contenido de `%USERPROFILE%\.codex\auth.json`.
Los diagnósticos solo comprueban si esa ruta existe.

Las respuestas RPC sin procesar se procesan solo el tiempo necesario para extraer el tipo de inicio de sesión y los campos de límite de uso mostrados.
Los tokens, ID de cuenta, direcciones de correo, contenido de archivos de autenticación y valores de proxy no se almacenan ni se escriben en registros.

La configuración se guarda en `%APPDATA%\CodexPeek\settings.json`.
Un registro de diagnóstico acotado se guarda en `%TEMP%\codex-peek.log`.

`usage-history.json` contiene únicamente el ID interno del perfil, `Primary` o `Secondary`, el
porcentaje de uso, una marca de tiempo de restablecimiento opcional y la marca de tiempo de la
observación correcta. No contiene correo electrónico, ID de cuenta, nombre o ruta raíz del perfil,
tokens, contenido del archivo de autenticación, conversaciones o prompts, configuración del proxy
ni la respuesta RPC sin procesar. Se conservan como máximo 30 días y 1.000 muestras por perfil/
ventana; se omiten valores repetidos y observaciones separadas por menos de cinco minutos para
reducir escrituras. Un archivo dañado se aísla o reinicia sin impedir que se muestre el uso.

**Clear usage forecast history** elimina todas las muestras después de confirmar. El instalador y
Portable conservan `%APPDATA%\CodexPeek` al desinstalar, por lo que el historial puede permanecer
después de quitar la aplicación; usa la acción de la bandeja o elimina el archivo/carpeta
manualmente para una limpieza completa.

Para la guía completa sobre tratamiento de datos e informes de vulnerabilidades, consulta [SECURITY.md](../../SECURITY.md).

## Solución de problemas

| Problema | Qué hacer |
| --- | --- |
| No se encuentra Codex CLI | Ejecuta `codex --version` y `where.exe codex`, y luego asegúrate de que Codex CLI esté en `PATH`. |
| El CLI no es compatible | Actualiza Codex CLI. La compatibilidad con los RPC requeridos importa más que el número de versión mostrado. |
| Sesión cerrada o autenticación vencida | Completa el flujo normal de inicio de sesión en Codex CLI y luego elige **Refresh authentication** en el menú de la bandeja. |
| El widget de la barra de tareas está en el monitor incorrecto | Elige **Widget: all monitors** o **Widget: primary monitor only** desde el menú de la bandeja. |
| Falta el widget de la barra de tareas | Usa el widget flotante o el icono de bandeja, reinicia Explorer si es necesario y selecciona el modo de monitor de widget preferido. |
| Se necesita más detalle | Ejecuta `--diagnose` o abre **Diagnostics** desde el menú de la bandeja. |

## Desarrollo

Las compilaciones desde código fuente requieren Rust 1.85 o posterior, Visual Studio 2022 C++ Build Tools y un
Windows SDK. Compila y valida desde la raíz del repositorio:

```powershell
git clone https://github.com/lch5518/CodexPeek.git
Set-Location .\CodexPeek
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo build --release
```

Las comprobaciones automatizadas no sustituyen los escenarios de recuperación de Windows, DPI, varios monitores y Explorer de la [lista de verificación de release](../RELEASE_CHECKLIST.md).

## ❤️ Soporte

Si CodexPeek te ahorra tiempo, considera apoyar su desarrollo.

- ⭐ Dale una estrella a este repositorio
- ❤️ [Patrocinar en GitHub](https://github.com/sponsors/lch5518)

Cada patrocinio ayuda a mantener el proyecto activamente mantenido.

## Licencia

Este proyecto está disponible bajo la [MIT License](../../LICENSE).
Consulta [THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md) para ver los avisos de terceros.
