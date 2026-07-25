# 설치 문서 설계

## 목적

소스 빌드 없이 Codex Usage Monitor를 처음 설치하는 사용자가 Installer와 Portable 중
적절한 방식을 선택하고, 설치·실행·검증·제거까지 안전하게 완료할 수 있게 한다.

## 문서 구조

### `README.ko.md`

README의 `다운로드 및 실행` 절은 빠른 시작 안내로 유지한다.

- Codex CLI 설치와 로그인이라는 사전 조건을 먼저 알린다.
- 일반 사용자에게 Installer를 권장한다.
- Installer와 Portable의 차이를 짧게 비교한다.
- 각각의 최소 실행 절차를 번호로 제시한다.
- 코드 서명 전 SmartScreen 경고와 SHA-256 확인 필요성을 알린다.
- 자세한 절차는 `docs/INSTALL.md`로 연결한다.

영문 `README.md`도 같은 정보 구조와 배포 파일명을 유지하되, 상세 설치 문서는 한국어
문서임을 링크 문구에서 분명히 한다.

### `docs/INSTALL.md`

상세 문서는 다음 순서로 구성한다.

1. Windows 및 Codex CLI 사전 조건 확인
2. Installer 다운로드, SmartScreen 확인, 설치와 첫 실행
3. Portable ZIP 다운로드, 압축 해제와 실행
4. `SHA256SUMS.txt`를 이용한 SHA-256 검증
5. Windows 자동 시작 설정과 설정 파일 위치
6. 새 버전으로 업데이트하는 방법
7. 제거되는 항목과 보존되는 설정
8. CLI 누락, 로그인 만료, SmartScreen, 위젯 미표시 문제 해결

## 내용의 단일 기준

- 최신 배포 위치는 GitHub Releases의 `latest` URL을 사용한다.
- 파일명은 `<version>` 표기로 특정 버전에 고정하지 않는다.
- 설치 경로, 설정 경로, 로그 경로, 자동 시작 레지스트리 이름은 현재 구현과
  `docs/RELEASE_CHECKLIST.md`를 기준으로 한다.
- README에는 상세 명령을 중복하지 않고 `docs/INSTALL.md`로 연결해 유지보수 시 내용이
  어긋나지 않게 한다.

## 검증

- README의 모든 상대 링크가 저장소 안의 실제 파일을 가리키는지 확인한다.
- Installer와 Portable 파일명이 릴리스 워크플로 계약과 일치하는지 확인한다.
- PowerShell 예제는 Windows PowerShell과 PowerShell 7에서 사용할 수 있는 명령만 쓴다.
- `git diff --check`로 공백 오류를 검사한다.
