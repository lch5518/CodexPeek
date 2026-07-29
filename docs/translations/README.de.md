# CodexPeek – Codex Usage Monitor for Windows

**Languages:** [English (default)](../../README.md) · [한국어](README.ko.md) · [Español](README.es.md) · [Português (Brasil)](README.pt-BR.md) · [Bahasa Indonesia](README.id.md) · [日本語](README.ja.md) · [हिन्दी](README.hi.md) · [Deutsch](README.de.md) · [Français](README.fr.md) · [Tiếng Việt](README.vi.md) · [Türkçe](README.tr.md) · [العربية](README.ar.md)

Codex Usage Monitor ist ein kleines natives Windows-Widget, mit dem du deine Codex-Nutzung auf einen Blick prüfen kannst.
Es zeigt die primären und sekundären Rate-Limit-Zeitfenster in der Taskleiste, in einem schwebenden Widget und im System-Tray an.

![Codex Usage Monitor taskbar widget](../images/taskbar-widget-en.png)

## Highlights

- Zeigt primäre und sekundäre Codex-Nutzungsfenster einschließlich Reset-Zeiten.
- Verwendet die `app-server`-Schnittstelle der installierten Codex CLI, statt Authentifizierungsdateien zu parsen.
- Ermöglicht die manuelle Auswahl aus bis zu acht isolierten Nutzungsprofilen.
- Unterstützt die Anzeige des Widgets auf jeder Taskleiste oder nur auf dem primären Monitor.
- Fällt sicher auf ein schwebendes Widget und ein Tray-Symbol zurück, wenn die Taskleisten-Anbindung nicht verfügbar ist.
- Unterstützt manuelles Aktualisieren, automatische Aktualisierungsintervalle, Windows-Autostart, Diagnosen und eine lokalisierte UI.

## Funktionsweise

Der Monitor startet `codex app-server --stdio` als lokalen Child-Prozess und tauscht JSONL-Nachrichten über Standard-Eingabe und Standard-Ausgabe aus.
Die installierte Codex CLI übernimmt ihre eigene Authentifizierung und kann unter ihrer bestehenden Konfiguration und Netzwerkrichtlinie OpenAI kontaktieren.

Der Monitor fragt nur den Anmeldestatus und die für die Anzeige benötigten Nutzungsfenster ab.
Er startet keine Codex-Aufgabe und ruft `codex exec` nicht auf.

## Nutzungsprofile

Das nicht löschbare Systemprofil **Standard-Codex-Konto** verwendet das beim Start von
CodexPeek geerbte Codex-Home oder den CLI-Standard, wenn `CODEX_HOME` nicht gesetzt ist.
Jedes verwaltete Profil erhält ein separates Codex-Home unter
`%APPDATA%\CodexPeek\profiles`. Insgesamt sind einschließlich des Systemprofils
höchstens acht Profile möglich.

Profilnamen werden von dir vergeben. CodexPeek prüft weder E-Mail-Adresse noch Konto-ID;
bestätige deshalb beim Hinzufügen oder erneuten Anmelden das gewünschte ChatGPT-Konto im
Browser. Die Auswahl ändert nur, welche Nutzung CodexPeek abfragt und anzeigt. Anmeldungen
in Terminal, IDE, Codex-App, WSL, Remote SSH und Dev Containers bleiben unverändert.

Die Auswahl erfolgt immer manuell. CodexPeek wählt oder rotiert Profile nicht automatisch
anhand des verbleibenden Limits und leitet keine Codex-Aufgaben über ein Profil. Beim
Löschen eines verwalteten Profils gehen seine lokalen Daten einschließlich der separat
gespeicherten CLI-Anmeldedaten unwiederbringlich verloren; prüfe die Bestätigung sorgfältig.

CodexPeek liest, parst oder kopiert niemals die `auth.json` eines Profils. Nur der
zugehörige `app-server`-Child eines verwalteten Profils erhält dessen `CODEX_HOME` und die
Datei-Credential-Store-Einstellung. Diagnosen enthalten nur aggregierte Anzahlen, keine
Labels, Pfade oder Kontodaten.

### Profilverwaltung

Du kannst das Systemprofil umbenennen, es aber nicht abmelden oder löschen. Ein eigener Name
für das Systemprofil ändert nur die Anzeige in CodexPeek und ist keine Kontoidentität. Nur die
Profilverwaltung kennzeichnet es als Standardkonto.

Im Tray-Untermenü **Nutzungsprofile** kannst du ein Profil auswählen und **Nutzungsprofile
verwalten** öffnen; einen Befehl zum Hinzufügen gibt es dort nicht. Profile fügst du nur mit
`+` unter der Liste in der Profilverwaltung hinzu. Es gibt unten keinen Schließen- oder
Hinzufügen-Knopf: Schließe die Profilverwaltung über das Fenster-`X` oder Esc.

## Voraussetzungen

- Windows 10 oder Windows 11, x64.
- Eine angemeldete [Codex CLI](https://github.com/openai/codex) mit Unterstützung für `account/read` und `account/rateLimits/read`.

## Herunterladen und Ausführen

Prüfe zuerst, ob die Codex CLI installiert und angemeldet ist:

```powershell
codex --version
codex login status
```

### Installer (empfohlen)

1. Lade `CodexPeek-Setup-v<version>-x64.exe` aus dem
   [neuesten GitHub Release](https://github.com/lch5518/CodexPeek/releases/latest) herunter.
2. Führe das Setup aus und folge den Anweisungen. Administratorzugriff ist nicht erforderlich.
3. Starte **Codex Usage Monitor** über das Startmenü.

### Portable

1. Lade `codex-peek-v<version>-windows-x86_64-portable.zip` aus dem
   neuesten Release herunter.
2. Entpacke die ZIP-Datei vollständig in einen beschreibbaren Ordner.
3. Führe `codex-peek.exe` aus dem entpackten Ordner aus.

### Aus dem Quellcode bauen

Diese Option erfordert Rust 1.85 oder neuer, Visual Studio 2022 C++ Build Tools und ein
Windows SDK. Sie führt die App aus dem geklonten Repository aus und erstellt keine
Startmenü-Verknüpfung und keinen Uninstaller.

```powershell
git clone https://github.com/lch5518/CodexPeek.git
Set-Location .\CodexPeek
cargo build --release
.\target\release\codex-peek.exe
```

So prüfst du den Build und die Codex-CLI-Verbindung, ohne die UI zu öffnen:

```powershell
.\target\release\codex-peek.exe --diagnose
```

### Codex um die Installation bitten

Kopiere den folgenden Prompt in Codex. Er bevorzugt den verifizierten Installer und fällt nur dann auf einen
Source-Build zurück, wenn keine kompatiblen Release-Assets verfügbar sind.

```text
Installiere CodexPeek auf diesem Windows x64-Computer und schließe die Verifikation für mich ab.

1. Bestätige, dass dies Windows x64 ist, und führe dann `codex --version` und `codex login status` aus.
2. Verwende nur das offizielle Repository und seine Releases:
   https://github.com/lch5518/CodexPeek
3. Bevorzuge die neueste `CodexPeek-Setup-v<version>-x64.exe`. Lade sie zusammen mit
   `SHA256SUMS.txt` herunter, suche den exakten Installer-Eintrag in dieser Datei,
   berechne den SHA-256 des Installers und fahre nur fort, wenn die Hashes übereinstimmen.
   Deaktiviere keine Sicherheitskontrollen und führe keine Datei aus, deren Checksumme
   fehlt oder abweicht.
4. Installiere es für den aktuellen Benutzer, ohne Administratorzugriff anzufordern.
   Erhalte vorhandene CodexPeek-Einstellungen und beende keine laufende App und keinen
   nicht zugehörigen Prozess; sag mir, wenn ich die App selbst schließen muss.
5. Nur wenn keine kompatiblen Release-Assets verfügbar sind, klone das offizielle
   Repository in ein neues, vom Benutzer beschreibbares Verzeichnis und führe
   `cargo build --release` aus. Wenn Git, Rust 1.85+, Visual Studio 2022 C++ Build Tools
   oder ein Windows SDK installiert werden müssen, erkläre zuerst genau, was sich ändern
   wird, und bitte um meine Zustimmung.
6. Lies oder drucke niemals den Inhalt von `%USERPROFILE%\.codex\auth.json`.
   Die Authentifizierung darf nur über die installierte Codex CLI erfolgen.
7. Führe nach Installation oder Build die resultierende Datei `codex-peek.exe --diagnose`
   aus. Wenn dies erfolgreich ist, starte CodexPeek.
8. Melde die gewählte Installationsmethode, die installierte Version, den Speicherort der
   ausführbaren Datei, das Checksum-Ergebnis und das Diagnoseergebnis. Wenn etwas
   fehlschlägt, stoppe sicher und erkläre den genauen Blocker, ohne sensible Informationen
   offenzulegen.
```

Installer- und Portable-Editionen verwenden `%APPDATA%\CodexPeek\settings.json`, daher
werden Einstellungen geteilt, wenn du zwischen ihnen wechselst. Der Installer fügt eine Startmenü-Verknüpfung hinzu,
aktiviert Windows-Autostart aber nicht standardmäßig.

Erste Releases sind nicht code-signiert und können Microsoft Defender SmartScreen auslösen.
Lade nur aus dem offiziellen Release herunter und verifiziere die Datei gegen `SHA256SUMS.txt`.

Weitere Informationen zu Hash-Verifikation, Updates, Deinstallationsverhalten, Diagnosen und Fehlerbehebung findest du im [detaillierten Installationsleitfaden (Korean)](../INSTALL.md).

## Monitor verwenden

Verwende das Tray-Menü, um die Nutzung zu aktualisieren, ein Aktualisierungsintervall von 1/5/10/15/30 Minuten zu wählen und das Widget ein- oder auszublenden.
Es bietet außerdem Einstellungen für Windows-Autostart, Startansicht, Authentifizierungsaktualisierung, automatische Authentifizierungsaktualisierung, Sprache und Diagnosen.
Wähle **Widget: all monitors** oder **Widget: primary monitor only**, um die Platzierung auf mehreren Monitoren zu steuern; die Auswahl bleibt über Neustarts hinweg gespeichert.

Standardmäßig folgt die UI-Sprache dem Windows-Gebietsschema, wenn es einer unterstützten Sprache entspricht. Du kannst eine Sprache auch manuell im Tray-Menü auswählen. Unterstützte Sprachen sind Koreanisch, Englisch, Spanisch, brasilianisches Portugiesisch, Indonesisch, Japanisch, Hindi, Deutsch, Französisch, Vietnamesisch, Türkisch und Arabisch.

Das Taskleisten-Widget nutzt das helle/dunkle Windows-Systemdesign für seinen Text und lässt das native Taskleistenmaterial im Hintergrund durchscheinen.

Es läuft immer nur eine Nutzungsanfrage gleichzeitig. Fehlgeschlagene Anfragen werden mit steigenden Verzögerungen erneut versucht, während die letzten erfolgreichen Werte sichtbar bleiben.

Wenn das Taskleisten-Widget nach einem Explorer-Neustart oder einer Änderung des Taskleistenlayouts nicht angehängt werden kann, bleibt das Tray-Symbol verfügbar und der Monitor versucht es sicher erneut.

## Datenschutz und Sicherheit

Der Monitor liest oder parst niemals den Inhalt von `%USERPROFILE%\.codex\auth.json`.
Diagnosen prüfen nur, ob dieser Pfad existiert.

Rohe RPC-Antworten werden nur so lange verarbeitet, wie es nötig ist, um den Login-Typ und die angezeigten Rate-Limit-Felder zu extrahieren.
Tokens, Konto-IDs, E-Mail-Adressen, Inhalte von Authentifizierungsdateien und Proxy-Werte werden nicht gespeichert und nicht in Logs geschrieben.

Einstellungen werden in `%APPDATA%\CodexPeek\settings.json` gespeichert.
Ein begrenztes Diagnose-Log wird in `%TEMP%\codex-peek.log` gespeichert.

Die vollständigen Hinweise zu Datenverarbeitung und Vulnerability Reporting findest du in [SECURITY.md](../../SECURITY.md).

## Fehlerbehebung

| Problem | Vorgehen |
| --- | --- |
| Codex CLI wird nicht gefunden | Führe `codex --version` und `where.exe codex` aus und stelle sicher, dass die Codex CLI auf `PATH` liegt. |
| Die CLI wird nicht unterstützt | Aktualisiere die Codex CLI. Die erforderliche RPC-Unterstützung ist wichtiger als die angezeigte Versionsnummer. |
| Abgemeldet oder Authentifizierung abgelaufen | Schließe den normalen Login-Flow in der Codex CLI ab und wähle dann **Refresh authentication** im Tray-Menü. |
| Das Taskleisten-Widget ist auf dem falschen Monitor | Wähle **Widget: all monitors** oder **Widget: primary monitor only** im Tray-Menü. |
| Das Taskleisten-Widget fehlt | Verwende das schwebende Widget oder das Tray-Symbol, starte Explorer bei Bedarf neu und wähle den bevorzugten Widget-Monitor-Modus. |
| Mehr Details werden benötigt | Führe `--diagnose` aus oder öffne **Diagnostics** im Tray-Menü. |

## Entwicklung

Source-Builds erfordern Rust 1.85 oder neuer, Visual Studio 2022 C++ Build Tools und ein
Windows SDK. Baue und validiere aus dem Repository-Root:

```powershell
git clone https://github.com/lch5518/CodexPeek.git
Set-Location .\CodexPeek
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo build --release
```

Automatisierte Checks ersetzen nicht die Windows-, DPI-, Multi-Monitor- und Explorer-Recovery-Szenarien in der [Release-Checkliste](../RELEASE_CHECKLIST.md).

## ❤️ Support

Wenn CodexPeek dir Zeit spart, erwäge, die Entwicklung zu unterstützen.

- ⭐ Gib diesem Repository einen Stern
- ❤️ [Auf GitHub sponsern](https://github.com/sponsors/lch5518)

Jedes Sponsoring hilft, das Projekt aktiv zu pflegen.

## Lizenz

Dieses Projekt ist unter der [MIT License](../../LICENSE) verfügbar.
Hinweise zu Drittanbietern findest du in [THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md).
