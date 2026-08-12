#!/usr/bin/env bash
set -euo pipefail

script="$(cat build.bat)"

grep -Fq 'set "HAS_BUNDLE_ARG=0"' <<<"$script" || {
  echo "missing: tracks whether --bundles was supplied" >&2
  exit 1
}

grep -Fq -- '--bundles nsis' <<<"$script" || {
  echo "missing: defaults Windows packaging to NSIS" >&2
  exit 1
}

grep -Fq 'Recommended installer:' <<<"$script" || {
  echo "missing: prints the recommended installer path" >&2
  exit 1
}

grep -Fq 'set "TEMP=%CD%\src-tauri\target\build-temp"' <<<"$script" || {
  echo "missing: uses a project-local build temporary directory" >&2
  exit 1
}

grep -Fq 'set "TMP=%TEMP%"' <<<"$script" || {
  echo "missing: keeps TEMP and TMP on the same build drive" >&2
  exit 1
}

node -e 'const c=require("./src-tauri/tauri.conf.json"); if(c.bundle?.windows?.nsis?.compression!=="zlib") process.exit(1)' || {
  echo "missing: configures low-memory NSIS zlib compression" >&2
  exit 1
}
