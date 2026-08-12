const { createServer } = require("http");
const { readFile } = require("fs");
const { join, extname, normalize, sep } = require("path");

const PORT = 1430;
const ROOT = join(__dirname, "public");

const MIME = {
  ".html": "text/html; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".js": "application/javascript; charset=utf-8",
  ".png": "image/png",
  ".ico": "image/x-icon",
  ".svg": "image/svg+xml",
  ".json": "application/json",
};

const server = createServer((req, res) => {
  let pathname;
  try {
    pathname = decodeURIComponent(req.url.split("?")[0]);
  } catch {
    res.writeHead(400);
    res.end();
    return;
  }
  const relative = normalize(pathname === "/" ? "index.html" : pathname).replace(/^([/\\])+/, "");
  const file = join(ROOT, relative);
  // 防止路径穿越：解析后的绝对路径必须仍位于 public 目录内
  if (file !== ROOT && !file.startsWith(ROOT + sep)) {
    res.writeHead(403);
    res.end();
    return;
  }
  readFile(file, (err, data) => {
    if (err) {
      res.writeHead(404);
      res.end();
      return;
    }
    res.writeHead(200, {
      "Content-Type": MIME[extname(file)] || "application/octet-stream",
      // dev 模式禁止缓存，避免 WebView2 加载旧的 app.js / index.html
      "Cache-Control": "no-store",
    });
    res.end(data);
  });
});

server.on("error", (err) => {
  if (err.code === "EADDRINUSE") {
    console.log(`[dev-server] Port ${PORT} already in use, assuming previous server is running.`);
    process.exit(0);
  }
  throw err;
});

server.listen(PORT, () => {
  console.log(`[dev-server] http://localhost:${PORT}`);
});
