@echo off
chcp 65001 > nul
cd /d "%~dp0"
echo ==============================================
echo   Arcana: 공부한 문서로 대화 모델 학습
echo   1단계 공책 문서로 한국어 배우기
echo   2단계 대화 데이터로 말하는 법 배우기
echo   (그래픽카드 있으면 큰 모델, 없으면 작은 모델)
echo ==============================================
echo.
py --version > nul 2>&1 || (echo Python이 없습니다. python.org 에서 설치해 주세요 ^(py 런처 포함^). & (if not defined ARCANA_AUTO pause) & exit /b 9)
py -c "import torch, numpy" > nul 2>&1 || (
  nvidia-smi > nul 2>&1 && py -m pip install torch numpy --index-url https://download.pytorch.org/whl/cu126 || py -m pip install torch numpy
)
py pretrain_notes.py %*
set RC=%ERRORLEVEL%
rem 자동 학습(앱이 몰래 실행)일 때는 기다리지 않고 결과만 알려 줌
if defined ARCANA_AUTO exit /b %RC%
echo.
echo 끝나면 Arcana 학습 탭에서 /Reload 를 입력해 주세요.
pause
