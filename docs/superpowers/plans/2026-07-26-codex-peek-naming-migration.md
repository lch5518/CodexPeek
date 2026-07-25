# CodexPeek Naming Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 공개 패키지·실행 파일·릴리스 자산·로그 이름을 CodexPeek로 바꾸면서 기존 설치 경로, 설정, AppId와 자동 시작 상태를 유지한다.

**Architecture:** Cargo 패키지는 `codex-peek`로 바꾸고 라이브러리 crate 이름은 `codex_usage_monitor`로 고정한다. 동일한 Inno Setup AppId와 내부 Windows 식별자를 유지하며 installer가 이전 실행 파일을 삭제하고 존재하는 자동 시작 값만 새 실행 파일 경로로 갱신한다.

**Tech Stack:** Rust 2021/MSRV 1.85, Cargo, PowerShell 7, Inno Setup 6, GitHub Actions

## Global Constraints

- 릴리스 버전은 `0.1.3`, 태그 형식은 `v0.1.3`이다.
- 새 Cargo 패키지명은 `codex-peek`, 새 실행 파일명은 `codex-peek.exe`이다.
- 새 자산명은 `CodexPeek-Setup-v0.1.3-x64.exe`, `codex-peek-v0.1.3-windows-x86_64-portable.zip`, `SHA256SUMS.txt`이다.
- 새 진단 로그 경로는 `%TEMP%\codex-peek.log`이다.
- 화면 표시명 `Codex Usage Monitor`는 유지한다.
- `%APPDATA%\CodexUsageMonitor`, `%LOCALAPPDATA%\Programs\CodexUsageMonitor`, AppId, mutex, 자동 시작 값 이름은 유지한다.
- `%USERPROFILE%\.codex\auth.json`의 내용은 읽거나 마이그레이션하지 않는다.
- 공개한 `v0.1.2` 태그와 자산은 이동하거나 교체하지 않는다.

---

### Task 1: 릴리스 자산 이름 계약과 Inno Setup 마이그레이션

**Files:**
- Modify: `tests/release_packaging.ps1`
- Modify: `scripts/package-windows-release.ps1`
- Move: `packaging/windows/CodexUsageMonitor.iss` to `packaging/windows/CodexPeek.iss`

**Interfaces:**
- Consumes: `-Version`, `-Executable`, `-OutputDirectory`, `-IsccPath` 패키징 인자
- Produces: `CodexPeek-Setup-v$Version-x64.exe`, `codex-peek-v$Version-windows-x86_64-portable.zip`, `SHA256SUMS.txt`

- [ ] **Step 1: 새 자산 이름과 installer 호환 계약을 테스트에 먼저 쓴다**

`tests/release_packaging.ps1`의 fixture와 기대값을 다음 이름으로 바꾼다.

```powershell
$fixtureExe = Join-Path $testRoot "codex-peek.exe"
$installerDefinition = Join-Path $repositoryRoot "packaging/windows/CodexPeek.iss"
$portableName = "codex-peek-v1.2.3-windows-x86_64-portable.zip"
$installerName = "CodexPeek-Setup-v1.2.3-x64.exe"
```

Portable 기대 파일은 `codex-peek.exe`로 바꾸고 다음 installer 문자열을 필수 계약에
추가한다.

```powershell
'"codex-usage-monitor.exe"'
'"codex-peek.exe"'
"CurStepChanged"
"RegQueryStringValue"
"RegWriteStringValue"
```

- [ ] **Step 2: 패키징 계약 테스트가 실패하는지 확인한다**

Run:

```powershell
pwsh -NoProfile -File tests/release_packaging.ps1
```

Expected: 기존 패키저가 이전 installer 또는 ZIP 이름을 생성해 FAIL한다.

- [ ] **Step 3: 패키징 스크립트 이름을 구현한다**

`scripts/package-windows-release.ps1`을 다음 계약으로 바꾼다.

```powershell
$installerScript = Join-Path $repositoryRoot "packaging/windows/CodexPeek.iss"
$portableName = "codex-peek-v$Version-windows-x86_64-portable.zip"
$installerName = "CodexPeek-Setup-v$Version-x64.exe"
Copy-Item -LiteralPath $executablePath `
    -Destination (Join-Path $stagingRoot "codex-peek.exe")
```

- [ ] **Step 4: Inno Setup 소스를 이동하고 업그레이드 처리를 구현한다**

`packaging/windows/CodexPeek.iss`에서 다음 값을 사용한다.

```text
#define AppExeName "codex-peek.exe"
OutputBaseFilename=CodexPeek-Setup-v{#AppVersion}-x64
```

`[InstallDelete]`로 `{app}\codex-usage-monitor.exe`만 삭제한다. `CurStepChanged`의
`ssPostInstall` 처리에서 `CodexUsageMonitor` Run 값이 존재할 때만 다음 고정 명령으로
갱신한다.

```text
"{app}\codex-peek.exe" --startup
```

기존 `CurUninstallStepChanged`의 Run 값 삭제와 AppId·설치 경로·mutex는 유지한다.

- [ ] **Step 5: 패키징 계약 테스트를 통과시킨다**

Run:

```powershell
pwsh -NoProfile -File tests/release_packaging.ps1
```

Expected: 새 자산명, ZIP 내용, SHA-256, 이전 실행 파일 삭제와 Run 값 마이그레이션 계약이 PASS한다.

- [ ] **Step 6: Task 1을 커밋한다**

```powershell
git add tests/release_packaging.ps1 scripts/package-windows-release.ps1 packaging/windows
git commit -m "build: Rename Windows release assets"
```

### Task 2: Cargo 패키지, 실행 파일과 로그 이름

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `build.rs`
- Modify: `src/main.rs`
- Modify: `src/diagnostics.rs`
- Test: `tests/diagnostics_runtime.rs`

**Interfaces:**
- Consumes: Cargo 패키지 metadata와 Windows version resource
- Produces: Cargo 패키지 `codex-peek` v0.1.3, binary `codex-peek.exe`, library crate `codex_usage_monitor`, log `%TEMP%\codex-peek.log`

- [ ] **Step 1: 진단 로그 기본 이름 테스트를 추가한다**

`src/diagnostics.rs`의 기존 내부 테스트 모듈이 private `path` 필드에 접근할 수 있으므로
다음 테스트를 추가한다.

```rust
#[test]
fn default_logger_uses_codex_peek_log_name() {
    assert_eq!(
        DiagnosticLogger::new().path,
        std::env::temp_dir().join("codex-peek.log")
    );
}
```

- [ ] **Step 2: 로그 이름 테스트 실패를 확인한다**

Run:

```powershell
cargo test --test diagnostics_runtime
```

Expected: 구현이 아직 `codex-usage-monitor.log`를 사용해 FAIL한다.

- [ ] **Step 3: Cargo 이름과 버전을 변경한다**

`Cargo.toml`에 다음 package와 library 경계를 사용한다.

```toml
[package]
name = "codex-peek"
version = "0.1.3"

[lib]
name = "codex_usage_monitor"
path = "src/lib.rs"
```

`Cargo.lock`의 루트 package 이름과 버전도 `codex-peek`/`0.1.3`으로 맞춘다.

- [ ] **Step 4: Windows resource와 진단 이름을 변경한다**

`build.rs`의 icon 임시 이름, `InternalName`, `OriginalFilename`을 각각
`codex-peek.ico`, `codex-peek`, `codex-peek.exe`로 바꾼다. `src/main.rs`의 stderr
접두사는 `codex-peek:`로, `src/diagnostics.rs`의 기본 로그는 `codex-peek.log`로 바꾼다.

- [ ] **Step 5: Cargo metadata와 관련 테스트를 검증한다**

Run:

```powershell
$metadata = cargo metadata --no-deps --format-version 1 | ConvertFrom-Json
if ($metadata.packages[0].name -ne "codex-peek") { throw "wrong package name" }
if ($metadata.packages[0].version -ne "0.1.3") { throw "wrong package version" }
if ($metadata.packages[0].targets.name -notcontains "codex_usage_monitor") { throw "library target missing" }
if ($metadata.packages[0].targets.name -notcontains "codex-peek") { throw "binary target missing" }
cargo test --test diagnostics_runtime
cargo test --test build_resources
```

Expected: metadata 네 항목과 두 테스트 target이 PASS한다.

- [ ] **Step 6: Task 2를 커밋한다**

```powershell
git add Cargo.toml Cargo.lock build.rs src/main.rs src/diagnostics.rs tests/diagnostics_runtime.rs
git commit -m "build: Rename package to codex-peek"
```

### Task 3: Installer lifecycle 마이그레이션 검증

**Files:**
- Modify: `tests/installer_smoke.ps1`
- Consume: `packaging/windows/CodexPeek.iss`
- Consume: `target/release/codex-peek.exe`

**Interfaces:**
- Consumes: 이전 실행 파일 fixture와 기존 `CodexUsageMonitor` Run 값
- Produces: 설치 후 이전 exe 제거, 새 exe 설치, Run 값 마이그레이션, 제거 후 설정 보존 검증

- [ ] **Step 1: smoke test를 새 이름과 업그레이드 fixture로 바꾼다**

다음 경로와 process 이름을 사용한다.

```powershell
$installedExecutable = Join-Path $installDirectory "codex-peek.exe"
$legacyExecutable = Join-Path $installDirectory "codex-usage-monitor.exe"
Get-Process -Name "codex-peek","codex-usage-monitor"
```

설치 전 `$legacyExecutable` fixture와 다음 Run 값을 만든다.

```powershell
"`"$legacyExecutable`" --startup"
```

설치 후 legacy exe가 없고 Run 값이 다음과 정확히 같은지 검사한다.

```powershell
"`"$installedExecutable`" --startup"
```

제거 후 새 exe·바로 가기·uninstall entry·Run 값은 없고 설정 sentinel은 남아야 한다.
finally에서는 테스트 전 Run 값을 복구한다.

- [ ] **Step 2: release executable과 installer를 실제 빌드한다**

Run:

```powershell
cargo build --release
pwsh -NoProfile -File scripts/package-windows-release.ps1 `
  -Version 0.1.3 `
  -Executable target\release\codex-peek.exe `
  -OutputDirectory target\release-assets-v0.1.3-validation `
  -IsccPath "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
```

Expected: 세 v0.1.3 자산이 생성되고 Inno Setup 컴파일이 성공한다.

- [ ] **Step 3: 격리된 사용자에서 installer lifecycle을 검증한다**

Run on an isolated Windows runner with no monitor process:

```powershell
pwsh -NoProfile -File tests/installer_smoke.ps1 `
  -Version 0.1.3 `
  -Installer target\release-assets-v0.1.3-validation\CodexPeek-Setup-v0.1.3-x64.exe `
  -AllowUserProfileMutation
```

Expected: legacy exe/Run 마이그레이션, 새 설치, 제거와 설정 보존이 PASS한다.

- [ ] **Step 4: Task 3을 커밋한다**

```powershell
git add tests/installer_smoke.ps1
git commit -m "test: Cover CodexPeek installer migration"
```

### Task 4: Release CI와 사용자 문서

**Files:**
- Modify: `.github/workflows/release.yml`
- Modify: `README.md`
- Modify: `README.ko.md`
- Modify: `docs/INSTALL.md`
- Modify: `docs/RELEASE_CHECKLIST.md`
- Modify: `SECURITY.md`
- Modify: `AGENTS.md`

**Interfaces:**
- Consumes: Task 1~3의 실제 파일명과 v0.1.3 metadata
- Produces: CI와 사용자가 공유하는 단일 CodexPeek 배포 이름 계약

- [ ] **Step 1: release workflow 이름을 변경한다**

workflow의 executable, installer, ZIP 기대값을 다음으로 바꾼다.

```text
target\release\codex-peek.exe
CodexPeek-Setup-v$version-x64.exe
codex-peek-v$version-windows-x86_64-portable.zip
```

Cargo metadata 검사에는 package name `codex-peek`도 추가한다. 기존 release 거부와
no-clobber 정책은 유지한다.

- [ ] **Step 2: 문서의 공개 이름과 명령을 변경한다**

모든 사용자 문서에서 executable·ZIP·installer·로그를 다음 이름으로 통일한다.

```text
codex-peek.exe
codex-peek-v<version>-windows-x86_64-portable.zip
CodexPeek-Setup-v<version>-x64.exe
%TEMP%\codex-peek.log
```

설치 경로와 설정 경로의 `CodexUsageMonitor`는 호환성 식별자이므로 유지한다.
`docs/INSTALL.md`의 Portable 업데이트 절에는 새 폴더 압축 해제를 권장하고 기존 폴더를
재사용하면 이전 이름 exe를 제거해야 한다고 의미만 설명하되 공개 legacy 파일명을
반복하지 않는다.

- [ ] **Step 3: repository instruction을 실제 계약과 맞춘다**

`AGENTS.md`의 패키지/실행 파일, 진단 명령과 로그 경로를 새 이름으로 바꾼다. 설정 경로와
보안 경계는 변경하지 않는다.

- [ ] **Step 4: 공개 legacy 이름 잔여를 검사한다**

Run:

```powershell
rg -n -F "codex-usage-monitor" . --glob "!target/**" --glob "!.git/**"
```

Expected: installer의 `[InstallDelete]`, installer smoke migration fixture, 이름
마이그레이션 설계·계획처럼 호환성 목적의 위치에서만 검색된다.

- [ ] **Step 5: Task 4를 커밋한다**

```powershell
git add .github/workflows/release.yml README.md README.ko.md docs/INSTALL.md docs/RELEASE_CHECKLIST.md SECURITY.md AGENTS.md
git commit -m "docs: Adopt CodexPeek artifact names"
```

### Task 5: 전체 검증과 릴리스 준비 상태

**Files:**
- Verify: all changed files

**Interfaces:**
- Consumes: Task 1~4의 새 이름 계약
- Produces: v0.1.3 태그 전 검증된 clean working tree

- [ ] **Step 1: PowerShell과 Rust 검증을 실행한다**

Run:

```powershell
pwsh -NoProfile -File tests/release_packaging.ps1
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
```

Expected: 모든 명령이 exit code 0으로 끝나고 `target\release\codex-peek.exe`가 존재한다.

- [ ] **Step 2: 실제 자산 이름과 해시를 검증한다**

새 빈 output directory에서 package script를 실행하고 다음 세 파일만 존재하는지 검사한다.

```text
CodexPeek-Setup-v0.1.3-x64.exe
SHA256SUMS.txt
codex-peek-v0.1.3-windows-x86_64-portable.zip
```

`SHA256SUMS.txt`의 두 항목을 `Get-FileHash -Algorithm SHA256` 결과와 비교한다.

- [ ] **Step 3: 최종 diff와 상태를 검사한다**

Run:

```powershell
git diff --check
git status --short
git log -6 --oneline --decorate
```

Expected: 공백 오류가 없고 계획된 커밋만 HEAD 위에 존재한다. `v0.1.3` 태그는 별도
사용자 요청 전에는 만들거나 푸시하지 않는다.
