<p align="center"><img src="app/icons/icon.png" width="120" /></p>
<h1 align="center">Arcana</h1>
<p align="center">처음부터 직접 만든 PC 비서 AI — Rust 엔진 · 원클릭 실행 · 자동 업데이트</p>

---

## 실행하기

[Releases](https://github.com/XxErone2xX/Arcana/releases/latest)에서 **Arcana.exe** 받아서 더블클릭. 끝.

- 설치 필요 없음 (파이썬도 필요 없음)
- 켤 때마다 새 버전 있으면 자동으로 업데이트됨
- 설정, 메모는 `%APPDATA%\Arcana` 에 저장됨

## 할 수 있는 말

| 기능 | 예시 |
|---|---|
| 프로그램 켜기/끄기 | `디코 켜줘`, `크롬 꺼`, `블렌더 실행해줘` |
| 타이머 | `라면 3분 타이머`, `1시간 30분 뒤에 알려줘` |
| 시간·날짜 | `지금 몇 시야`, `오늘 며칠이야` |
| 검색 | `파이썬 딕셔너리 사용법 검색해줘` |
| 유튜브 | `유튜브에서 침착맨 틀어줘`, `로파이 틀어줘` |
| 볼륨 | `소리 키워`, `볼륨 줄여`, `음소거` |
| 화면 | `스샷`, `컴 잠가` |
| 전원 | `컴퓨터 꺼줘` (물어보고 끔) |
| 메모 | `내일 체육복 챙기기 메모해줘`, `메모 보여줘`, `메모 다 지워` |

프로그램 추가는 메뉴(☰) → **앱 목록 / 설정 파일 열기** → `apps`에 추가 → **설정 다시 읽기**. 재학습 필요 없어.

```json
"포토샵": { "open": "C:\\Program Files\\Adobe\\Adobe Photoshop 2025\\Photoshop.exe", "close": ["Photoshop.exe"], "aliases": ["ps"] }
```

---

## 구조

AI 회사들 방식 그대로 나눠져 있어: **학습은 파이썬, 실행은 Rust**.

```
Arcana/
├─ training/              🐍 학습 (Python + PyTorch)
│  ├─ data.py             학습 문장 생성기 — 새 명령은 여기에 템플릿 추가
│  ├─ model.py            트랜스포머 정의 (BERT/JointBERT 구조 참고)
│  ├─ train.py            학습 → 끝나면 자동으로 export
│  └─ export.py           Rust가 읽는 형식으로 내보내기
├─ models/                🧠 학습 결과 (exe 안에 내장됨)
│  ├─ arcana-nlu.safetensors   가중치 (HuggingFace 표준 형식)
│  ├─ arcana-nlu.json          설정 + 글자 사전 + 의도 목록
│  └─ golden.json              Rust = PyTorch 검사용
├─ crates/
│  ├─ arcana-core/        🦀 두뇌: 트랜스포머 추론 엔진 (외부 AI 라이브러리 없이 직접 구현)
│  └─ arcana-skills/      🦀 손발: 스킬 시스템 + 내장 스킬들
├─ app/                   🦀 Tauri 앱: 창, 자동 업데이트, 두뇌↔UI 연결
├─ ui/                    🎨 화면 (HTML/CSS/JS)
│  ├─ css/theme.css       색·글꼴·모서리 장식 — 꾸밀 땐 여기부터
│  └─ js/api.js           Rust랑 통신하는 부분
├─ docs/releases/         릴리스 설명
└─ .github/workflows/     자동 빌드 + 릴리스
```

### 흐름
```
"라면 3분 타이머"
   │  arcana-core: 글자 → 트랜스포머 4층 → 의도 set_timer (96%) + 슬롯 TIME="3분"
   ▼
arcana-skills: set_timer 담당 스킬 찾기 → 3분 뒤 알림 예약
   ▼
app → ui: 대답 표시, 3분 뒤 알림음 + 창 앞으로
```

### 미래 확장 포인트
- **두뇌 교체**: `arcana_core::Brain` 트레이트만 구현하면 더 큰 모델, 생성형 모델(대화형)로 갈아끼울 수 있음. 앱/스킬은 안 건드림.
- **스킬 추가**: `crates/arcana-skills/src/skills/` 에 파일 하나 + `Skill` 트레이트 구현.
- **엔진 가속**: `arcana-core/src/tensor.rs` 만 바꾸면 SIMD·멀티스레드·GPU·양자화 적용 가능.
- **모델 형식 버전**: `format_version` 으로 관리 → 옛날 앱이 새 모델을 잘못 읽는 일 없음.
- **설정 호환**: 설정 항목 추가해도 예전 config.json 안 깨짐.
- **UI**: 웹 기술이라 브라우저에서 `ui/index.html` 그냥 열어도 미리보기 됨 (가짜 응답).

---

## 개발

### 필요한 것
- [Rust](https://rustup.rs) (exe 빌드)
- Python 3.10+ & `pip install -r training/requirements.txt` (재학습할 때만)

### 자주 쓰는 명령
```bash
cargo run -p arcana                               # 앱 개발 모드 실행
cargo run -p arcana-skills --example cli          # 터미널에서 두뇌+스킬만 테스트
cargo test -p arcana-core -p arcana-skills        # 테스트
cargo build -p arcana --release                   # target/release/Arcana.exe

cd training && python train.py                    # 재학습 (끝나면 models/ 자동 갱신)
```

### 새 명령(의도) 추가
1. `training/data.py` 의 `INTENTS` 에 의도 이름 + 문장 템플릿 추가
2. `python train.py` (CPU 5~6분)
3. `crates/arcana-skills/src/skills/` 에 스킬 만들고 `skills/mod.rs` 에 등록
4. 앱 메뉴 → 정보에 **스킬 없음** 이 안 뜨면 연결 완료

### 릴리스 (자동 업데이트 배포)
1. `Cargo.toml` 의 `version` 올리기 (예: `0.2.0`)
2. `docs/releases/v0.2.0.md` 작성
3. `git tag v0.2.0 && git push origin v0.2.0`

→ GitHub Actions가 윈도우에서 빌드해서 릴리스에 `Arcana.exe` 올림 → 모든 사용자 Arcana가 다음 실행 때 자동 업데이트.

## 성능
- 모델: 약 85만 파라미터 (4층 · 128차원 · 헤드 4개), 글자 단위
- 학습 때 안 본 문장 38개 중 37개 정답
- 한계: 학습 안 한 종류의 말(잡담, 지식 질문)은 못 함
