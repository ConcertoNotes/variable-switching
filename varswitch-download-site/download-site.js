(function (root, factory) {
  if (typeof module === "object" && module.exports) module.exports = factory();
  else root.VarSwitchDownloadSite = factory();
})(typeof globalThis !== "undefined" ? globalThis : this, function () {
  const endpoint = "/api/download-count";

  function formatCount(value) {
    const count = Number(value);
    if (!Number.isFinite(count) || count < 0) return "—";
    const units = [[1e9, "B"], [1e6, "M"], [1e3, "K"]];
    for (const [threshold, suffix] of units) {
      if (count >= threshold) return `${(count / threshold).toFixed(1).replace(/\.0$/, "")}${suffix}`;
    }
    return String(Math.floor(count));
  }

  async function requestCount(fetchImpl, metric, method = "GET") {
    const url = metric ? `${endpoint}?metric=${encodeURIComponent(metric)}` : endpoint;
    const options = method === "POST"
      ? { method, headers: { "Content-Type": "application/json" }, body: JSON.stringify({ metric }) }
      : { method };
    const response = await fetchImpl(url, options);
    if (!response.ok) throw new Error(`counter request failed (${response.status})`);
    return response.json();
  }

  return { endpoint, formatCount, requestCount };
});

(function (root) {
  if (!root || !root.document || !root.VarSwitchDownloadSite) return;
  const { formatCount, requestCount } = root.VarSwitchDownloadSite;
  const doc = root.document;
  const downloadCount = doc.getElementById("downloadCount");
  const sparkleCount = doc.getElementById("sparkleCount");
  const sparkleButton = doc.getElementById("downloadCountButton");

  function renderCounts(data) {
    if (downloadCount) downloadCount.textContent = formatCount(data?.downloads);
    if (sparkleCount) sparkleCount.textContent = formatCount(data?.sparkles);
  }

  requestCount(root.fetch.bind(root), "", "GET")
    .then(renderCounts)
    .catch(() => renderCounts({}));

  if (sparkleButton) {
    const votedKey = "varswitch:sparkle-v1";
    if (root.localStorage?.getItem(votedKey) === "1") sparkleButton.classList.add("is-selected");
    sparkleButton.addEventListener("click", async () => {
      if (root.localStorage?.getItem(votedKey) === "1" || sparkleButton.disabled) return;
      sparkleButton.disabled = true;
      try {
        const result = await requestCount(root.fetch.bind(root), "sparkles", "POST");
        if (sparkleCount) sparkleCount.textContent = formatCount(result?.count);
        root.localStorage?.setItem(votedKey, "1");
        sparkleButton.classList.add("is-selected");
      } catch (_) {
        // 计数接口不可用时不影响页面其他功能。
      } finally {
        sparkleButton.disabled = false;
      }
    });
  }

  doc.querySelectorAll("[data-download-track]").forEach((link) => {
    link.addEventListener("click", (event) => {
      if (event.defaultPrevented) return;
      event.preventDefault();
      const href = link.href;
      requestCount(root.fetch.bind(root), "downloads", "POST")
        .catch(() => null)
        .finally(() => { root.location.href = href; });
    });
  });
})(typeof globalThis !== "undefined" ? globalThis : this);
