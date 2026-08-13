//! 供应商余额 / 额度查询：根据配置的 Base URL 识别供应商，调用对应余额接口。
//!
//! ── 调研结论（2026-08 官方文档核实）─────────────────────────────────────────
//!
//! 1. DeepSeek：`GET https://api.deepseek.com/user/balance`，Bearer 鉴权，
//!    返回 `balance_infos[]`（currency / total_balance / granted_balance / topped_up_balance，
//!    金额为字符串）。文档：https://api-docs.deepseek.com/api/get-user-balance
//! 2. Kimi / Moonshot：`GET {origin}/v1/users/me/balance`，
//!    返回 `data.{available_balance, voucher_balance, cash_balance}`（数字）。
//!    api.moonshot.cn / api.kimi.com 计价 CNY；api.moonshot.ai / api.kimi.ai 计价 USD。
//!    文档：https://platform.kimi.com/docs/api/balance
//! 3. SiliconFlow：`GET {origin}/v1/user/info`，
//!    返回 `data.{balance(赠费), chargeBalance(充值), totalBalance(总可用)}`（字符串）。
//!    文档：https://docs.siliconflow.com/cn/api-reference/userinfo/get-user-info
//! 4. OpenRouter：`GET {origin}/api/v1/credits`（需要管理密钥）返回
//!    `data.{total_credits, total_usage}`；普通推理密钥退回 `GET {origin}/api/v1/key`，
//!    返回 `data.{usage, limit, limit_remaining}`。均为 USD。
//! 5. NewAPI / one-api 系中转站：兼容 OpenAI 计费接口
//!    `GET {root}(/v1)/dashboard/billing/subscription` 的 `hard_limit_usd` 为总额度，
//!    `GET {root}(/v1)/dashboard/billing/usage` 的 `total_usage` 单位为 0.01 美元，
//!    余额 = hard_limit_usd - total_usage / 100。
//!    文档：https://doc.newapi.pro/api/fei-account-billing-panel/
//!
//! 官方大厂端点（Anthropic / OpenAI / Google / xAI / 火山 / 千帆等）没有公开的
//! Key 维度余额接口，直接标记为不支持，避免发出无意义的请求。

use crate::*;

const BALANCE_HTTP_TIMEOUT_SECS: u64 = 8;

/// new-api / one-api 对「不限额度」的令牌会把 hard_limit_usd 返回成 1e8 这类哨兵值
/// （本机实测多个中转站均为 100000000，个别站点甚至是 2e12）。拿它减去已用额度会得到
/// 「余额 $99,999,971」这种荒唐数字，因此超过该阈值一律视为站点未设上限，只展示真实已用金额。
const UNLIMITED_QUOTA_THRESHOLD: f64 = 1_000_000.0;

#[derive(Serialize, Clone, Default, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BalanceResult {
    /// false 表示识别出该供应商没有可用的余额接口（前端显示「不支持」）
    pub(crate) supported: bool,
    pub(crate) provider: String,
    /// "CNY" | "USD" | ""
    pub(crate) currency: String,
    /// 剩余可用余额（主展示数值）
    pub(crate) balance: Option<f64>,
    /// 总额度（充值总额 / 订阅硬上限）
    pub(crate) total_quota: Option<f64>,
    /// 已用金额
    pub(crate) used: Option<f64>,
    /// 赠送 / 代金券部分
    pub(crate) granted: Option<f64>,
    /// 充值 / 现金部分
    pub(crate) topped_up: Option<f64>,
    /// 站点未设置额度上限，此时只有 used 有意义（前端据此改成展示已用金额）
    pub(crate) unlimited_quota: bool,
    /// 配了站点访问令牌但查询失败时的原因，结果本身来自计费接口的回退路径
    pub(crate) site_token_error: Option<String>,
}

#[derive(Debug, PartialEq)]
enum BalanceProvider {
    DeepSeek,
    /// usd = true 表示国际站（api.moonshot.ai / api.kimi.ai）
    Moonshot { usd: bool },
    SiliconFlow { usd: bool },
    OpenRouter,
    /// 未识别的第三方站点：尝试 one-api / new-api 系通用计费接口
    OpenAiCompat,
    Unsupported,
}

/// 已知没有 Key 维度余额接口的官方端点域名（含各子域）
const UNSUPPORTED_DOMAINS: [&str; 14] = [
    "anthropic.com",
    "openai.com",
    "googleapis.com",
    "x.ai",
    "bigmodel.cn",
    "aliyuncs.com",
    "volces.com",
    "baidubce.com",
    "groq.com",
    "longcat.chat",
    "iflow.cn",
    "minimax.chat",
    "minimaxi.com",
    "azure.com",
];

fn detect_balance_provider(host: &str) -> BalanceProvider {
    let host = host.to_ascii_lowercase();
    let matches_domain =
        |domain: &str| host == domain || host.ends_with(&format!(".{domain}"));

    if matches_domain("deepseek.com") {
        return BalanceProvider::DeepSeek;
    }
    if matches_domain("moonshot.cn") || matches_domain("kimi.com") {
        return BalanceProvider::Moonshot { usd: false };
    }
    if matches_domain("moonshot.ai") || matches_domain("kimi.ai") {
        return BalanceProvider::Moonshot { usd: true };
    }
    if matches_domain("siliconflow.cn") {
        return BalanceProvider::SiliconFlow { usd: false };
    }
    if matches_domain("siliconflow.com") {
        return BalanceProvider::SiliconFlow { usd: true };
    }
    if matches_domain("openrouter.ai") {
        return BalanceProvider::OpenRouter;
    }
    if UNSUPPORTED_DOMAINS.iter().any(|domain| matches_domain(domain)) {
        return BalanceProvider::Unsupported;
    }
    // 本地代理 / 回环地址没有余额概念
    if matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1" | "[::1]") {
        return BalanceProvider::Unsupported;
    }
    BalanceProvider::OpenAiCompat
}

fn unsupported_result() -> BalanceResult {
    BalanceResult {
        supported: false,
        provider: "unsupported".into(),
        ..Default::default()
    }
}

/// 金额字段兼容数字与字符串两种写法（DeepSeek / SiliconFlow 返回字符串）
fn json_number(value: Option<&serde_json::Value>) -> Option<f64> {
    let value = value?;
    if let Some(n) = value.as_f64() {
        return Some(n);
    }
    value.as_str().and_then(|raw| raw.trim().parse::<f64>().ok())
}

/// scheme://host[:port]，去掉 Base URL 里的路径部分
fn url_origin(url: &reqwest::Url) -> String {
    let mut origin = format!("{}://{}", url.scheme(), url.host_str().unwrap_or_default());
    if let Some(port) = url.port() {
        origin.push_str(&format!(":{port}"));
    }
    origin
}

/// 站点令牌的存储键：host[:port]，与前端 `new URL(x).host` 保持一致，
/// 这样同一中转站的多套配置共用一份令牌，不必重复填写。
pub(crate) fn site_token_key(base_url: &str) -> Result<String, String> {
    let normalized = normalize_endpoint_url(base_url)?;
    let parsed = reqwest::Url::parse(&normalized).map_err(|e| format!("URL 无效: {e}"))?;
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    if host.is_empty() {
        return Err("URL 缺少主机名".to_string());
    }
    Ok(match parsed.port() {
        Some(port) => format!("{host}:{port}"),
        None => host,
    })
}

/// one-api / new-api 计费接口挂在站点根路径下：
/// 去掉 Base URL 尾部的 /v1、/anthropic、/openai 等协议段，保留部署子路径。
fn openai_compat_root(base: &str) -> String {
    let mut root = base.trim_end_matches('/').to_string();
    loop {
        let lower = root.to_ascii_lowercase();
        let stripped = ["/v1", "/anthropic", "/openai"].iter().find_map(|suffix| {
            lower
                .ends_with(suffix)
                .then(|| root[..root.len() - suffix.len()].to_string())
        });
        match stripped {
            Some(next) => root = next.trim_end_matches('/').to_string(),
            None => break,
        }
    }
    root
}

/// 距今 offset_days 天的日期字符串（UTC，YYYY-MM-DD），用于 usage 查询区间
fn days_offset_date(offset_days: i64) -> String {
    let days = (chrono_timestamp_millis() / 86_400_000) as i64 + offset_days;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

fn http_error_message(status: u16) -> String {
    match status {
        401 => "HTTP 401：API Key 无效或已过期".to_string(),
        403 => "HTTP 403：该 API Key 无权限查询".to_string(),
        429 => "HTTP 429：请求过于频繁，请稍后再试".to_string(),
        other => format!("HTTP {other}"),
    }
}

// ── 站点访问令牌（查询中转站账户余额）────────────────────────────────────────
//
// 中转站把令牌设为「不限额度」时，计费接口只能给出已用金额。要拿到账户里真正剩多少，
// 得用站点后台「个人设置」里生成的系统访问令牌调 /api/user/self——sk- 开头的 API Key
// 在该接口上一律 401。new-api 还要求附带数字用户 ID 的 New-Api-User 头（one-api 不需要）。
// 文档：https://doc.newapi.pro/api/fei-user/

#[derive(Serialize, Deserialize, Clone, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SiteBalanceToken {
    pub(crate) token: String,
    /// new-api 必填的数字用户 ID；one-api 留空即可
    #[serde(default)]
    pub(crate) user_id: String,
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub(crate) struct SiteBalanceTokensData {
    #[serde(default)]
    pub(crate) tokens: HashMap<String, SiteBalanceToken>,
}

/// 回传给前端的条目：令牌本身打码，只用于展示「已配置」状态
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SiteBalanceTokenInfo {
    pub(crate) host: String,
    pub(crate) masked_token: String,
    pub(crate) user_id: String,
}

/// 站点令牌等同于账户凭据，与 profiles 一样存在数据目录，且只写私有权限文件
fn site_tokens_path(app: &tauri::AppHandle) -> PathBuf {
    data_dir(app).join("site_balance_tokens.json")
}

fn read_site_tokens(app: &tauri::AppHandle) -> SiteBalanceTokensData {
    let path = site_tokens_path(app);
    if !path.exists() {
        return SiteBalanceTokensData::default();
    }
    let mut data: SiteBalanceTokensData = fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();
    for (host, entry) in data.tokens.iter_mut() {
        entry.token = decrypt_secret_or_keep(&entry.token, &format!("{host} 的站点令牌"));
    }
    data
}

fn write_site_tokens(app: &tauri::AppHandle, data: &SiteBalanceTokensData) -> Result<(), String> {
    let mut encrypted = data.clone();
    for entry in encrypted.tokens.values_mut() {
        entry.token = encrypt_secret(&entry.token);
    }
    let json = serde_json::to_string_pretty(&encrypted).map_err(|e| e.to_string())?;
    write_private_file(&site_tokens_path(app), &json)
}

/// 打码规则与 Deep Link 导入确认框一致：保留前 6 后 4
fn mask_secret(value: &str) -> String {
    let chars: Vec<char> = value.trim().chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    if chars.len() < 12 {
        return "*".repeat(chars.len());
    }
    let head: String = chars[..6].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}****{tail}")
}

#[tauri::command]
pub(crate) fn get_site_balance_tokens(
    app: tauri::AppHandle,
) -> Result<Vec<SiteBalanceTokenInfo>, String> {
    let data = read_site_tokens(&app);
    let mut list: Vec<SiteBalanceTokenInfo> = data
        .tokens
        .into_iter()
        .map(|(host, entry)| SiteBalanceTokenInfo {
            host,
            masked_token: mask_secret(&entry.token),
            user_id: entry.user_id,
        })
        .collect();
    list.sort_by(|a, b| a.host.cmp(&b.host));
    Ok(list)
}

#[tauri::command]
pub(crate) fn save_site_balance_token(
    app: tauri::AppHandle,
    base_url: String,
    token: String,
    user_id: Option<String>,
) -> Result<String, String> {
    let host = site_token_key(&base_url)?;
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err("访问令牌不能为空".to_string());
    }
    let user_id = user_id.unwrap_or_default().trim().to_string();
    if !user_id.is_empty() && user_id.parse::<u64>().is_err() {
        return Err("用户 ID 必须是数字".to_string());
    }
    let mut data = read_site_tokens(&app);
    data.tokens
        .insert(host.clone(), SiteBalanceToken { token, user_id });
    write_site_tokens(&app, &data)?;
    log_info!("[balance] 已保存 {host} 的站点访问令牌");
    Ok(host)
}

#[tauri::command]
pub(crate) fn delete_site_balance_token(
    app: tauri::AppHandle,
    base_url: String,
) -> Result<String, String> {
    let host = site_token_key(&base_url)?;
    let mut data = read_site_tokens(&app);
    data.tokens.remove(&host);
    write_site_tokens(&app, &data)?;
    log_info!("[balance] 已删除 {host} 的站点访问令牌");
    Ok(host)
}

/// 站点把 quota 换算成货币的除数，从公开的 /api/status 读取，取不到时用 new-api 默认值
fn fetch_quota_per_unit(client: &reqwest::blocking::Client, origin: &str) -> f64 {
    const DEFAULT_QUOTA_PER_UNIT: f64 = 500_000.0;
    client
        .get(format!("{origin}/api/status"))
        .header("Accept", "application/json")
        .send()
        .ok()
        .filter(|resp| resp.status().is_success())
        .and_then(|resp| resp.json::<serde_json::Value>().ok())
        .and_then(|body| json_number(body.get("data")?.get("quota_per_unit")))
        .filter(|value| *value > 0.0)
        .unwrap_or(DEFAULT_QUOTA_PER_UNIT)
}

/// 取出 /api/user/self 里的原始 quota（尚未按站点系数折算）。
/// success 为 false 时把站点原文当错误抛出，便于用户直接看到
/// 「New-Api-User header is required」这类提示。
fn extract_account_quota(body: &serde_json::Value) -> Result<(f64, Option<f64>), String> {
    if body.get("success").and_then(|v| v.as_bool()) == Some(false) {
        let message = body
            .get("message")
            .and_then(|v| v.as_str())
            .filter(|text| !text.trim().is_empty())
            .unwrap_or("站点拒绝了该访问令牌");
        return Err(message.to_string());
    }
    let data = body.get("data").ok_or("账户响应格式无法识别")?;
    let quota = json_number(data.get("quota")).ok_or("账户响应缺少额度字段")?;
    Ok((quota, json_number(data.get("used_quota"))))
}

/// 原始 quota 按站点的 quota_per_unit 折算成货币金额
fn build_site_account_result(quota: f64, used: Option<f64>, quota_per_unit: f64) -> BalanceResult {
    let divisor = if quota_per_unit > 0.0 { quota_per_unit } else { 1.0 };
    BalanceResult {
        supported: true,
        provider: "site_account".into(),
        currency: "USD".into(),
        balance: Some(quota / divisor),
        used: used.map(|value| value / divisor),
        ..Default::default()
    }
}

/// 用站点访问令牌查账户余额。one-api 要求裸令牌，new-api 允许 Bearer 前缀，
/// 因此裸令牌失败时再退一次 Bearer，覆盖两种实现。
fn query_site_account(
    client: &reqwest::blocking::Client,
    origin: &str,
    entry: &SiteBalanceToken,
) -> Result<BalanceResult, String> {
    let url = format!("{origin}/api/user/self");
    let mut last_error = String::new();
    for authorization in [entry.token.clone(), format!("Bearer {}", entry.token)] {
        let mut request = client
            .get(&url)
            .header("Authorization", authorization)
            .header("Accept", "application/json");
        if !entry.user_id.is_empty() {
            request = request.header("New-Api-User", entry.user_id.clone());
        }
        let response = request.send().map_err(|e| {
            if e.is_timeout() {
                "请求超时".to_string()
            } else if e.is_connect() {
                "连接失败".to_string()
            } else {
                e.to_string()
            }
        })?;
        let status = response.status().as_u16();
        let body = response
            .json::<serde_json::Value>()
            .unwrap_or(serde_json::Value::Null);
        match extract_account_quota(&body) {
            // 换算系数只在确定拿到额度后才去取，避免为无效令牌多打一次请求
            Ok((quota, used)) => {
                return Ok(build_site_account_result(
                    quota,
                    used,
                    fetch_quota_per_unit(client, origin),
                ))
            }
            Err(message) => {
                last_error = if status == 200 {
                    message
                } else {
                    format!("HTTP {status}：{message}")
                };
            }
        }
    }
    Err(last_error)
}

/// 发起 GET 请求并解析 JSON。网络层错误转成用户可读的中文提示。
fn balance_get(
    client: &reqwest::blocking::Client,
    url: &str,
    api_key: &str,
) -> Result<(u16, serde_json::Value), String> {
    let response = client
        .get(url)
        .bearer_auth(api_key)
        .header("Accept", "application/json")
        .send()
        .map_err(|e| {
            if e.is_timeout() {
                "请求超时".to_string()
            } else if e.is_connect() {
                "连接失败".to_string()
            } else {
                e.to_string()
            }
        })?;
    let status = response.status().as_u16();
    let body = response
        .json::<serde_json::Value>()
        .unwrap_or(serde_json::Value::Null);
    Ok((status, body))
}

// ── 各供应商响应解析（纯函数，便于单测）─────────────────────────────────────

fn parse_deepseek(body: &serde_json::Value) -> Option<BalanceResult> {
    let info = body.get("balance_infos")?.as_array()?.first()?;
    let total = json_number(info.get("total_balance"))?;
    Some(BalanceResult {
        supported: true,
        provider: "deepseek".into(),
        currency: info
            .get("currency")
            .and_then(|v| v.as_str())
            .unwrap_or("CNY")
            .to_string(),
        balance: Some(total),
        granted: json_number(info.get("granted_balance")),
        topped_up: json_number(info.get("topped_up_balance")),
        ..Default::default()
    })
}

fn parse_moonshot(body: &serde_json::Value, usd: bool) -> Option<BalanceResult> {
    let data = body.get("data")?;
    let available = json_number(data.get("available_balance"))?;
    Some(BalanceResult {
        supported: true,
        provider: "moonshot".into(),
        currency: if usd { "USD" } else { "CNY" }.into(),
        balance: Some(available),
        granted: json_number(data.get("voucher_balance")),
        topped_up: json_number(data.get("cash_balance")),
        ..Default::default()
    })
}

fn parse_siliconflow(body: &serde_json::Value, usd: bool) -> Option<BalanceResult> {
    let data = body.get("data")?;
    let total = json_number(data.get("totalBalance"))
        .or_else(|| json_number(data.get("balance")))?;
    Some(BalanceResult {
        supported: true,
        provider: "siliconflow".into(),
        currency: if usd { "USD" } else { "CNY" }.into(),
        balance: Some(total),
        granted: json_number(data.get("balance")),
        topped_up: json_number(data.get("chargeBalance")),
        ..Default::default()
    })
}

fn parse_openrouter_credits(body: &serde_json::Value) -> Option<BalanceResult> {
    let data = body.get("data")?;
    let total = json_number(data.get("total_credits"))?;
    let used = json_number(data.get("total_usage")).unwrap_or(0.0);
    Some(BalanceResult {
        supported: true,
        provider: "openrouter".into(),
        currency: "USD".into(),
        balance: Some(total - used),
        total_quota: Some(total),
        used: Some(used),
        ..Default::default()
    })
}

/// 推理密钥查询 /api/v1/key：limit 可能为 null（不限额），此时只展示已用金额
fn parse_openrouter_key(body: &serde_json::Value) -> Option<BalanceResult> {
    let data = body.get("data")?;
    let used = json_number(data.get("usage"));
    let limit = json_number(data.get("limit"));
    let remaining = json_number(data.get("limit_remaining"))
        .or(match (limit, used) {
            (Some(limit), Some(used)) => Some(limit - used),
            _ => None,
        });
    if used.is_none() && remaining.is_none() {
        return None;
    }
    Some(BalanceResult {
        supported: true,
        provider: "openrouter".into(),
        currency: "USD".into(),
        balance: remaining,
        total_quota: limit,
        used,
        ..Default::default()
    })
}

// ── 命令入口 ────────────────────────────────────────────────────────────────

/// 余额查询要发同步 HTTP 请求，最坏情况（未知中转站探测多个端点）会占用调用线程数十秒。
/// Tauri 的同步命令跑在主线程上，直接同步执行会冻结窗口，因此命令声明为 async 并把
/// 实际查询交给阻塞线程池，主线程只负责收发 IPC。
#[tauri::command]
pub(crate) async fn query_provider_balance(
    app: tauri::AppHandle,
    base_url: String,
    api_key: String,
) -> Result<BalanceResult, String> {
    // 站点令牌在进入线程池前读好，闭包里就不必再碰 AppHandle
    let site_token = site_token_key(&base_url)
        .ok()
        .and_then(|host| read_site_tokens(&app).tokens.remove(&host))
        .filter(|entry| !entry.token.trim().is_empty());
    tauri::async_runtime::spawn_blocking(move || {
        query_provider_balance_blocking(base_url, api_key, site_token)
    })
    .await
    .map_err(|e| format!("余额查询任务异常退出: {e}"))?
}

fn query_provider_balance_blocking(
    base_url: String,
    api_key: String,
    site_token: Option<SiteBalanceToken>,
) -> Result<BalanceResult, String> {
    let key = api_key.trim();
    if key.is_empty() {
        return Err("API Key 为空，无法查询余额".to_string());
    }
    let normalized = normalize_endpoint_url(&base_url)?;
    let parsed = reqwest::Url::parse(&normalized).map_err(|e| format!("URL 无效: {e}"))?;
    let host = parsed.host_str().unwrap_or_default().to_string();
    let origin = url_origin(&parsed);
    let provider = detect_balance_provider(&host);
    if provider == BalanceProvider::Unsupported {
        return Ok(unsupported_result());
    }

    let client = build_http_client(BALANCE_HTTP_TIMEOUT_SECS)?;
    match provider {
        BalanceProvider::DeepSeek => {
            let (status, body) = balance_get(&client, &format!("{origin}/user/balance"), key)?;
            if status != 200 {
                return Err(http_error_message(status));
            }
            parse_deepseek(&body).ok_or_else(|| "余额响应格式无法识别".to_string())
        }
        BalanceProvider::Moonshot { usd } => {
            let (status, body) =
                balance_get(&client, &format!("{origin}/v1/users/me/balance"), key)?;
            if status != 200 {
                return Err(http_error_message(status));
            }
            parse_moonshot(&body, usd).ok_or_else(|| "余额响应格式无法识别".to_string())
        }
        BalanceProvider::SiliconFlow { usd } => {
            let (status, body) = balance_get(&client, &format!("{origin}/v1/user/info"), key)?;
            if status != 200 {
                return Err(http_error_message(status));
            }
            parse_siliconflow(&body, usd).ok_or_else(|| "余额响应格式无法识别".to_string())
        }
        BalanceProvider::OpenRouter => {
            // /credits 需要管理密钥；普通推理密钥会 401/403，退回 /key 查询当前密钥额度
            let (status, body) =
                balance_get(&client, &format!("{origin}/api/v1/credits"), key)?;
            if status == 200 {
                if let Some(result) = parse_openrouter_credits(&body) {
                    return Ok(result);
                }
            }
            let (key_status, key_body) =
                balance_get(&client, &format!("{origin}/api/v1/key"), key)?;
            if key_status != 200 {
                return Err(http_error_message(if key_status == 0 { status } else { key_status }));
            }
            parse_openrouter_key(&key_body).ok_or_else(|| "余额响应格式无法识别".to_string())
        }
        BalanceProvider::OpenAiCompat => {
            // 配了站点访问令牌就先查账户真实余额；失败不直接报错，退回计费接口
            // 至少还能给出已用金额，同时把失败原因带回前端提示用户
            let mut site_token_error = None;
            if let Some(entry) = site_token.as_ref() {
                match query_site_account(&client, &origin, entry) {
                    Ok(result) => return Ok(result),
                    Err(message) => {
                        log_warn!("[balance] {host} 站点令牌查询失败，回退计费接口");
                        site_token_error = Some(message);
                    }
                }
            }
            query_openai_compat(&client, &normalized, key, &host).map(|mut result| {
                result.site_token_error = site_token_error;
                result
            })
        }
        BalanceProvider::Unsupported => Ok(unsupported_result()),
    }
}

/// 由 subscription 的额度上限与 usage 的已用金额组合出展示结果。
/// 返回 None 表示两者都不可信，调用方应继续尝试其他端点。
fn build_compat_result(total_quota: f64, used: Option<f64>) -> Option<BalanceResult> {
    let unlimited = total_quota >= UNLIMITED_QUOTA_THRESHOLD;
    if unlimited && used.is_none() {
        return None;
    }
    Some(BalanceResult {
        supported: true,
        provider: "openai_compat".into(),
        currency: "USD".into(),
        balance: if unlimited { None } else { used.map(|u| total_quota - u) },
        total_quota: if unlimited { None } else { Some(total_quota) },
        used,
        unlimited_quota: unlimited,
        ..Default::default()
    })
}

/// one-api / new-api 系通用计费接口。站点没实现时返回「不支持」而非报错，
/// 让前端安静地显示 N/A；鉴权失败（401/403）仍然作为错误反馈给用户。
fn query_openai_compat(
    client: &reqwest::blocking::Client,
    normalized_base: &str,
    api_key: &str,
    host: &str,
) -> Result<BalanceResult, String> {
    let root = openai_compat_root(normalized_base);
    let mut last_error: Option<String> = None;
    for prefix in [format!("{root}/v1"), root.clone()] {
        let subscription_url = format!("{prefix}/dashboard/billing/subscription");
        let (status, body) = match balance_get(client, &subscription_url, api_key) {
            Ok(pair) => pair,
            Err(e) => {
                last_error = Some(e);
                continue;
            }
        };
        if status == 401 || status == 403 {
            // WAF / 网关拦截返回的是 HTML 而非 JSON，那是整站防护而不是 Key 失效，
            // 不能报「API Key 无效」误导用户，按不支持处理
            if body.is_null() {
                return Ok(unsupported_result());
            }
            return Err(http_error_message(status));
        }
        if status != 200 {
            last_error = Some(http_error_message(status));
            continue;
        }
        let Some(total_quota) = json_number(body.get("hard_limit_usd")) else {
            last_error = Some("余额响应格式无法识别".to_string());
            continue;
        };
        // usage 查询失败不阻塞：还有可信额度时至少能展示总额度
        let usage_url = format!(
            "{prefix}/dashboard/billing/usage?start_date={}&end_date={}",
            days_offset_date(-99),
            days_offset_date(1)
        );
        // total_usage 的单位是 0.01 美元
        let used = balance_get(client, &usage_url, api_key)
            .ok()
            .filter(|(status, _)| *status == 200)
            .and_then(|(_, body)| json_number(body.get("total_usage")))
            .map(|value| value / 100.0);
        match build_compat_result(total_quota, used) {
            Some(result) => return Ok(result),
            None => {
                // 额度是哨兵值又拿不到已用金额，没有任何可展示的数据
                last_error = Some("站点未返回可用的额度数据".to_string());
                continue;
            }
        }
    }
    log_info!(
        "[balance] {host} 未实现通用计费接口，视为不支持余额查询（{}）",
        last_error.unwrap_or_default()
    );
    Ok(unsupported_result())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn detects_known_providers_by_host() {
        assert_eq!(detect_balance_provider("api.deepseek.com"), BalanceProvider::DeepSeek);
        assert_eq!(
            detect_balance_provider("api.moonshot.cn"),
            BalanceProvider::Moonshot { usd: false }
        );
        assert_eq!(
            detect_balance_provider("api.kimi.ai"),
            BalanceProvider::Moonshot { usd: true }
        );
        assert_eq!(
            detect_balance_provider("api.siliconflow.cn"),
            BalanceProvider::SiliconFlow { usd: false }
        );
        assert_eq!(detect_balance_provider("openrouter.ai"), BalanceProvider::OpenRouter);
    }

    #[test]
    fn official_endpoints_without_balance_api_are_unsupported() {
        assert_eq!(detect_balance_provider("api.anthropic.com"), BalanceProvider::Unsupported);
        assert_eq!(detect_balance_provider("api.openai.com"), BalanceProvider::Unsupported);
        assert_eq!(detect_balance_provider("open.bigmodel.cn"), BalanceProvider::Unsupported);
        assert_eq!(detect_balance_provider("127.0.0.1"), BalanceProvider::Unsupported);
    }

    #[test]
    fn unknown_hosts_fall_back_to_openai_compat_probe() {
        assert_eq!(
            detect_balance_provider("relay.example.com"),
            BalanceProvider::OpenAiCompat
        );
    }

    #[test]
    fn host_matching_requires_domain_boundary() {
        // 恶意注册的 fake-deepseek.com 之类域名不能被误判成官方供应商
        assert_eq!(
            detect_balance_provider("fakedeepseek.com"),
            BalanceProvider::OpenAiCompat
        );
        assert_eq!(
            detect_balance_provider("deepseek.com.evil.com"),
            BalanceProvider::OpenAiCompat
        );
    }

    #[test]
    fn compat_root_strips_protocol_suffixes_only() {
        assert_eq!(openai_compat_root("https://relay.test/v1"), "https://relay.test");
        assert_eq!(openai_compat_root("https://relay.test/anthropic"), "https://relay.test");
        assert_eq!(
            openai_compat_root("https://relay.test/api/v1"),
            "https://relay.test/api"
        );
        assert_eq!(openai_compat_root("https://relay.test"), "https://relay.test");
    }

    #[test]
    fn parses_deepseek_string_amounts() {
        let body = json!({
            "is_available": true,
            "balance_infos": [{
                "currency": "CNY",
                "total_balance": "110.00",
                "granted_balance": "10.00",
                "topped_up_balance": "100.00"
            }]
        });
        let result = parse_deepseek(&body).expect("should parse");
        assert_eq!(result.currency, "CNY");
        assert_eq!(result.balance, Some(110.0));
        assert_eq!(result.granted, Some(10.0));
        assert_eq!(result.topped_up, Some(100.0));
    }

    #[test]
    fn parses_moonshot_numeric_amounts() {
        let body = json!({
            "code": 0,
            "data": {
                "available_balance": 49.58894,
                "voucher_balance": 46.58893,
                "cash_balance": 3.00001
            },
            "status": true
        });
        let result = parse_moonshot(&body, false).expect("should parse");
        assert_eq!(result.currency, "CNY");
        assert_eq!(result.balance, Some(49.58894));
    }

    #[test]
    fn parses_siliconflow_total_balance() {
        let body = json!({
            "code": 20000,
            "status": true,
            "data": { "balance": "0.88", "chargeBalance": "88.00", "totalBalance": "88.88" }
        });
        let result = parse_siliconflow(&body, false).expect("should parse");
        assert_eq!(result.balance, Some(88.88));
        assert_eq!(result.topped_up, Some(88.0));
    }

    #[test]
    fn parses_openrouter_credits_and_key_fallback() {
        let credits = json!({ "data": { "total_credits": 100.5, "total_usage": 25.75 } });
        let result = parse_openrouter_credits(&credits).expect("should parse");
        assert_eq!(result.balance, Some(74.75));
        assert_eq!(result.total_quota, Some(100.5));

        let key_info = json!({ "data": { "usage": 12.5, "limit": null, "limit_remaining": null } });
        let result = parse_openrouter_key(&key_info).expect("should parse");
        assert_eq!(result.balance, None);
        assert_eq!(result.used, Some(12.5));
    }

    #[test]
    fn newapi_unlimited_sentinel_quota_shows_usage_instead_of_fake_balance() {
        // 本机实测：多个 new-api 中转站的 hard_limit_usd 恒为 1e8，
        // 直接相减会得出「余额 $99,999,971」，必须退化成只展示已用金额
        let result = build_compat_result(100_000_000.0, Some(28.79)).expect("should build");
        assert_eq!(result.balance, None);
        assert_eq!(result.total_quota, None);
        assert_eq!(result.used, Some(28.79));
        assert!(result.unlimited_quota);

        // 个别站点返回 2e12，同样属于哨兵值
        let result = build_compat_result(2_000_000_000_001.76, Some(5765.51)).expect("should build");
        assert_eq!(result.balance, None);
        assert!(result.unlimited_quota);
    }

    #[test]
    fn compat_result_computes_balance_for_real_quota() {
        let result = build_compat_result(100.0, Some(28.79)).expect("should build");
        assert_eq!(result.total_quota, Some(100.0));
        assert_eq!(result.used, Some(28.79));
        assert!((result.balance.expect("balance") - 71.21).abs() < 1e-9);
        assert!(!result.unlimited_quota);
    }

    #[test]
    fn compat_result_is_none_when_sentinel_quota_has_no_usage() {
        assert!(build_compat_result(100_000_000.0, None).is_none());
    }

    #[test]
    fn compat_result_keeps_quota_when_usage_is_unavailable() {
        let result = build_compat_result(50.0, None).expect("should build");
        assert_eq!(result.total_quota, Some(50.0));
        assert_eq!(result.balance, None);
    }

    #[test]
    fn account_quota_is_converted_with_site_quota_per_unit() {
        let body = json!({
            "success": true,
            "message": "",
            "data": { "id": 1, "quota": 1_000_000, "used_quota": 500_000 }
        });
        let (quota, used) = extract_account_quota(&body).expect("should extract");
        // 实测各中转站 /api/status 的 quota_per_unit 均为 500000
        let result = build_site_account_result(quota, used, 500_000.0);
        assert_eq!(result.balance, Some(2.0));
        assert_eq!(result.used, Some(1.0));
        assert_eq!(result.provider, "site_account");
    }

    #[test]
    fn account_error_message_is_surfaced_verbatim() {
        let body = json!({ "success": false, "message": "New-Api-User header is required" });
        let error = extract_account_quota(&body).expect_err("should fail");
        assert_eq!(error, "New-Api-User header is required");

        // 站点只回一个空 message 时给个可读兜底
        let body = json!({ "success": false, "message": "" });
        assert_eq!(
            extract_account_quota(&body).expect_err("should fail"),
            "站点拒绝了该访问令牌"
        );
    }

    #[test]
    fn site_token_key_is_host_with_port() {
        assert_eq!(site_token_key("https://77code.cn").unwrap(), "77code.cn");
        assert_eq!(
            site_token_key("http://77code.cn/v1/").unwrap(),
            "77code.cn"
        );
        // 同一站点的不同配置共用一份令牌，端口不同则视为不同站点
        assert_eq!(
            site_token_key("https://code.strova.top:8443/v1").unwrap(),
            "code.strova.top:8443"
        );
        assert!(site_token_key("not a url").is_err());
    }

    #[test]
    fn masked_secret_keeps_only_head_and_tail() {
        assert_eq!(mask_secret("abcdef1234567890"), "abcdef****7890");
        assert_eq!(mask_secret("short"), "*****");
        assert_eq!(mask_secret(""), "");
    }

    #[test]
    fn json_number_accepts_string_and_number() {
        assert_eq!(json_number(Some(&json!("12.5"))), Some(12.5));
        assert_eq!(json_number(Some(&json!(3))), Some(3.0));
        assert_eq!(json_number(Some(&json!("abc"))), None);
        assert_eq!(json_number(None), None);
    }
}
