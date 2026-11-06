@echo off
chcp 65001 > nul
cd /d "%~dp0"
echo ==============================================
echo   Arcana 대화 모델 (base, 약 2,600만) 학습
echo   그래픽카드(NVIDIA) 사용, 1시간 안팎 걸림
echo ==============================================
echo.

python --version > nul 2>&1 || (echo Python이 없습니다. python.org 에서 설치해 주세요 ^(Add to PATH 체크^). & pause & exit /b)

echo [1/3] 그래픽카드용 PyTorch 설치 확인
python -c "import torch,sys; sys.exit(0 if torch.cuda.is_available() else 1)" > nul 2>&1 || python -m pip install torch --index-url https://download.pytorch.org/whl/cu126

echo [2/3] 학습 시작
python train_chat.py --size base %*
if errorlevel 1 (echo 학습 실패 & pause & exit /b)

echo [3/3] 앱에 적용
mkdir "%APPDATA%\Arcana\models" 2> nul
copy /Y out\base\arcana-chat.safetensors "%APPDATA%\Arcana\models\" > nul
copy /Y out\base\arcana-chat.json "%APPDATA%\Arcana\models\" > nul
echo 완료! 앱을 다시 켜면 큰 모델로 대화합니다.
pause
