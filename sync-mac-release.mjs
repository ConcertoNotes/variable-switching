#!/usr/bin/env node
// 把 GitHub Actions 构建的 macOS 产物并入本地下载站。
//
// macOS 安装包无法在 Windows 上交叉编译（需要 macOS SDK、hdiutil 与 Apple 工具链），
// 因此由 .github/workflows/build.yml 在 macos runner 上构建并发布到 GitHub Release。
// 本脚本负责把这些产物取回下载站，让 latest.json 同时覆盖 Windows 与 macOS 两个平台。
//
// 用法：
//   node sync-mac-release.mjs                      同步与当前版本号匹配的 Release
//   node sync-mac-release.mjs --tag v3.1.2         指定 Release 标签
//   node sync-mac-release.mjs --host github        不下载 dmg，直接链接 GitHub Release
//   node sync-mac-release.mjs --token <PAT>        私有仓库或规避 API 限流时使用

import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.dirname(fileURLToPath(import.meta.url));
const SITE_DIR = path.join(ROOT, 'varswitch-download-site');
const RELEASES_DIR = path.join(SITE_DIR, 'releases');
const INDEX_FILE = path.join(SITE_DIR, 'index.html');
const LATEST_FILE = path.join(SITE_DIR, 'latest.json');
const TAURI_CONF = path.join(ROOT, 'src-tauri', 'tauri.conf.json');

const DOWNLOAD_DOMAIN = 'https://download.varswitch.strova.top';
const DEFAULT_REPO = 'ConcertoNotes/varswitch';

// arch 取值与 .github/workflows/build.yml 的 releaseAssetNamePattern 保持一致。
const MAC_TARGETS = [
  { platform: 'darwin-aarch64', arch: 'aarch64' },
  { platform: 'darwin-x86_64', arch: 'x64' },
];

function parseArgs(argv) {
  const options = { tag: '', repo: DEFAULT_REPO, host: 'local', token: process.env.GITHUB_TOKEN || '' };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    const next = () => {
      const value = argv[i + 1];
      if (!value) throw new Error(`${arg} 需要一个参数值`);
      i += 1;
      return value;
    };
    if (arg === '--tag') options.tag = next();
    else if (arg === '--repo') options.repo = next();
    else if (arg === '--host') options.host = next();
    else if (arg === '--token') options.token = next();
    else if (arg === '--help' || arg === '-h') options.help = true;
    else throw new Error(`未知参数: ${arg}`);
  }
  if (!['local', 'github'].includes(options.host)) {
    throw new Error(`--host 只能是 local 或 github，收到: ${options.host}`);
  }
  return options;
}

function printHelp() {
  console.log(`用法: node sync-mac-release.mjs [选项]

选项:
  --tag <v3.1.2>     要同步的 GitHub Release 标签，默认取 tauri.conf.json 的版本号
  --repo <owner/name> 仓库，默认 ${DEFAULT_REPO}
  --host <local|github>
                     local  下载 dmg 与更新包到下载站，由自有域名分发（默认）
                     github 不下载安装包，latest.json 与下载页直接指向 GitHub Release
  --token <PAT>      GitHub Token，私有仓库或触发 API 限流时需要
  -h, --help         显示帮助

前置条件:
  已推送 tag 并且 GitHub Actions 的三个构建作业全部成功。`);
}

async function readJson(file) {
  return JSON.parse(await fs.readFile(file, 'utf8'));
}

async function fetchRelease({ repo, tag, token }) {
  const url = `https://api.github.com/repos/${repo}/releases/tags/${tag}`;
  const headers = { Accept: 'application/vnd.github+json', 'User-Agent': 'varswitch-sync' };
  if (token) headers.Authorization = `Bearer ${token}`;

  const response = await fetch(url, { headers });
  if (response.status === 404) {
    // 私有仓库对未认证请求同样返回 404，因此这里无法区分「不存在」和「没权限」。
    const hint = token
      ? '请确认 tag 已推送、Actions 构建已完成，且 token 对该仓库有读权限。'
      : `${repo} 是私有仓库时匿名访问也会返回 404，请用 --token 传入有 repo 读权限的 Token 后重试。`;
    throw new Error(`找不到 Release ${tag}（${repo}）。${hint}`);
  }
  if (response.status === 403 && !token) {
    throw new Error('GitHub API 限流。请用 --token 传入一个 Personal Access Token 后重试。');
  }
  if (!response.ok) {
    throw new Error(`读取 Release 失败: HTTP ${response.status} ${response.statusText}`);
  }
  return response.json();
}

async function downloadAsset(asset, token) {
  const headers = { Accept: 'application/octet-stream', 'User-Agent': 'varswitch-sync' };
  if (token) headers.Authorization = `Bearer ${token}`;
  const response = await fetch(asset.url, { headers, redirect: 'follow' });
  if (!response.ok) {
    throw new Error(`下载 ${asset.name} 失败: HTTP ${response.status}`);
  }
  return Buffer.from(await response.arrayBuffer());
}

function findAsset(assets, name) {
  // tauri-action 上传时会把 label 设为原始文件名，GitHub 可能改写 name 字段。
  return assets.find((asset) => asset.label === name || asset.name === name);
}

function formatSize(bytes) {
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

// 只匹配 macOS 产物，Windows 的 VarSwitch_x.y.z_x64-setup.exe 由 deploy-download-site.bat 负责。
const MAC_ARTIFACT = /^VarSwitch_.+_(aarch64|x64)\.(dmg|app\.tar\.gz)(\.sig)?$/;

async function pruneOldMacArtifacts(version) {
  const keep = new Set();
  for (const { arch } of MAC_TARGETS) {
    keep.add(`VarSwitch_${version}_${arch}.dmg`);
    keep.add(`VarSwitch_${version}_${arch}.app.tar.gz`);
    keep.add(`VarSwitch_${version}_${arch}.app.tar.gz.sig`);
  }

  let removed = 0;
  for (const name of await fs.readdir(RELEASES_DIR)) {
    if (MAC_ARTIFACT.test(name) && !keep.has(name)) {
      await fs.unlink(path.join(RELEASES_DIR, name));
      removed += 1;
    }
  }
  if (removed > 0) console.log(`已清理 ${removed} 个旧版本 macOS 产物`);
}

async function updateIndexHtml(entries) {
  let html = await fs.readFile(INDEX_FILE, 'utf8');
  let changed = 0;

  for (const entry of entries) {
    const hrefPattern = new RegExp(
      `(data-mac-download="${entry.arch}"[^>]*?href=")[^"]*(")`,
      'i',
    );
    const filePattern = new RegExp(
      `(data-mac-file="${entry.arch}"[^>]*>)[^<]*(</)`,
      'i',
    );

    if (hrefPattern.test(html)) {
      html = html.replace(hrefPattern, `$1${entry.pageUrl}$2`);
      changed += 1;
    }
    if (filePattern.test(html)) {
      html = html.replace(filePattern, `$1${entry.dmgName}$2`);
    }
  }

  if (changed === 0) {
    console.warn('[warn] index.html 中没有找到 data-mac-download 锚点，跳过下载页更新。');
    return;
  }

  await fs.writeFile(INDEX_FILE, html, 'utf8');
  console.log(`已更新 index.html 的 ${changed} 个 macOS 下载链接`);
}

async function updateLatestJson(version, entries) {
  let manifest;
  try {
    manifest = await readJson(LATEST_FILE);
  } catch {
    manifest = { version, notes: `VarSwitch update ${version}`, platforms: {} };
  }

  if (manifest.version !== version) {
    console.warn(
      `[warn] latest.json 当前版本是 ${manifest.version}，与本次同步的 ${version} 不一致。\n` +
        '       Windows 与 macOS 必须发布同一版本，否则另一端用户会被反复提示更新。\n' +
        '       请先用 build.bat + deploy-download-site.bat 发布同版本的 Windows 包。',
    );
  }

  manifest.platforms = manifest.platforms || {};
  for (const entry of entries) {
    manifest.platforms[entry.platform] = { signature: entry.signature, url: entry.updaterUrl };
  }
  manifest.pub_date = new Date().toISOString().replace(/\.\d+Z$/, 'Z');

  await fs.writeFile(LATEST_FILE, `${JSON.stringify(manifest, null, 2)}\n`, 'utf8');
  console.log(`已更新 latest.json，覆盖平台: ${Object.keys(manifest.platforms).join(', ')}`);
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.help) {
    printHelp();
    return;
  }

  const conf = await readJson(TAURI_CONF);
  const version = conf.version;
  const tag = options.tag || `v${version}`;

  console.log(`仓库:   ${options.repo}`);
  console.log(`标签:   ${tag}`);
  console.log(`版本:   ${version}`);
  console.log(`托管:   ${options.host === 'local' ? '下载站自有域名' : 'GitHub Release'}`);
  console.log('');

  const release = await fetchRelease({ repo: options.repo, tag, token: options.token });
  const assets = release.assets || [];
  if (assets.length === 0) {
    throw new Error(`Release ${tag} 没有任何资源，构建可能失败了。`);
  }

  await fs.mkdir(RELEASES_DIR, { recursive: true });
  await pruneOldMacArtifacts(version);
  const entries = [];

  for (const target of MAC_TARGETS) {
    const dmgName = `VarSwitch_${version}_${target.arch}.dmg`;
    const updaterName = `VarSwitch_${version}_${target.arch}.app.tar.gz`;
    const sigName = `${updaterName}.sig`;

    const dmgAsset = findAsset(assets, dmgName);
    const updaterAsset = findAsset(assets, updaterName);
    const sigAsset = findAsset(assets, sigName);

    const missing = [
      !dmgAsset && dmgName,
      !updaterAsset && updaterName,
      !sigAsset && sigName,
    ].filter(Boolean);

    if (missing.length > 0) {
      console.error(`\n[error] Release ${tag} 缺少 ${target.platform} 的产物:`);
      missing.forEach((name) => console.error(`        - ${name}`));
      console.error('\n实际存在的资源:');
      assets.forEach((asset) => console.error(`        - ${asset.label || asset.name}`));
      throw new Error(`${target.platform} 产物不完整，无法同步。`);
    }

    // 签名必须内联进 latest.json，Tauri 更新器不接受 URL 形式的 signature。
    const signature = (await downloadAsset(sigAsset, options.token)).toString('utf8').trim();

    let pageUrl;
    let updaterUrl;

    if (options.host === 'local') {
      // 落盘一律使用规范名，GitHub 有时会改写资源的 name 字段，
      // 若照抄回来会和页面链接、latest.json 里的 URL 对不上。
      for (const [asset, fileName] of [[dmgAsset, dmgName], [updaterAsset, updaterName]]) {
        const buffer = await downloadAsset(asset, options.token);
        await fs.writeFile(path.join(RELEASES_DIR, fileName), buffer);
        console.log(`已下载 ${fileName} (${formatSize(buffer.length)})`);
      }
      await fs.writeFile(path.join(RELEASES_DIR, sigName), signature, 'utf8');
      pageUrl = `./releases/${dmgName}`;
      updaterUrl = `${DOWNLOAD_DOMAIN}/releases/${updaterName}`;
    } else {
      pageUrl = dmgAsset.browser_download_url;
      updaterUrl = updaterAsset.browser_download_url;
      console.log(`已解析 ${dmgName} 的 GitHub 下载地址`);
    }

    entries.push({ ...target, dmgName, pageUrl, updaterUrl, signature });
  }

  console.log('');
  await updateLatestJson(version, entries);
  await updateIndexHtml(entries);

  console.log('\n完成。接下来部署下载站：');
  console.log('  cd varswitch-download-site && npx vercel --prod --yes');
}

main().catch((error) => {
  console.error(`\n[error] ${error.message}`);
  process.exitCode = 1;
});
