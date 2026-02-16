const { createServer } = require("http");
const { readFile } = require("fs");
const { join, extname } = require("path");

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
  const url = req.url.split("?")[0];
  const file = join(ROOT, url === "/" ? "index.html" : url);
  readFile(file, (err, data) => {
    if (err) {
      res.writeHead(404);
      res.end();
      return;
    }
    res.writeHead(200, {
      "Content-Type": MIME[extname(file)] || "application/octet-stream",
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
