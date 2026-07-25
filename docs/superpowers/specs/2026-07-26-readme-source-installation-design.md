# README 소스 빌드 및 Codex 설치 프롬프트 설계

## 목적

`README.md`와 `README.ko.md`의 다운로드 및 실행 안내에 다음 두 경로를 추가한다.

1. 사용자가 PowerShell에서 소스를 직접 빌드하고 실행하는 방법
2. 사용자가 하나의 프롬프트를 Codex에 복사해 설치를 맡기는 방법

기존 Installer와 Portable 안내는 유지하며, 소스 빌드는 별도의 선택지로 제시한다.

## README 구성

두 README의 Installer 및 Portable 설명 다음에 같은 순서로 섹션을 추가한다.

- **Build from source / 소스에서 직접 빌드**
  - Rust 1.85 이상, Visual Studio 2022 C++ Build Tools, Windows SDK 요구 사항
  - 공식 저장소 clone
  - `cargo build --release`
  - `target\release\codex-peek.exe` 실행
  - `--diagnose` 실행 예시
- **Ask Codex to install / Codex에 설치 요청**
  - 그대로 복사할 수 있는 단일 프롬프트
  - 영어 README에는 영어 프롬프트, 한국어 README에는 한국어 프롬프트

기존 Development 섹션은 기여자를 위한 전체 검사 명령을 계속 제공한다. 새 소스 빌드
섹션은 일반 사용자가 앱을 빌드하고 실행하는 데 필요한 최소 명령만 제공한다.

## Codex 설치 프롬프트 동작

복사용 프롬프트는 Codex가 다음 순서로 작업하도록 요구한다.

1. Windows x64 환경과 Codex CLI 설치·로그인 상태를 확인한다.
2. 공식 저장소의 최신 GitHub Release에서 Installer와 `SHA256SUMS.txt`를 내려받는다.
3. Installer의 SHA-256을 매니페스트와 대조하고, 불일치 시 실행하지 않는다.
4. 해시가 맞으면 관리자 권한이 필요 없는 사용자별 설치를 진행한다.
5. Release 자산을 사용할 수 없을 때만 공식 저장소를 clone하고 release 빌드한다.
6. Git, Rust 또는 MSVC Build Tools 설치와 같이 시스템을 변경하는 선행 작업이 필요하면
   무엇을 설치할지 먼저 설명하고 사용자 승인을 받는다.
7. 실행 중인 기존 앱이나 관련 없는 프로세스를 임의로 종료하지 않는다.
8. `%USERPROFILE%\.codex\auth.json`은 존재 여부 외에 내용을 읽거나 출력하지 않는다.
9. 설치 또는 빌드 후 `codex-peek.exe --diagnose`를 실행하고 설치 위치, 버전, 진단 결과를
   사용자에게 요약한다.

## 검증

- 두 README에 소스 빌드 명령과 복사용 프롬프트가 모두 있는지 확인한다.
- 명령의 실행 파일명이 `codex-peek.exe`인지 확인한다.
- 프롬프트에 공식 저장소, SHA-256 검증, EXE 우선, 소스 fallback, 시스템 변경 승인,
  인증 파일 비접근이 모두 포함됐는지 확인한다.
- `git diff --check`로 Markdown 공백 오류를 검사한다.
