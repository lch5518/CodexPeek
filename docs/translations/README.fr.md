# Moniteur d'utilisation Codex

**Languages:** [English (default)](../../README.md) · [한국어](README.ko.md) · [Español](README.es.md) · [Português (Brasil)](README.pt-BR.md) · [Bahasa Indonesia](README.id.md) · [日本語](README.ja.md) · [हिन्दी](README.hi.md) · [Deutsch](README.de.md) · [Français](README.fr.md) · [Tiếng Việt](README.vi.md) · [Türkçe](README.tr.md) · [العربية](README.ar.md)

Codex Usage Monitor est un petit widget Windows natif qui permet de consulter rapidement votre utilisation de Codex.
Il affiche les fenêtres de limite de débit principale et secondaire dans la barre des tâches, dans un widget flottant et dans la zone de notification système.

![Widget Codex Usage Monitor dans la barre des tâches](../images/taskbar-widget-en.png)

## Points forts

- Affiche les fenêtres d'utilisation Codex principale et secondaire, y compris les heures de réinitialisation.
- Utilise l'interface `app-server` du Codex CLI installé au lieu d'analyser les fichiers d'authentification.
- Permet d'afficher le widget sur toutes les barres des tâches ou uniquement sur le moniteur principal.
- Bascule de façon sûre vers un widget flottant et une icône de zone de notification lorsque l'attachement à la barre des tâches n'est pas disponible.
- Prend en charge l'actualisation manuelle, les intervalles d'actualisation automatique, le démarrage avec Windows, les diagnostics et l'interface localisée.

## Fonctionnement

Le moniteur lance `codex app-server --stdio` comme processus enfant local et échange des messages JSONL via l'entrée et la sortie standard.
Le Codex CLI installé gère sa propre authentification et peut contacter OpenAI selon sa configuration existante et sa politique réseau.

Le moniteur demande uniquement l'état de connexion et les fenêtres d'utilisation nécessaires à l'affichage.
Il ne démarre aucune tâche Codex et n'appelle pas `codex exec`.

## Prérequis

- Windows 10 ou Windows 11, x64.
- Un [Codex CLI](https://github.com/openai/codex) connecté et compatible avec `account/read` et `account/rateLimits/read`.

## Télécharger et exécuter

Vérifiez d'abord que Codex CLI est installé et connecté :

```powershell
codex --version
codex login status
```

### Programme d'installation (recommandé)

1. Téléchargez `CodexPeek-Setup-v<version>-x64.exe` depuis la
   [dernière GitHub Release](https://github.com/lch5518/CodexPeek/releases/latest).
2. Lancez l'installation et suivez les invites. L'accès administrateur n'est pas requis.
3. Démarrez **Codex Usage Monitor** depuis le menu Démarrer.

### Portable

1. Téléchargez `codex-peek-v<version>-windows-x86_64-portable.zip` depuis la
   dernière release.
2. Extrayez complètement le ZIP dans un dossier où vous pouvez écrire.
3. Lancez `codex-peek.exe` depuis le dossier extrait.

### Compiler depuis les sources

Cette option nécessite Rust 1.85 ou version ultérieure, Visual Studio 2022 C++ Build Tools et un
Windows SDK. Elle exécute l'application depuis le dépôt cloné et ne crée ni raccourci dans le menu Démarrer
ni programme de désinstallation.

```powershell
git clone https://github.com/lch5518/CodexPeek.git
Set-Location .\CodexPeek
cargo build --release
.\target\release\codex-peek.exe
```

Pour vérifier la compilation et la connexion au Codex CLI sans ouvrir l'interface :

```powershell
.\target\release\codex-peek.exe --diagnose
```

### Demander à Codex de l'installer

Copiez l'invite ci-dessous dans Codex. Elle privilégie le programme d'installation vérifié et ne revient à une
compilation depuis les sources que si aucun artefact de Release compatible n'est disponible.

```text
Installe CodexPeek sur cet ordinateur Windows x64 et termine la vérification pour moi.

1. Confirme que cet ordinateur est sous Windows x64, puis exécute `codex --version` et `codex login status`.
2. Utilise uniquement le dépôt officiel et ses Releases :
   https://github.com/lch5518/CodexPeek
3. Privilégie le dernier `CodexPeek-Setup-v<version>-x64.exe`. Télécharge-le avec
   `SHA256SUMS.txt`, trouve l'entrée exacte de l'Installer dans ce fichier, calcule le
   SHA-256 de l'Installer et continue uniquement si les hachages correspondent. Ne désactive
   aucun contrôle de sécurité et n'exécute aucun fichier dont la somme de contrôle est absente
   ou différente.
4. Installe-le pour l'utilisateur actuel sans demander d'accès administrateur. Préserve les
   paramètres CodexPeek existants et n'arrête pas l'application en cours d'exécution ni aucun
   processus sans rapport ; indique-moi si je dois fermer l'application moi-même.
5. Seulement si aucun artefact de Release compatible n'est disponible, clone le dépôt officiel
   dans un nouveau répertoire accessible en écriture par l'utilisateur et exécute `cargo build --release`.
   Si Git, Rust 1.85+, Visual Studio 2022 C++ Build Tools ou un Windows SDK doivent être installés,
   explique d'abord exactement ce qui changera et demande mon approbation.
6. Ne lis jamais et n'affiche jamais le contenu de `%USERPROFILE%\.codex\auth.json`. L'authentification
   doit être gérée uniquement par le Codex CLI installé.
7. Après l'installation ou la compilation, exécute le `codex-peek.exe --diagnose` obtenu. S'il
   réussit, lance CodexPeek.
8. Indique la méthode d'installation choisie, la version installée, l'emplacement de l'exécutable,
   le résultat de la somme de contrôle et le résultat du diagnostic. En cas d'échec, arrête-toi
   en toute sécurité et explique le blocage exact sans exposer d'information sensible.
```

Les éditions Installer et Portable utilisent `%APPDATA%\CodexUsageMonitor\settings.json`, les
paramètres sont donc partagés si vous passez de l'une à l'autre. Le programme d'installation ajoute un raccourci au menu Démarrer
mais n'active pas le démarrage avec Windows par défaut.

Les premières releases ne sont pas signées par code et peuvent déclencher Microsoft Defender SmartScreen.
Téléchargez uniquement depuis la release officielle et vérifiez le fichier avec `SHA256SUMS.txt`.

Consultez le [guide d'installation détaillé (coréen)](../INSTALL.md) pour la vérification des hachages,
les mises à jour, le comportement de désinstallation, les diagnostics et le dépannage.

## Utiliser le moniteur

Utilisez le menu de la zone de notification pour actualiser l'utilisation, choisir un intervalle d'actualisation de 1/5/10/15/30 minutes, et afficher ou masquer le widget.
Il fournit aussi des paramètres pour le démarrage avec Windows, la vue de démarrage, l'actualisation de l'authentification, l'actualisation automatique de l'authentification, la langue et les diagnostics.
Choisissez **Widget: all monitors** ou **Widget: primary monitor only** pour contrôler le placement sur plusieurs moniteurs ; la sélection est mémorisée entre les redémarrages.

Par défaut, la langue de l'interface suit les paramètres régionaux de Windows lorsqu'ils correspondent à une langue prise en charge. Vous pouvez aussi choisir une langue manuellement depuis le menu de la zone de notification. Les langues prises en charge sont le coréen, l'anglais, l'espagnol, le portugais brésilien, l'indonésien, le japonais, l'hindi, l'allemand, le français, le vietnamien, le turc et l'arabe.

Le widget de barre des tâches utilise le thème clair/sombre de Windows pour son texte et laisse apparaître le matériau natif de la barre des tâches en arrière-plan.

Une seule demande d'utilisation s'exécute à la fois. Les demandes échouées sont retentées avec des délais croissants tandis que les dernières valeurs réussies restent visibles.

Si le widget ne peut pas être attaché à la barre des tâches après un redémarrage d'Explorer ou un changement de disposition de la barre des tâches, l'icône de zone de notification reste disponible et le moniteur réessaie de façon sûre.

## Confidentialité et sécurité

Le moniteur ne lit ni n'analyse jamais le contenu de `%USERPROFILE%\.codex\auth.json`.
Les diagnostics vérifient uniquement si ce chemin existe.

Les réponses RPC brutes sont traitées seulement le temps d'extraire le type de connexion et les champs de limite de débit affichés.
Les jetons, identifiants de compte, adresses e-mail, contenus des fichiers d'authentification et valeurs de proxy ne sont ni stockés ni écrits dans les journaux.

Les paramètres sont stockés dans `%APPDATA%\CodexUsageMonitor\settings.json`.
Un journal de diagnostic borné est stocké dans `%TEMP%\codex-peek.log`.

Pour les consignes complètes sur le traitement des données et le signalement des vulnérabilités, consultez [SECURITY.md](../../SECURITY.md).

## Dépannage

| Problème | Que faire |
| --- | --- |
| Codex CLI est introuvable | Exécutez `codex --version` et `where.exe codex`, puis vérifiez que Codex CLI est dans `PATH`. |
| Le CLI n'est pas pris en charge | Mettez Codex CLI à jour. La prise en charge RPC requise importe plus que le numéro de version affiché. |
| Vous êtes déconnecté ou l'authentification a expiré | Terminez le flux de connexion normal dans Codex CLI, puis choisissez **Refresh authentication** dans le menu de la zone de notification. |
| Le widget de barre des tâches est sur le mauvais moniteur | Choisissez **Widget: all monitors** ou **Widget: primary monitor only** dans le menu de la zone de notification. |
| Le widget de barre des tâches est absent | Utilisez le widget flottant ou l'icône de zone de notification, redémarrez Explorer si nécessaire, puis sélectionnez le mode de moniteur préféré pour le widget. |
| Plus de détails sont nécessaires | Exécutez `--diagnose` ou ouvrez **Diagnostics** depuis le menu de la zone de notification. |

## Développement

Les compilations depuis les sources nécessitent Rust 1.85 ou version ultérieure, Visual Studio 2022 C++ Build Tools et un
Windows SDK. Compilez et validez depuis la racine du dépôt :

```powershell
git clone https://github.com/lch5518/CodexPeek.git
Set-Location .\CodexPeek
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo build --release
```

Les contrôles automatisés ne remplacent pas les scénarios Windows, DPI, multi-moniteur et récupération d'Explorer de la [checklist de release](../RELEASE_CHECKLIST.md).

## ❤️ Soutien

Si CodexPeek vous fait gagner du temps, pensez à soutenir son développement.

- ⭐ Ajoutez une étoile à ce dépôt
- ❤️ [Sponsoriser sur GitHub](https://github.com/sponsors/lch5518)

Chaque sponsoring aide à maintenir activement le projet.

## Licence

Ce projet est disponible sous la [licence MIT](../../LICENSE).
Consultez [THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md) pour les avis de tiers.
