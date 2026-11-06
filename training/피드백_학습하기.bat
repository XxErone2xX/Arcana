@echo off
chcp 65001 > nul
cd /d "%~dp0"
echo ==============================================
echo   Arcana 3단계: 피드백 학습 (DPO)
echo   앱에서 누른 좋아요/싫어요, 가르친 대답으로
echo   대화 모델을 다시 학습합니다. (몇 분 걸림)
echo ==============================================
echo.
python --version > nul 2>&1 || (echo Python이 없습니다. python.org 에서 설치해 주세요 ^(Add to PATH 체크^). & pause & exit /b)
python -c "import torch" > nul 2>&1 || python -m pip install torch numpy
python -c "import numpy" > nul 2>&1 || python -m pip install numpy
python train_feedback.py %*
echo.
echo Arcana 앱을 껐다 켜면 새 모델이 적용됩니다.
pause
