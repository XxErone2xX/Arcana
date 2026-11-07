@echo off
chcp 65001 > nul
cd /d "%~dp0"
echo ============================================
echo  위키백과 한 번에 받기 (위키미디어 공식 덤프)
echo  한국어 위키백과 전체(약 1GB)를 받으면서 문서 본문만 뽑습니다.
echo  인터넷 속도에 따라 10~40분 걸립니다.
echo ============================================
py --version > nul 2>&1 || (echo Python이 없습니다. python.org 에서 설치해 주세요 ^(py 런처 포함^). & pause & exit /b 9)
py import_wiki_dump.py %*
echo.
echo 다음 학습(/Train 또는 자동 학습) 때 1단계 재료로 쓰입니다.
pause
