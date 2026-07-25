# README Source Installation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add source-build instructions and a copy-ready Codex installation prompt to both project READMEs.

**Architecture:** Keep Installer and Portable as the first two user paths, then add source build and agent-assisted installation as explicit alternatives in the same download section. English and Korean content must carry the same commands, safety boundaries, fallback order, and completion checks.

**Tech Stack:** GitHub-flavored Markdown, PowerShell, Cargo/Rust 1.85+, Windows 10/11 x64

## Global Constraints

- Modify only `README.md` and `README.ko.md` for the user-facing implementation.
- Prefer the official Installer and verify it against `SHA256SUMS.txt`.
- Use source build only when compatible Release assets are unavailable.
- Require approval before installing Git, Rust, Visual Studio Build Tools, or other system prerequisites.
- Never read or print `%USERPROFILE%\.codex\auth.json`; only Codex CLI may handle authentication.
- Do not terminate running applications or unrelated processes without user direction.
- Keep executable and artifact names aligned with `codex-peek`.

---

### Task 1: Add direct source-build instructions

**Files:**
- Modify: `README.md`
- Modify: `README.ko.md`

**Interfaces:**
- Consumes: the existing Installer and Portable instructions
- Produces: equivalent English and Korean source-build instructions

- [ ] **Step 1: Add the English source-build section**

Insert a `### Build from source` section after Portable. State that this path does not
create a Start Menu shortcut or uninstaller and requires Rust 1.85+, Visual Studio 2022
C++ Build Tools, and a Windows SDK. Include:

```powershell
git clone https://github.com/lch5518/CodexPeek.git
Set-Location .\CodexPeek
cargo build --release
.\target\release\codex-peek.exe
```

Include the diagnostic command:

```powershell
.\target\release\codex-peek.exe --diagnose
```

- [ ] **Step 2: Add the Korean source-build section**

Insert `### 소스에서 직접 빌드` at the matching location in `README.ko.md`, with the
same prerequisites, limitations, build command, executable path, and diagnostic command.

- [ ] **Step 3: Verify source-build parity**

Run:

```powershell
rg -n "Build from source|소스에서 직접 빌드|cargo build --release|target\\\\release\\\\codex-peek.exe|--diagnose" README.md README.ko.md
```

Expected: both README files contain their localized heading and the same build,
executable, and diagnostic commands.

### Task 2: Add copy-ready Codex installation prompts

**Files:**
- Modify: `README.md`
- Modify: `README.ko.md`

**Interfaces:**
- Consumes: official repository and release URLs, Installer filename contract, source-build commands
- Produces: one self-contained English prompt and one self-contained Korean prompt

- [ ] **Step 1: Add the English prompt**

Add `### Ask Codex to install it` and a `text` code fence. The prompt must tell Codex:

1. Confirm Windows x64 and run `codex --version` plus `codex login status`.
2. Use only `https://github.com/lch5518/CodexPeek`.
3. Prefer the latest `CodexPeek-Setup-v<version>-x64.exe`.
4. Download `SHA256SUMS.txt`, match the exact Installer entry, calculate SHA-256, and
   refuse to run on missing or mismatched checksums.
5. Install per-user without requesting administrator access.
6. Fall back to cloning and `cargo build --release` only when compatible Release assets
   are unavailable.
7. Explain and request approval before installing build prerequisites.
8. Never inspect authentication-file contents or terminate running processes.
9. Run `codex-peek.exe --diagnose`, launch the app after success, and report the version,
   install location, selected installation path, and diagnostic result.

- [ ] **Step 2: Add the Korean prompt**

Add `### Codex에 설치 요청` and a Korean `text` code fence with the same nine
requirements. It must be independently copyable without referring to nearby prose.

- [ ] **Step 3: Verify prompt safety and fallback order**

Run:

```powershell
rg -n "SHA256SUMS.txt|SHA-256|cargo build --release|auth.json|--diagnose|CodexPeek-Setup|github.com/lch5518/CodexPeek" README.md README.ko.md
```

Expected: both README files include the official source, Installer/checksum verification,
source fallback, authentication-file boundary, and diagnostic completion check.

### Task 3: Review and commit the documentation

**Files:**
- Modify: `README.md`
- Modify: `README.ko.md`

**Interfaces:**
- Consumes: completed Task 1 and Task 2 documentation
- Produces: one reviewed documentation commit

- [ ] **Step 1: Inspect the complete README diff**

Run:

```powershell
git diff -- README.md README.ko.md
```

Expected: only the requested installation alternatives are added; Installer, Portable,
privacy, and contributor development guidance remain intact.

- [ ] **Step 2: Check Markdown whitespace and repository status**

Run:

```powershell
git diff --check
git status --short
```

Expected: no whitespace errors and only the two README files plus this plan are changed.

- [ ] **Step 3: Commit the README implementation**

```powershell
git add README.md README.ko.md
git commit -m "docs: Add source and Codex installation guides"
```
