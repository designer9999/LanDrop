@echo off
setlocal
title LanDrop Dev
cd /d "%~dp0"

where node >nul 2>nul
if errorlevel 1 (
  echo Node.js was not found in PATH.
  echo Install Node.js or open this from a terminal where npm works.
  pause
  exit /b 1
)

where npm >nul 2>nul
if errorlevel 1 (
  echo npm was not found in PATH.
  echo Install Node.js or open this from a terminal where npm works.
  pause
  exit /b 1
)

if not exist "node_modules\@tauri-apps\cli-win32-x64-msvc" (
  echo Missing Windows Tauri native package. Repairing node_modules...
  call npm install
  if errorlevel 1 (
    echo.
    echo npm install failed.
    pause
    exit /b 1
  )
)

if not exist "node_modules\@rolldown\binding-win32-x64-msvc" (
  echo Missing Windows Vite native package. Repairing node_modules...
  call npm install
  if errorlevel 1 (
    echo.
    echo npm install failed.
    pause
    exit /b 1
  )
)

set "LANDROP_DEV_PORT="
for /f %%P in ('powershell -NoProfile -ExecutionPolicy Bypass -Command "$used = Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue | ForEach-Object { $_.LocalPort }; for ($p = 1420; $p -le 1499; $p++) { if ($used -notcontains $p) { Write-Output $p; exit 0 } }; exit 1"') do set "LANDROP_DEV_PORT=%%P"

if not defined LANDROP_DEV_PORT (
  echo Could not find an available dev port from 1420 to 1499.
  pause
  exit /b 1
)

set "LANDROP_DEV_CONFIG=%TEMP%\landrop-tauri-dev-%LANDROP_DEV_PORT%.json"
> "%LANDROP_DEV_CONFIG%" (
  echo {
  echo   "build": {
  echo     "devUrl": "http://localhost:%LANDROP_DEV_PORT%",
  echo     "beforeDevCommand": "npm run dev -- --port %LANDROP_DEV_PORT% --strictPort"
  echo   }
  echo }
)

echo Starting LanDrop dev server on http://localhost:%LANDROP_DEV_PORT%
call npx tauri dev --config "%LANDROP_DEV_CONFIG%"
set "LANDROP_EXIT_CODE=%ERRORLEVEL%"

del "%LANDROP_DEV_CONFIG%" >nul 2>nul
if not "%LANDROP_EXIT_CODE%"=="0" (
  echo.
  echo LanDrop dev exited with code %LANDROP_EXIT_CODE%.
)
pause
exit /b %LANDROP_EXIT_CODE%
