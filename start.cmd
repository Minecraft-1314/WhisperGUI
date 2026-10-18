@echo off
setlocal
set "APP=%~dp0dist\WhisperGUI-win-x64\WhisperGUI.exe"
if not exist "%APP%" (
  echo Release build not found. Run build.cmd first.
  exit /b 1
)
start "" "%APP%"
exit /b 0
