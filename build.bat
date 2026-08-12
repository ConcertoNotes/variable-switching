@echo off
setlocal EnableExtensions EnableDelayedExpansion

set "INSTALL_MODE=auto"
set "SHOW_HELP=0"
set "VERSION_MODE=prompt"
set "BUILD_VERSION="
set "NO_PAUSE=0"
set "VERSION_STATE_FILE=.build-version"
set "TAURI_ARGS="
set "HAS_BUNDLE_ARG=0"
set "SKIP_TAG=0"
set "TAG_NAME="
set "TAG_PUSHED=0"
set "TAG_PUSH_FAILED=0"
set "GITHUB_REPO=ConcertoNotes/varswitch"

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
if /I "%~1"=="--no-pause" (
  set "NO_PAUSE=1"
  shift
  goto :parse_args
)
if /I "%~1"=="--no-version-prompt" (
  set "VERSION_MODE=keep"
  shift
  goto :parse_args
)
if /I "%~1"=="--patch" (
  set "VERSION_MODE=patch"
  shift
  goto :parse_args
)
if /I "%~1"=="--version" (
  if "%~2"=="" (
    echo [ERROR] --version requires a value, e.g. --version 1.2.3
    goto :fail_no_popd
  )
  set "VERSION_MODE=set"
  set "BUILD_VERSION=%~2"
  shift
  shift
  goto :parse_args
)
if /I "%~1"=="--no-tag" (
  set "SKIP_TAG=1"
  shift
  goto :parse_args
)
if /I "%~1"=="--bundles" set "HAS_BUNDLE_ARG=1"
if /I "%~1"=="-b" set "HAS_BUNDLE_ARG=1"
set "TAURI_ARGS=!TAURI_ARGS! %~1"
shift
goto :parse_args

:after_parse
if "%SHOW_HELP%"=="1" goto :help
if "%HAS_BUNDLE_ARG%"=="0" set "TAURI_ARGS=!TAURI_ARGS! --bundles nsis"

pushd "%~dp0" >nul

if not exist "package.json" (
  echo [ERROR] package.json not found in project root.
  goto :fail
)

if not exist "src-tauri\Cargo.toml" (
  echo [ERROR] src-tauri\Cargo.toml not found.
  goto :fail
)

if not exist "src-tauri\tauri.conf.json" (
  echo [ERROR] src-tauri\tauri.conf.json not found.
  goto :fail
)

where npm >nul 2>&1
if errorlevel 1 (
  echo [ERROR] npm is not available in PATH.
  goto :fail
)

where node >nul 2>&1
if errorlevel 1 (
  echo [ERROR] node is not available in PATH.
  goto :fail
)

for /f "usebackq delims=" %%V in (`node -e "const fs=require('fs'); const p=JSON.parse(fs.readFileSync('src-tauri/tauri.conf.json','utf8')); console.log(p.version || '0.0.0')"`) do set "CURRENT_VERSION=%%V"
set "LAST_BUILD_VERSION="
if exist "%VERSION_STATE_FILE%" (
  for /f "usebackq delims=" %%V in ("%VERSION_STATE_FILE%") do (
    if not defined LAST_BUILD_VERSION set "LAST_BUILD_VERSION=%%V"
  )
)
if "%LAST_BUILD_VERSION%"=="" set "LAST_BUILD_VERSION=%CURRENT_VERSION%"
for /f "usebackq delims=" %%V in (`node -e "const v=(process.argv[1]||'0.0.0').trim().replace(/^v/i,''); const p=v.split('.').map(n=>parseInt(n,10)||0); while(p.length<3)p.push(0); p[2]+=1; console.log(p.slice(0,3).join('.'))" "%LAST_BUILD_VERSION%"`) do set "DEFAULT_VERSION=%%V"

if /I "%VERSION_MODE%"=="keep" (
  set "FINAL_VERSION=%CURRENT_VERSION%"
) else if /I "%VERSION_MODE%"=="patch" (
  set "FINAL_VERSION=%DEFAULT_VERSION%"
) else if /I "%VERSION_MODE%"=="set" (
  set "FINAL_VERSION=%BUILD_VERSION%"
) else (
  echo Current version: %CURRENT_VERSION%
  echo Last build version: %LAST_BUILD_VERSION%
  echo Default next build version: %DEFAULT_VERSION%
  set /p "FINAL_VERSION=Build version [Enter=%DEFAULT_VERSION%]: "
  if "!FINAL_VERSION!"=="" set "FINAL_VERSION=%DEFAULT_VERSION%"
)

set "VERSION_TMP=%TEMP%\varswitch-build-version.txt"
node -e "const raw=(process.argv[1]||'').trim(); const v=raw.replace(/^v/i,''); if(/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(v) === false){ console.error('[ERROR] Invalid version: '+raw); process.exit(1); } process.stdout.write(v);" "%FINAL_VERSION%" > "%VERSION_TMP%"
if errorlevel 1 (
  if exist "%VERSION_TMP%" del "%VERSION_TMP%" >nul 2>&1
  goto :fail
)
set /p "FINAL_VERSION="<"%VERSION_TMP%"
del "%VERSION_TMP%" >nul 2>&1

echo [1/5] Syncing project version to %FINAL_VERSION%...
node -e "const fs=require('fs'); const version=process.argv[1]; function writeJson(file, update){ const data=JSON.parse(fs.readFileSync(file,'utf8')); update(data); fs.writeFileSync(file, JSON.stringify(data,null,2)+'\n'); } writeJson('package.json', d=>{d.version=version}); if(fs.existsSync('package-lock.json')) writeJson('package-lock.json', d=>{d.version=version; if(d.packages&&d.packages['']) d.packages[''].version=version;}); writeJson('src-tauri/tauri.conf.json', d=>{d.version=version}); const cargo='src-tauri/Cargo.toml'; let inPackage=false; const text=fs.readFileSync(cargo,'utf8').split(/\r?\n/).map(line=>{ if(/^\[package\]\s*$/.test(line)){ inPackage=true; return line; } if(/^\[/.test(line)) inPackage=false; if(inPackage && /^version\s*=/.test(line)) return 'version = \"'+version+'\"'; return line; }).join('\n'); fs.writeFileSync(cargo, text.endsWith('\n') ? text : text+'\n'); const html='public/index.html'; if(fs.existsSync(html)){ fs.writeFileSync(html, fs.readFileSync(html,'utf8').replace(/VarSwitch <span>v\d+\.\d+\.\d+<\/span>/, 'VarSwitch <span>v'+version+'</span>')); } const readme='README.md'; if(fs.existsSync(readme)){ fs.writeFileSync(readme, fs.readFileSync(readme,'utf8').replace(/(\u5f53\u524d\u5e94\u7528\u7248\u672c\uff1a)`\d+\.\d+\.\d+`/, '$1`'+version+'`')); }" "%FINAL_VERSION%"
if errorlevel 1 (
  echo [ERROR] Failed to sync version files.
  goto :fail
)
> "%VERSION_STATE_FILE%" echo %FINAL_VERSION%

echo [2/5] Pushing release tag to trigger the GitHub macOS build...
if "%SKIP_TAG%"=="1" (
  echo       --no-tag set, skipping tag push.
  goto :after_tag_push
)
where git >nul 2>&1
if errorlevel 1 (
  echo       git is not available in PATH, skipping tag push.
  goto :after_tag_push
)
git rev-parse --is-inside-work-tree >nul 2>&1
if errorlevel 1 (
  echo       Not a git working tree, skipping tag push.
  goto :after_tag_push
)
set "TAG_NAME=v%FINAL_VERSION%"
git ls-remote --exit-code --tags origin "refs/tags/%TAG_NAME%" >nul 2>&1
if not errorlevel 1 (
  echo       Tag %TAG_NAME% already exists on origin, macOS build was already triggered.
  goto :after_tag_push
)
rem The tag has to include the version bump commit, otherwise Actions would
rem build the previous version number into the macOS artifacts.
for %%P in ("package.json" "package-lock.json" "src-tauri\tauri.conf.json" "src-tauri\Cargo.toml" "src-tauri\Cargo.lock") do (
  if exist "%%~P" git add -- "%%~P" >nul 2>&1
)
set "NEED_COMMIT=0"
git diff --cached --quiet
if errorlevel 1 set "NEED_COMMIT=1"
if "%NEED_COMMIT%"=="1" (
  git commit -q -m "chore: release %TAG_NAME%"
  if errorlevel 1 (
    echo [WARN] Failed to commit the version bump, skipping tag push.
    set "TAG_PUSH_FAILED=1"
    goto :after_tag_push
  )
) else (
  echo       Version files unchanged, tagging current HEAD.
)
git rev-parse -q --verify "refs/tags/%TAG_NAME%" >nul 2>&1
if errorlevel 1 goto :create_tag
echo       Local tag %TAG_NAME% already exists, pushing it as is.
echo       If it points at an older commit, fix it with:
echo         git tag -f %TAG_NAME% ^&^& git push -f origin %TAG_NAME%
goto :push_tag
:create_tag
git tag "%TAG_NAME%"
if errorlevel 1 (
  echo [WARN] Failed to create tag %TAG_NAME%, skipping push.
  set "TAG_PUSH_FAILED=1"
  goto :after_tag_push
)
:push_tag
git push origin HEAD
if errorlevel 1 (
  echo [WARN] Failed to push the current branch to origin, skipping tag push.
  set "TAG_PUSH_FAILED=1"
  goto :after_tag_push
)
git push origin "%TAG_NAME%"
if errorlevel 1 (
  echo [WARN] Failed to push tag %TAG_NAME%.
  set "TAG_PUSH_FAILED=1"
  goto :after_tag_push
)
set "TAG_PUSHED=1"
echo       Pushed %TAG_NAME%, GitHub Actions is now building the macOS packages.
:after_tag_push

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
    echo [3/5] Installing dependencies via npm ci...
    call npm ci
    set "NPM_EXIT=!ERRORLEVEL!"
  ) else (
    echo [3/5] Installing dependencies via npm install...
    call npm install
    set "NPM_EXIT=!ERRORLEVEL!"
  )
  if not "!NPM_EXIT!"=="0" (
    echo [ERROR] Dependency installation failed.
    echo         If this is EPERM on @tauri-apps/cli, close running node/Tauri processes and retry.
    echo         You can also retry with --skip-install when node_modules already exists.
    goto :fail
  )
  if not exist "node_modules\.bin\tauri.cmd" (
    echo [ERROR] Tauri CLI is still missing after dependency installation.
    echo         Please rerun after closing node/Tauri processes.
    goto :fail
  )
) else (
  if /I "%INSTALL_MODE%"=="skip" (
    echo [3/5] Skipping dependency installation.
    if "%HAS_TAURI_BIN%"=="0" (
      echo [ERROR] Tauri CLI is not available in node_modules.
      echo         Run build.bat without --skip-install once.
      goto :fail
    )
  ) else (
    echo [3/5] node_modules and Tauri CLI exist, skipping dependency installation.
  )
)

if exist "app-icon.png" (
  echo [4/5] Syncing Tauri icons from app-icon.png...
  powershell -NoProfile -ExecutionPolicy Bypass -Command "Add-Type -AssemblyName System.Drawing; $src='app-icon.png'; $dst='src-tauri/icons/source-square.png'; $img=[System.Drawing.Image]::FromFile($src); $size=[Math]::Max($img.Width,$img.Height); $bmp=New-Object System.Drawing.Bitmap($size,$size); $g=[System.Drawing.Graphics]::FromImage($bmp); $g.Clear([System.Drawing.Color]::Transparent); $x=[int](($size-$img.Width)/2); $y=[int](($size-$img.Height)/2); $g.DrawImage($img,$x,$y,$img.Width,$img.Height); $bmp.Save($dst,[System.Drawing.Imaging.ImageFormat]::Png); $g.Dispose(); $bmp.Dispose(); $img.Dispose()"
  if errorlevel 1 (
    echo [ERROR] Failed to generate square icon source.
    goto :fail
  )

  call npm run tauri -- icon src-tauri/icons/source-square.png
  if errorlevel 1 (
    echo [ERROR] Failed to generate Tauri icons.
    goto :fail
  )
) else (
  echo [4/5] app-icon.png not found, skipping icon sync.
)

echo [5/5] Building Tauri bundle...
set "TEMP=%CD%\src-tauri\target\build-temp"
set "TMP=%TEMP%"
if exist "%TEMP%" rmdir /s /q "%TEMP%" >nul 2>&1
mkdir "%TEMP%" >nul 2>&1
if errorlevel 1 (
  echo [ERROR] Failed to prepare build temporary directory:
  echo         %TEMP%
  goto :fail
)
echo       Build temporary directory: %TEMP%
if exist "src-tauri\updater.key" (
  for /f "usebackq delims=" %%K in ("src-tauri\updater.key") do set "TAURI_SIGNING_PRIVATE_KEY=%%K"
  set "TAURI_SIGNING_PRIVATE_KEY_PASSWORD=varswitch-updater"
)
call npm run tauri -- build!TAURI_ARGS!
set "TAURI_EXIT=!ERRORLEVEL!"
if exist "%TEMP%" rmdir /s /q "%TEMP%" >nul 2>&1
if not "!TAURI_EXIT!"=="0" (
  echo [ERROR] Tauri build failed.
  goto :fail
)

echo.
echo Build completed. Artifacts are under:
echo   src-tauri\target\release\bundle

set "RECOMMENDED_INSTALLER="
if exist "src-tauri\target\release\bundle\nsis" (
  for /f "delims=" %%F in ('dir /b /s /o-d "src-tauri\target\release\bundle\nsis\VarSwitch_*_x64-setup.exe" 2^>nul') do (
    if not defined RECOMMENDED_INSTALLER set "RECOMMENDED_INSTALLER=%%F"
  )
)
if defined RECOMMENDED_INSTALLER (
  echo.
  echo Recommended installer:
  echo   !RECOMMENDED_INSTALLER!
)

echo.
echo All bundle files currently present ^(may include earlier builds^):
for %%D in (appimage deb dmg msi nsis rpm app) do (
  if exist "src-tauri\target\release\bundle\%%D" (
    for /f "delims=" %%F in ('dir /b /s "src-tauri\target\release\bundle\%%D\*" 2^>nul') do echo   %%F
  )
)

if "%TAG_PUSHED%"=="1" (
  echo.
  echo GitHub Actions is building the macOS packages for v%FINAL_VERSION%:
  echo   https://github.com/%GITHUB_REPO%/actions
  echo Once the run is green, publish both platforms with:
  echo   deploy-download-site.bat
)
if "%TAG_PUSH_FAILED%"=="1" (
  echo.
  echo [WARN] Release tag was not pushed, so the macOS build has not started.
  echo        Push it manually, then run deploy-download-site.bat:
  echo          git push origin v%FINAL_VERSION%
)

popd
exit /b 0

:help
echo Usage:
echo   build.bat [--skip-install^|--install] [--version x.y.z^|--patch^|--no-version-prompt] [--no-tag] [--no-pause] [tauri-build-args]
echo.
echo Examples:
echo   build.bat
echo   build.bat --skip-install
echo   build.bat --install
echo   build.bat --patch
echo   build.bat --version 1.2.3
echo   build.bat --no-version-prompt
echo   build.bat --no-tag
echo   build.bat --bundles msi
echo   build.bat --target x86_64-pc-windows-msvc
echo.
echo Options:
echo   --skip-install    Never run npm ci / npm install.
echo   --install         Always run npm ci / npm install first.
echo   --version x.y.z   Set package/Tauri/Cargo version before building.
echo   --patch           Use current patch version + 1 without prompting.
echo   --no-version-prompt
echo                    Keep current version without prompting.
echo   --no-tag         Do not commit and push the vX.Y.Z release tag.
echo   --no-pause       Do not pause when an error occurs.
echo   --bundles type   Override the default Windows bundle type ^(nsis^).
echo   -h, --help        Show this help.
echo.
echo Release flow:
echo   The version bump is committed and pushed as tag vX.Y.Z right after the
echo   version is chosen, so GitHub Actions builds the macOS packages while the
echo   Windows bundle compiles locally. Use --no-tag for local test builds.
exit /b 0

:fail
popd
:fail_no_popd
if not "%NO_PAUSE%"=="1" pause
exit /b 1
