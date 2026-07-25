# Installation Documentation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 소스 빌드 없이 Installer 또는 Portable 배포 파일로 앱을 설치하는 빠른 안내와 상세 한국어 가이드를 제공한다.

**Architecture:** `README.ko.md`와 `README.md`는 릴리스 페이지에서 실행까지의 최소 절차만 제공한다. `docs/INSTALL.md`는 설치, 검증, 업데이트, 제거, 문제 해결의 단일 상세 기준이 되며 README에서 이 문서로 연결한다.

**Tech Stack:** GitHub-flavored Markdown, PowerShell 5.1 이상, GitHub Releases, Inno Setup 배포 파일

## Global Constraints

- 대상은 Windows 10/11 x64이다.
- 설치 전에 지원되는 Codex CLI가 설치되고 로그인돼 있어야 한다.
- 최신 배포 위치는 `https://github.com/lch5518/CodexPeek/releases/latest`이다.
- Installer 파일명은 `CodexPeek-Setup-v<version>-x64.exe`이다.
- Portable 파일명은 `codex-peek-v<version>-windows-x86_64-portable.zip`이다.
- 설정은 `%APPDATA%\CodexUsageMonitor\settings.json`, 로그는 `%TEMP%\codex-peek.log`에 유지한다.
- 코드 서명 전에는 SmartScreen 경고 가능성과 `SHA256SUMS.txt` 검증 방법을 명시한다.
- 동일한 상세 명령을 README와 설치 문서에 중복하지 않는다.

---

### Task 1: 상세 한국어 설치 가이드

**Files:**
- Create: `docs/INSTALL.md`
- Reference: `docs/RELEASE_CHECKLIST.md`
- Reference: `packaging/windows/CodexUsageMonitor.iss`

**Interfaces:**
- Consumes: 릴리스 자산 이름, 설치 경로, 설정·로그 경로, 제거 정책
- Produces: README에서 연결할 상세 설치 문서 `docs/INSTALL.md`

- [ ] **Step 1: 설치 계약을 확인한다**

Run:

```powershell
rg -n "OutputBaseFilename|DefaultDirName|PrivilegesRequired|RegDeleteValue|portable.zip|SHA256SUMS" packaging/windows/CodexUsageMonitor.iss scripts/package-windows-release.ps1 docs/RELEASE_CHECKLIST.md
```

Expected: 사용자별 설치 경로, 관리자 권한 불필요, 자동 시작 제거, 세 릴리스 자산 이름이 확인된다.

- [ ] **Step 2: 상세 설치 문서를 작성한다**

`docs/INSTALL.md`에 다음 순서와 내용을 작성한다.

1. 요구 사항과 `codex --version`/`codex login` 확인
2. Installer 권장 대상과 번호가 있는 설치 순서
3. Portable 권장 대상과 번호가 있는 실행 순서
4. 두 배포 파일에 공통으로 적용되는 SHA-256 PowerShell 검증
5. 설치 경로, 설정 경로, 로그 경로, Windows 자동 시작 정책
6. 실행 중 앱을 종료한 뒤 새 버전을 덮어 설치하거나 Portable 파일을 교체하는 업데이트 절차
7. 제거 시 삭제되는 앱·바로 가기·자동 시작과 보존되는 설정·로그
8. SmartScreen, CLI 누락, 로그인 만료, 위젯 미표시 문제 해결표

- [ ] **Step 3: 상세 문서 계약을 검사한다**

Run:

```powershell
rg -n "CodexPeek-Setup-v<version>-x64.exe|codex-peek-v<version>-windows-x86_64-portable.zip|SHA256SUMS.txt|%APPDATA%|%TEMP%|SmartScreen|--diagnose" docs/INSTALL.md
```

Expected: 모든 필수 설치·검증·진단 항목이 검색된다.

### Task 2: README 빠른 설치 안내

**Files:**
- Modify: `README.ko.md`
- Modify: `README.md`
- Consume: `docs/INSTALL.md`

**Interfaces:**
- Consumes: Task 1의 상세 설치 문서
- Produces: 릴리스 페이지에서 첫 실행까지 안내하는 한·영 빠른 시작

- [ ] **Step 1: 한국어 빠른 설치 절을 정리한다**

`README.ko.md`의 `다운로드 및 실행` 절을 다음 구조로 바꾼다.

- Codex CLI 설치·로그인 사전 확인
- Installer 3단계 빠른 시작
- Portable 3단계 빠른 시작
- 두 방식의 차이와 공유 설정 한 문단
- SmartScreen 및 해시 검증 요약
- `[상세 설치 가이드](docs/INSTALL.md)` 링크

- [ ] **Step 2: 영문 README를 같은 정보 구조로 맞춘다**

`README.md`에 Installer와 Portable의 동일한 3단계 절차, 공유 설정, unsigned 경고를
유지하고 상세 링크를 `Detailed installation guide (Korean)`으로 표시한다.

- [ ] **Step 3: 중복과 오래된 문구를 검사한다**

Run:

```powershell
rg -n "There is no installer|설치 프로그램.*제공하지|target.release|rustup-init" README.md README.ko.md docs/INSTALL.md
```

Expected: 검색 결과가 없다.

### Task 3: 문서 검증과 커밋

**Files:**
- Verify: `README.ko.md`
- Verify: `README.md`
- Verify: `docs/INSTALL.md`

**Interfaces:**
- Consumes: Task 1과 Task 2의 문서
- Produces: 링크와 배포 계약이 검증된 설치 문서 커밋

- [ ] **Step 1: 상대 링크 대상과 파일명을 검증한다**

Run:

```powershell
if (-not (Test-Path -LiteralPath docs/INSTALL.md -PathType Leaf)) { throw "docs/INSTALL.md missing" }
rg -n "docs/INSTALL.md" README.md README.ko.md
rg -n "CodexPeek-Setup-v<version>-x64.exe|codex-peek-v<version>-windows-x86_64-portable.zip" README.md README.ko.md docs/INSTALL.md
```

Expected: 두 README의 링크와 세 문서의 동일한 배포 파일명이 확인된다.

- [ ] **Step 2: Markdown 공백 오류를 검사한다**

Run:

```powershell
git diff --check
git status --short
```

Expected: `git diff --check`가 성공하고 설치 문서 관련 파일만 변경돼 있다.

- [ ] **Step 3: 문서 변경을 커밋한다**

```powershell
git add README.md README.ko.md docs/INSTALL.md
git commit -m "docs: Add installation guides"
```
