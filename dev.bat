@echo off
setlocal EnableExtensions EnableDelayedExpansion

rem This script intentionally uses no goto/call labels, so it keeps working
rem even if the file gets saved with Unix line endings by an editor or tool.

set "INSTALL_MODE=auto"
set "SHOW_HELP=0"
set "FRESH_MODE=1"
set "NO_PAUSE=0"
set "TAURI_ARGS="
set "ERR="

if not "%~1"=="" for %%A in (%*) do (
  if /I "%%~A"=="--help" (
    set "SHOW_HELP=1"
  ) else if /I "%%~A"=="-h" (
    set "SHOW_HELP=1"
  ) else if /I "%%~A"=="--skip-install" (
    set "INSTALL_MODE=skip"
  ) else if /I "%%~A"=="--install" (
    set "INSTALL_MODE=force"
  ) else if /I "%%~A"=="--fast" (
    set "FRESH_MODE=0"
  ) else if /I "%%~A"=="--no-pause" (
    set "NO_PAUSE=1"
  ) else (
    set "TAURI_ARGS=!TAURI_ARGS! %%~A"
  )
)

if "%SHOW_HELP%"=="1" (
  echo Usage:
  echo   dev.bat [--skip-install^|--install] [--fast] [--no-pause] [tauri-dev-args]
  echo.
  echo Examples:
  echo   dev.bat
  echo   dev.bat --skip-install
  echo   dev.bat --fast
  echo   dev.bat --port 1430
  echo.
  echo Options:
  echo   --skip-install    Never run npm ci / npm install.
  echo   --install         Always run npm ci / npm install first.
  echo   --fast            Skip the forced fresh rebuild of the app crate
  echo                     that runs by default before tauri dev.
  echo   --no-pause        Do not pause when an error occurs.
  echo   -h, --help        Show this help.
  exit /b 0
)

pushd "%~dp0" >nul

rem The user-level TEMP/TMP on this machine can be a broken literal
rem %%USERPROFILE%% short-name path. The MSVC linker then writes lnk*.tmp
rem files under src-tauri and the tauri dev watcher rebuilds in an endless
rem loop. Force a sane per-project temp dir inside target/, which both the
rem watcher and git ignore.
set "DEV_TEMP=%CD%\src-tauri\target\dev-temp"
if not exist "%DEV_TEMP%" mkdir "%DEV_TEMP%" >nul 2>&1
if exist "%DEV_TEMP%" (
  set "TEMP=%DEV_TEMP%"
  set "TMP=%DEV_TEMP%"
) else (
  echo [WARN] Could not create "%DEV_TEMP%", keeping current TEMP.
)

if not exist "package.json" (
  echo [ERROR] package.json not found in project root.
  set "ERR=1"
)
if not defined ERR (
  if not exist "src-tauri\Cargo.toml" (
    echo [ERROR] src-tauri\Cargo.toml not found.
    set "ERR=1"
  )
)
if not defined ERR (
  where npm >nul 2>&1
  if errorlevel 1 (
    echo [ERROR] npm is not available in PATH.
    set "ERR=1"
  )
)

set "DO_INSTALL=0"
set "HAS_TAURI_BIN=0"
if exist "node_modules\.bin\tauri.cmd" set "HAS_TAURI_BIN=1"
if /I "%INSTALL_MODE%"=="force" set "DO_INSTALL=1"
if /I "%INSTALL_MODE%"=="auto" (
  if not exist "node_modules" set "DO_INSTALL=1"
  if "%HAS_TAURI_BIN%"=="0" set "DO_INSTALL=1"
)

if not defined ERR if "%DO_INSTALL%"=="1" (
  set "NPM_EXIT=0"
  if exist "package-lock.json" (
    echo [1/3] Installing dependencies via npm ci...
    call npm ci
    set "NPM_EXIT=!ERRORLEVEL!"
  ) else (
    echo [1/3] Installing dependencies via npm install...
    call npm install
    set "NPM_EXIT=!ERRORLEVEL!"
  )
  if not "!NPM_EXIT!"=="0" (
    echo [ERROR] Dependency installation failed.
    set "ERR=1"
  ) else (
    if not exist "node_modules\.bin\tauri.cmd" (
      echo [ERROR] Tauri CLI is still missing after dependency installation.
      echo         Please rerun after closing node/Tauri processes.
      set "ERR=1"
    )
  )
)
if not defined ERR if not "%DO_INSTALL%"=="1" (
  if /I "%INSTALL_MODE%"=="skip" (
    echo [1/3] Skipping dependency installation.
    if "%HAS_TAURI_BIN%"=="0" (
      echo [ERROR] Tauri CLI is not available in node_modules.
      echo         Run dev.bat without --skip-install once.
      set "ERR=1"
    )
  ) else (
    echo [1/3] node_modules and Tauri CLI exist, skipping dependency installation.
  )
)

rem Fresh rebuild: kill leftover VarSwitch processes first, because the
rem single-instance plugin would otherwise just focus the old window that is
rem still running old code, and a live exe also blocks the linker. Then clean
rem only this crate's build artifacts so app code is always recompiled while
rem dependency caches stay intact.
if not defined ERR (
  if "%FRESH_MODE%"=="1" (
    echo [2/3] Forcing fresh rebuild of app code...
    taskkill /f /im varswitch.exe >nul 2>&1
    where cargo >nul 2>&1
    if errorlevel 1 (
      echo         cargo not found in PATH, skipping cargo clean.
    ) else (
      cargo clean -p varswitch --manifest-path "src-tauri\Cargo.toml" >nul 2>&1
    )
  ) else (
    echo [2/3] Fast mode: skipping forced rebuild.
  )
)

if not defined ERR (
  echo [3/3] Starting Tauri dev mode...
  call npm run tauri -- dev!TAURI_ARGS!
  if errorlevel 1 (
    echo [ERROR] Tauri dev failed.
    set "ERR=1"
  )
)

popd
if defined ERR (
  if not "%NO_PAUSE%"=="1" pause
  exit /b 1
)
exit /b 0
