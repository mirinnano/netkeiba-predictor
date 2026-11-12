@echo off
rem Start the app built by "npm run tauri build" (release) or "npm run tauri dev" (debug).
set EXE=%~dp0src-tauri\target\release\netkeiba-predictor.exe
if not exist "%EXE%" set EXE=%~dp0src-tauri\target\debug\netkeiba-predictor.exe
if not exist "%EXE%" (
  echo App is not built yet. Run "npm install" and then "npm run tauri build" in this folder.
  pause
  exit /b 1
)
start "" "%EXE%"
