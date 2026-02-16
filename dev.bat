@echo off
setlocal EnableExtensions EnableDelayedExpansion

set "INSTALL_MODE=auto"
set "SHOW_HELP=0"
set "TAURI_ARGS="

:parse_args
if "%~1"=="" goto :after_parse
if /I "%~1"=="--help" (
  set "SHOW_HELP=1"
  shift
  goto :parse_args
)
if /I "%~1"=="-h" (
  set "SHOW_HELP=1"
  shift
  goto :parse_args
)
if /I "%~1"=="--skip-install" (
  set "INSTALL_MODE=skip"
  shift
  goto :parse_args
)
if /I "%~1"=="--install" (
  set "INSTALL_MODE=force"
  shift
  goto :parse_args
)
set "TAURI_ARGS=!TAURI_ARGS! %~1"
shift
goto :parse_args

:after_parse
if "%SHOW_HELP%"=="1" goto :help

pushd "%~dp0" >nul

if not exist "package.json" (
  echo [ERROR] package.json not found in project root.
  popd
  exit /b 1
)

if not exist "src-tauri\Cargo.toml" (
  echo [ERROR] src-tauri\Cargo.toml not found.
  popd
  exit /b 1
)

where npm >nul 2>&1
if errorlevel 1 (
  echo [ERROR] npm is not available in PATH.
  popd
  exit /b 1
)

set "DO_INSTALL=0"
set "HAS_TAURI_BIN=0"
if exist "node_modules\.bin\tauri.cmd" set "HAS_TAURI_BIN=1"

if /I "%INSTALL_MODE%"=="force" set "DO_INSTALL=1"
if /I "%INSTALL_MODE%"=="auto" (
  if not exist "node_modules" set "DO_INSTALL=1"
  if "%HAS_TAURI_BIN%"=="0" set "DO_INSTALL=1"
)

if "%DO_INSTALL%"=="1" (
  set "NPM_EXIT=0"
  if exist "package-lock.json" (
    echo [1/2] Installing dependencies via npm ci...
    call npm ci
    set "NPM_EXIT=!ERRORLEVEL!"
  ) else (
    echo [1/2] Installing dependencies via npm install...
    call npm install
    set "NPM_EXIT=!ERRORLEVEL!"
  )
  if not "!NPM_EXIT!"=="0" (
    echo [ERROR] Dependency installation failed.
    popd
    exit /b 1
  )
  if not exist "node_modules\.bin\tauri.cmd" (
    echo [ERROR] Tauri CLI is still missing after dependency installation.
    echo         Please rerun after closing node/Tauri processes.
    popd
    exit /b 1
  )
) else (
  if /I "%INSTALL_MODE%"=="skip" (
    echo [1/2] Skipping dependency installation.
    if "%HAS_TAURI_BIN%"=="0" (
      echo [ERROR] Tauri CLI is not available in node_modules.
      echo         Run dev.bat without --skip-install once.
      popd
      exit /b 1
    )
  ) else (
    echo [1/2] node_modules and Tauri CLI exist, skipping dependency installation.
  )
)

echo [2/2] Starting Tauri dev mode...
call npm run tauri -- dev!TAURI_ARGS!
if errorlevel 1 (
  echo [ERROR] Tauri dev failed.
  popd
  exit /b 1
)

popd
exit /b 0

:help
echo Usage:
echo   dev.bat [--skip-install^|--install] [tauri-dev-args]
echo.
echo Examples:
echo   dev.bat
echo   dev.bat --skip-install
echo   dev.bat --port 1430
echo   dev.bat -- -- --debug
echo.
echo Options:
echo   --skip-install    Never run npm ci / npm install.
echo   --install         Always run npm ci / npm install first.
echo   -h, --help        Show this help.
exit /b 0
