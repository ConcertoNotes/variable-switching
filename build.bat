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
    echo         If this is EPERM on @tauri-apps/cli, close running node/Tauri processes and retry.
    echo         You can also retry with --skip-install when node_modules already exists.
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
    echo [1/3] Skipping dependency installation.
    if "%HAS_TAURI_BIN%"=="0" (
      echo [ERROR] Tauri CLI is not available in node_modules.
      echo         Run build.bat without --skip-install once.
      popd
      exit /b 1
    )
  ) else (
    echo [1/3] node_modules and Tauri CLI exist, skipping dependency installation.
  )
)

if exist "app-icon.png" (
  echo [2/3] Syncing Tauri icons from app-icon.png...
  powershell -NoProfile -ExecutionPolicy Bypass -Command "Add-Type -AssemblyName System.Drawing; $src='app-icon.png'; $dst='src-tauri/icons/source-square.png'; $img=[System.Drawing.Image]::FromFile($src); $size=[Math]::Max($img.Width,$img.Height); $bmp=New-Object System.Drawing.Bitmap($size,$size); $g=[System.Drawing.Graphics]::FromImage($bmp); $g.Clear([System.Drawing.Color]::Transparent); $x=[int](($size-$img.Width)/2); $y=[int](($size-$img.Height)/2); $g.DrawImage($img,$x,$y,$img.Width,$img.Height); $bmp.Save($dst,[System.Drawing.Imaging.ImageFormat]::Png); $g.Dispose(); $bmp.Dispose(); $img.Dispose()"
  if errorlevel 1 (
    echo [ERROR] Failed to generate square icon source.
    popd
    exit /b 1
  )

  call npm run tauri -- icon src-tauri/icons/source-square.png
  if errorlevel 1 (
    echo [ERROR] Failed to generate Tauri icons.
    popd
    exit /b 1
  )
) else (
  echo [2/3] app-icon.png not found, skipping icon sync.
)

echo [3/3] Building Tauri bundle...
call npm run tauri -- build!TAURI_ARGS!
if errorlevel 1 (
  echo [ERROR] Tauri build failed.
  popd
  exit /b 1
)

echo.
echo Build completed. Artifacts are under:
echo   src-tauri\target\release\bundle

for %%D in (appimage deb dmg msi nsis rpm app) do (
  if exist "src-tauri\target\release\bundle\%%D" (
    for /f "delims=" %%F in ('dir /b /s "src-tauri\target\release\bundle\%%D\*" 2^>nul') do echo   %%F
  )
)

popd
exit /b 0

:help
echo Usage:
echo   build.bat [--skip-install^|--install] [tauri-build-args]
echo.
echo Examples:
echo   build.bat
echo   build.bat --skip-install
echo   build.bat --install
echo   build.bat --target x86_64-pc-windows-msvc
echo.
echo Options:
echo   --skip-install    Never run npm ci / npm install.
echo   --install         Always run npm ci / npm install first.
echo   -h, --help        Show this help.
exit /b 0
