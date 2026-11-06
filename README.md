<p align="center"><img src="app/icons/icon.png" width="120" /></p>
<h1 align="center">Arcana</h1>
<p align="center">처음부터 직접 만든 AI 모델 <b>Arcana</b>로 돌아가는 PC 비서 — Rust 엔진 · 원클릭 실행 · 자동 업데이트</p>

> 앱 이름은 아직 정하는 중이라 임시로 Arcana를 쓰고 있어. 바꾸는 법은 아래 "앱 이름 바꾸기" 참고.

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
| 대화 | `배고파`, `오늘 너무 피곤해`, `롤 졌어`, `재밌는 얘기 해줘` |

지금은 **한국어만** 돼 (학습 데이터가 한국어라서). Arcana 말투는 합니다체.

프로그램 추가는 메뉴(☰) → **앱 목록 / 설정 파일 열기** → `apps`에 추가 → **설정 다시 읽기**. 재학습 필요 없어.

```json
"포토샵": { "open": "C:\\Program Files\\Adobe\\Adobe Photoshop 2025\\Photoshop.exe", "close": ["Photoshop.exe"], "aliases": ["ps"] }
```

---

## Arcana 모델

처음부터 직접 만든 모델. 구조만 공개 논문(BERT, GPT 계열)을 참고했고, 코드·학습 데이터·가중치는 전부 직접.

| 부분 | 하는 일 | 구조 | 크기 |
|---|---|---|---|
| Arcana 명령 이해 | 어떤 명령인지 + 앱 이름·시간 등 뽑기 | 인코더 (JointBERT 참고) | 약 85만 |
| Arcana 대화 | 대답을 글자 하나씩 직접 만들어냄 | 디코더 (GPT 계열 참고) | small 약 200만 (내장) |

명령이면 명령 이해 모델이 처리하고, 명령이 아닌 말은 대화 모델이 대답해.

### 대화 모델 키우기 (그래픽카드)
| 크기 | 파라미터 | 비고 |
|---|---|---|
| small | 약 200만 | CPU 15분, exe에 내장 |
| base | 약 2,600만 | 그래픽카드 권장 — `training/큰모델_학습하기.bat` 더블클릭 |
| large | 약 1억 | 그래픽카드 + 대량의 대화 데이터 필수 |

큰 모델은 exe에 넣지 않고 `%APPDATA%\Arcana\models\` 에 두면 앱이 자동으로 그걸 써.
**모델 크기보다 데이터가 중요해.** 직접 구한 대화 데이터는 jsonl로 넣을 수 있어:
```bash
python train_chat.py --size base --data my_chat.jsonl
# my_chat.jsonl 한 줄 = {"user": "배고파", "bot": "식사하실 시간인가 봅니다."}
```

## 앱 이름 바꾸기
1. `app/tauri.conf.json` 의 `productName`, `title`
2. `app/Cargo.toml` 의 `[[bin]] name` (exe 파일 이름)
3. `app/src/updater.rs` 의 `ASSET`, `.github/workflows/*.yml` 의 `Arcana.exe` (exe 이름과 같게)

UI 제목은 1번을 자동으로 따라가. 모델 이름(Arcana)은 그대로 유지돼.

## 구조

AI 회사들 방식 그대로 나눠져 있어: **학습은 파이썬, 실행은 Rust**.

```
Arcana/
├─ training/              🐍 학습 (Python + PyTorch)
│  ├─ data.py             학습 문장 생성기 — 새 명령은 여기에 템플릿 추가
│  ├─ model.py            트랜스포머 정의 (BERT/JointBERT 구조 참고)
│  ├─ train.py            명령 모델 학습 → 끝나면 자동으로 export
│  ├─ chat_data.py        대화 데이터 생성기 (합니다체) — 새 대화 주제는 여기에
│  ├─ chat_model.py       대화 모델 정의 + 크기 설정(PRESETS)
│  ├─ train_chat.py       대화 모델 학습 (--size small/base/large)
│  └─ export.py           Rust가 읽는 형식으로 내보내기
├─ models/                🧠 학습 결과 (exe 안에 내장됨)
│  ├─ arcana-nlu.safetensors   가중치 (HuggingFace 표준 형식)
│  ├─ arcana-nlu.json          설정 + 글자 사전 + 의도 목록
│  ├─ arcana-chat.*            대화 모델
│  └─ golden*.json             Rust = PyTorch 검사용
├─ crates/
│  ├─ arcana-core/        🦀 Arcana 모델 추론 엔진 (외부 AI 라이브러리 없이 직접 구현, KV 캐시)
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
- **모델 교체**: `arcana_core::Brain`(명령), `arcana_core::Talker`(대화) 트레이트만 구현하면 갈아끼울 수 있음. 앱/스킬은 안 건드림.
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
3. 커밋·푸시 후 GitHub → **Actions → Release → Run workflow** (또는 `git tag v0.2.0 && git push origin v0.2.0`)

→ GitHub Actions가 윈도우에서 빌드해서 릴리스에 `Arcana.exe` 올림 → 모든 사용자 Arcana가 다음 실행 때 자동 업데이트.

## 한계
- 한국어만 가능
- 대화는 학습 데이터에 있는 주제(인사, 감정, 학교, 게임, 음식 등) 안에서만 자연스러움
- 지식 질문(역사, 계산 등)은 못 함 → 검색 명령으로 대신
