const METRICS = {
  downloads: "varswitch:downloads",
  sparkles: "varswitch:sparkles",
};

function redisConfig() {
  const url = process.env.KV_REST_API_URL || process.env.UPSTASH_REDIS_REST_URL;
  const token = process.env.KV_REST_API_TOKEN || process.env.UPSTASH_REDIS_REST_TOKEN;
  return url && token ? { url: url.replace(/\/$/, ""), token } : null;
}

async function redisCommand(config, command, ...args) {
  const path = [command, ...args].map((value) => encodeURIComponent(value)).join("/");
  const response = await fetch(`${config.url}/${path}`, {
    headers: { Authorization: `Bearer ${config.token}` },
  });
  if (!response.ok) throw new Error(`Redis request failed (${response.status})`);
  const payload = await response.json();
  return payload.result;
}

function sendJson(res, status, body) {
  res.status(status).setHeader("Cache-Control", "no-store");
  res.setHeader("Content-Type", "application/json; charset=utf-8");
  res.end(JSON.stringify(body));
}

module.exports = async function handler(req, res) {
  if (!["GET", "POST", "OPTIONS"].includes(req.method)) {
    res.setHeader("Allow", "GET, POST, OPTIONS");
    return sendJson(res, 405, { error: "method_not_allowed" });
  }
  if (req.method === "OPTIONS") {
    res.setHeader("Allow", "GET, POST, OPTIONS");
    return sendJson(res, 204, {});
  }

  const metric = String(req.query?.metric || (req.method === "POST" ? req.body?.metric : "all"));
  const config = redisConfig();
  if (!config) return sendJson(res, 503, { error: "counter_unconfigured" });

  try {
    if (metric === "all") {
      const [downloads, sparkles] = await Promise.all([
        redisCommand(config, "get", METRICS.downloads),
        redisCommand(config, "get", METRICS.sparkles),
      ]);
      return sendJson(res, 200, {
        downloads: Number(downloads || 0),
        sparkles: Number(sparkles || 0),
      });
    }
    if (!Object.prototype.hasOwnProperty.call(METRICS, metric)) {
      return sendJson(res, 400, { error: "invalid_metric" });
    }
    const value = req.method === "POST"
      ? await redisCommand(config, "incr", METRICS[metric])
      : await redisCommand(config, "get", METRICS[metric]);
    return sendJson(res, 200, { metric, count: Number(value || 0) });
  } catch (error) {
    console.error("download counter error", error);
    return sendJson(res, 502, { error: "counter_unavailable" });
  }
};
