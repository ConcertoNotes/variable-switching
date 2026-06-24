# VarSwitch 下载页

这是一个独立的静态下载页项目，用于部署到 Vercel 并绑定 Cloudflare 托管的域名。

## 文件结构

```text
varswitch-download-site/
├── index.html
├── styles.css
├── vercel.json
└── releases/
```

## 放置安装包

在 `releases/` 目录中放入安装包，并保持文件名与页面链接一致：

```text
releases/VarSwitch-windows-x64.msi
releases/VarSwitch-macos-universal.dmg
```

如果文件名不同，请同步修改 `index.html` 中两个下载链接的 `href`。

## Vercel 部署

1. 进入 Vercel 新建项目。
2. 导入或上传这个独立目录 `varswitch-download-site`。
3. Framework Preset 选择 `Other`。
4. Build Command 留空。
5. Output Directory 使用项目根目录。
6. 部署完成后，在项目 Settings -> Domains 添加你的域名。

## Cloudflare 绑定域名

在 Vercel 添加域名后，Vercel 会给出需要配置的 DNS 记录。到 Cloudflare DNS 中按提示添加：

- 根域名通常使用 `A` 记录指向 Vercel 提供的 IP。
- `www` 或其他子域名通常使用 `CNAME` 指向 Vercel 提供的目标。
- 如果 Cloudflare 代理导致验证异常，先将对应记录切换为 DNS only，等 Vercel 证书签发完成后再决定是否开启代理。

DNS 生效可能需要等待一段时间。
