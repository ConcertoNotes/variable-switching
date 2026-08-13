//! Codex 工具箱域：插件市场、会话同步、手机远程（飞书 / QQ / 微信）与 smart_control（从 lib.rs 拆分，逻辑未改动）。

use crate::*;

pub(crate) static QQ_QR_ACTIVE: AtomicBool = AtomicBool::new(false);
// B2: QQ 扫码子进程句柄，用于主动取消
pub(crate) static QQ_QR_CHILD: Mutex<Option<std::process::Child>> = Mutex::new(None);
// QQ 扫码轮次。取消或超时后递增，防止旧扫码 worker 清理新一轮子进程。
pub(crate) static QQ_QR_GENERATION: AtomicU64 = AtomicU64::new(0);
// 取消与扫码 worker 的状态写入必须串行，避免取消后旧 worker 回写二维码或凭据。
pub(crate) static QQ_QR_STATE_LOCK: Mutex<()> = Mutex::new(());
pub(crate) static WECHAT_QR_ACTIVE: AtomicBool = AtomicBool::new(false);
pub(crate) static LARK_REGISTRATION_ACTIVE: AtomicBool = AtomicBool::new(false);
pub(crate) static MOBILE_REMOTE_START_ACTIVE: AtomicBool = AtomicBool::new(false);
pub(crate) static MOBILE_REMOTE_CANCEL_REQUESTED: AtomicBool = AtomicBool::new(false);
// B3: 保证看门狗单例，start→stop→start 快速操作不会产生多个看门狗
pub(crate) static WATCHDOG_ACTIVE: AtomicBool = AtomicBool::new(false);
// B6: 连接代际计数器，每次新 WS 连接递增，清理时校验代际避免新连接被旧连接收尾代码打翻
pub(crate) static SMART_CONTROL_WS_GENERATION: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
// B16: 飞书/QQ token 缓存（避免每条消息重新鉴权，防止触发限频）
// 格式：app_id → (token, expiry_unix_secs)
pub(crate) static LARK_TOKEN_CACHE: OnceLock<Mutex<std::collections::HashMap<String, (String, u64)>>> =
    OnceLock::new();
pub(crate) static QQ_TOKEN_CACHE: OnceLock<Mutex<std::collections::HashMap<String, (String, u64)>>> =
    OnceLock::new();
// B9: 每通道一把消息处理串行锁，防止并发处理同一通道的消息导致 Codex 乱序响应
// （完整队列+超时+去重需重构整个消息流，这里做最小化串行互斥）
pub(crate) static LARK_MSG_LOCK: Mutex<()> = Mutex::new(());
pub(crate) static QQ_MSG_LOCK: Mutex<()> = Mutex::new(());
pub(crate) static WECHAT_MSG_LOCK: Mutex<()> = Mutex::new(());
pub(crate) static LARK_BRIDGE_ACTIVE: AtomicBool = AtomicBool::new(false);
pub(crate) static LARK_BRIDGE_CHILD: Mutex<Option<std::process::Child>> = Mutex::new(None);
pub(crate) static QQ_GATEWAY_ACTIVE: AtomicBool = AtomicBool::new(false);
pub(crate) static QQ_GATEWAY_CHILD: Mutex<Option<std::process::Child>> = Mutex::new(None);
pub(crate) static WECHAT_LISTENER_ACTIVE: AtomicBool = AtomicBool::new(false);
// B8: 全局写锁，保证多线程并发写 toolbox state 时不互相丢更新
pub(crate) static TOOLBOX_STATE_WRITE_LOCK: Mutex<()> = Mutex::new(());
// B18: QQ msg_seq 递增计数器，避免同毫秒并发时碰撞导致平台去重吞消息
pub(crate) static QQ_MSG_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
pub(crate) static SMART_CONTROL_STATUS_CACHE: Mutex<Option<SmartControlStatus>> = Mutex::new(None);
/// 上次探测完成的时间戳（Unix 毫秒），0 表示缓存无效
pub(crate) static SMART_CONTROL_PROBE_AT: AtomicU64 = AtomicU64::new(0);
pub(crate) static SMART_CONTROL_SERVER_ACTIVE: AtomicBool = AtomicBool::new(false);
pub(crate) static SMART_CONTROL_SERVER_CANCEL_REQUESTED: AtomicBool = AtomicBool::new(false);
pub(crate) static SMART_CONTROL_REMOTE_CONNECTED: AtomicBool = AtomicBool::new(false);
pub(crate) static SMART_CONTROL_LAST_EVENT: Mutex<Option<SmartControlEvent>> = Mutex::new(None);
pub(crate) static SMART_CONTROL_WS_WRITER: Mutex<Option<TcpStream>> = Mutex::new(None);
pub(crate) static SMART_CONTROL_NEXT_REQUEST_ID: OnceLock<Mutex<u64>> = OnceLock::new();
pub(crate) static SMART_CONTROL_PENDING: OnceLock<(Mutex<HashMap<u64, SmartControlPendingResult>>, Condvar)> =
    OnceLock::new();
pub(crate) static SMART_CONTROL_EVENT_LOG: OnceLock<Mutex<Vec<SmartControlEvent>>> = OnceLock::new();
// B9: 各通道最近收到的 message_id 去重缓冲（上限 512 条，防止平台重投消息重复触发 Codex）
pub(crate) static LARK_SEEN_MSG_IDS: OnceLock<Mutex<std::collections::VecDeque<String>>> = OnceLock::new();
pub(crate) static QQ_SEEN_MSG_IDS: OnceLock<Mutex<std::collections::VecDeque<String>>> = OnceLock::new();
// B13: 飞书注册代际计数器，防止旧 worker 把过期凭据覆盖新凭据
pub(crate) static LARK_REGISTRATION_GENERATION: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
pub(crate) static SMART_CONTROL_CLIENT_STATE: OnceLock<Mutex<SmartControlClientState>> = OnceLock::new();
pub(crate) static SMART_CONTROL_SERVER_CHUNKS: OnceLock<Mutex<HashMap<String, SmartControlChunkAssembly>>> =
    OnceLock::new();
pub(crate) static SMART_CONTROL_TURN_STREAMS: OnceLock<Mutex<HashMap<u64, SmartControlTurnAccumulator>>> =
    OnceLock::new();
pub(crate) static SMART_CONTROL_APPROVALS: OnceLock<Mutex<Vec<SmartControlApprovalRequest>>> = OnceLock::new();

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexSessionBinding {
    pub(crate) channel: String,
    pub(crate) thread_id: String,
    pub(crate) thread_name: String,
    pub(crate) session_file: String,
    pub(crate) updated_at: String,
    pub(crate) cwd: String,
    pub(crate) sync_enabled: bool,
    pub(crate) last_synced_at: String,
    pub(crate) note: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexThreadRecord {
    pub(crate) id: String,
    pub(crate) thread_name: String,
    pub(crate) updated_at: String,
    pub(crate) session_file: String,
    pub(crate) cwd: String,
    pub(crate) last_user_message: String,
    pub(crate) last_assistant_message: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TrashedCodexThreadRecord {
    pub(crate) id: String,
    pub(crate) thread_name: String,
    pub(crate) updated_at: String,
    pub(crate) session_file: String,
    pub(crate) cwd: String,
    pub(crate) last_user_message: String,
    pub(crate) last_assistant_message: String,
    pub(crate) deleted_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexSessionSyncState {
    pub(crate) last_synced_at: String,
    pub(crate) total: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PluginMarketplaceItem {
    pub(crate) name: String,
    pub(crate) source: String,
    pub(crate) source_type: String,
    pub(crate) is_official: bool,
    pub(crate) is_current: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexBuiltinPluginSkill {
    pub(crate) name: String,
    pub(crate) description: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexBuiltinPluginItem {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) display_name: String,
    pub(crate) marketplace: String,
    pub(crate) version: String,
    pub(crate) description: String,
    pub(crate) root: String,
    pub(crate) enabled: bool,
    pub(crate) important: bool,
    pub(crate) skills: Vec<CodexBuiltinPluginSkill>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexBuiltinPluginStatus {
    pub(crate) available: bool,
    pub(crate) marketplace_source: String,
    pub(crate) marketplace_configured: bool,
    pub(crate) enabled_count: usize,
    pub(crate) total_count: usize,
    pub(crate) important_enabled_count: usize,
    pub(crate) important_total_count: usize,
    pub(crate) plugins: Vec<CodexBuiltinPluginItem>,
    pub(crate) last_error: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RemoteDeviceState {
    pub(crate) enabled: bool,
    pub(crate) listen_addr: String,
    pub(crate) port: u16,
    pub(crate) discovery_token: String,
    pub(crate) last_error: String,
    pub(crate) last_started_at: String,
    pub(crate) last_seen_at: String,
    pub(crate) device_name: String,
    pub(crate) local_ip: String,
    #[serde(default)]
    pub(crate) mode: String,
    #[serde(default)]
    pub(crate) active_thread_id: String,
    #[serde(default)]
    pub(crate) active_thread_name: String,
    #[serde(default)]
    pub(crate) remote_control_preferred: bool,
    #[serde(default)]
    pub(crate) remote_control_status: String,
    #[serde(default)]
    pub(crate) remote_control_backend_url: String,
    #[serde(default)]
    pub(crate) remote_control_connected: bool,
    #[serde(default)]
    pub(crate) remote_control_detail: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SmartControlStatus {
    pub(crate) available: bool,
    pub(crate) connected: bool,
    pub(crate) backend_url: String,
    pub(crate) status: String,
    pub(crate) detail: String,
    pub(crate) checked_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SmartControlEvent {
    pub(crate) received_at: String,
    pub(crate) event_type: String,
    pub(crate) message_id: String,
    pub(crate) method: String,
    pub(crate) raw_preview: String,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SmartControlDebugSnapshot {
    pub(crate) connected: bool,
    pub(crate) pending_count: usize,
    pub(crate) last_event: Option<SmartControlEvent>,
    pub(crate) events: Vec<SmartControlEvent>,
    pub(crate) client: SmartControlClientState,
    pub(crate) approvals: Vec<SmartControlApprovalRequest>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SmartControlPendingResult {
    pub(crate) done: bool,
    pub(crate) text: String,
    pub(crate) error: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SmartControlClientState {
    pub(crate) client_id: String,
    pub(crate) stream_id: String,
    pub(crate) next_seq_id: u64,
    pub(crate) initialized: bool,
    pub(crate) cursor: String,
    pub(crate) last_pong_status: String,
    pub(crate) last_initialize_id: u64,
    pub(crate) active_thread_id: String,
}

impl Default for SmartControlClientState {
    fn default() -> Self {
        Self {
            client_id: format!("varswitch-client-{}", uuid::Uuid::new_v4()),
            stream_id: format!("varswitch-stream-{}", uuid::Uuid::new_v4()),
            next_seq_id: 1,
            initialized: false,
            cursor: String::new(),
            last_pong_status: String::new(),
            last_initialize_id: 0,
            active_thread_id: String::new(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SmartControlChunkAssembly {
    pub(crate) segment_count: usize,
    pub(crate) message_size_bytes: usize,
    pub(crate) raw: Vec<u8>,
    pub(crate) next_segment_id: usize,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SmartControlTurnAccumulator {
    pub(crate) text_parts: Vec<String>,
    pub(crate) final_text: String,
    pub(crate) error: String,
    pub(crate) done: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SmartControlApprovalRequest {
    pub(crate) request_id: String,
    pub(crate) method: String,
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) options: Vec<String>,
    pub(crate) received_at: String,
    pub(crate) raw_preview: String,
}

impl Default for RemoteDeviceState {
    fn default() -> Self {
        Self {
            enabled: false,
            listen_addr: "0.0.0.0".into(),
            port: 38527,
            discovery_token: String::new(),
            last_error: String::new(),
            last_started_at: String::new(),
            last_seen_at: String::new(),
            device_name: String::new(),
            local_ip: String::new(),
            mode: "platform_bot".into(),
            active_thread_id: String::new(),
            active_thread_name: String::new(),
            remote_control_preferred: true,
            remote_control_status: "未检测".into(),
            remote_control_backend_url: "http://127.0.0.1:3847".into(),
            remote_control_connected: false,
            remote_control_detail: String::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct MobileChannelBinding {
    pub(crate) channel: String,
    pub(crate) thread_id: String,
    pub(crate) thread_name: String,
    pub(crate) session_file: String,
    pub(crate) app_id: String,
    pub(crate) app_secret: String,
    pub(crate) bot_token: String,
    pub(crate) account_id: String,
    pub(crate) base_url: String,
    pub(crate) user_id: String,
    pub(crate) bot_open_id: String,
    pub(crate) gateway_url: String,
    pub(crate) qr_url: String,
    pub(crate) qr_data_url: String,
    pub(crate) qr_status: String,
    pub(crate) qr_device_code: String,
    pub(crate) qr_started_at: String,
    pub(crate) launcher_url: String,
    pub(crate) enabled: bool,
    pub(crate) listening: bool,
    pub(crate) status: String,
    pub(crate) last_error: String,
    pub(crate) credential_status: String,
    pub(crate) last_checked_at: String,
    pub(crate) updated_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct ToolboxState {
    pub(crate) plugin_marketplace_input: String,
    pub(crate) mobile_remote: RemoteDeviceState,
    pub(crate) session_bindings: Vec<CodexSessionBinding>,
    pub(crate) synced_codex_threads: Vec<CodexThreadRecord>,
    pub(crate) trashed_codex_threads: Vec<TrashedCodexThreadRecord>,
    pub(crate) session_sync: CodexSessionSyncState,
    pub(crate) mobile_channels: Vec<MobileChannelBinding>,
    pub(crate) selected_mobile_thread_id: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolboxSnapshot {
    pub(crate) plugin_marketplace_input: String,
    pub(crate) plugin_marketplaces: Vec<PluginMarketplaceItem>,
    pub(crate) builtin_plugins: CodexBuiltinPluginStatus,
    pub(crate) session_bindings: Vec<CodexSessionBinding>,
    pub(crate) codex_threads: Vec<CodexThreadRecord>,
    pub(crate) synced_codex_threads: Vec<CodexThreadRecord>,
    pub(crate) trashed_codex_threads: Vec<TrashedCodexThreadRecord>,
    pub(crate) session_sync: CodexSessionSyncState,
    pub(crate) mobile_channels: Vec<MobileChannelBinding>,
    pub(crate) selected_mobile_thread_id: String,
    pub(crate) mobile_remote: RemoteDeviceState,
    pub(crate) codex_home: String,
    pub(crate) codex_config_path: String,
}

pub(crate) fn toolbox_state_path(app: &tauri::AppHandle) -> PathBuf {
    data_dir(app).join("codex_toolbox_state.json")
}

pub(crate) fn default_smart_control_backend_url() -> &'static str {
    "http://127.0.0.1:3847"
}

pub(crate) fn normalize_smart_control_backend_url(value: &str) -> String {
    let mut raw = value.trim().trim_end_matches('/').to_string();
    if raw.is_empty() {
        raw = default_smart_control_backend_url().to_string();
    }
    if !raw.starts_with("http://") && !raw.starts_with("https://") {
        raw = format!("http://{raw}");
    }
    raw.trim_end_matches('/').to_string()
}

pub(crate) fn smart_control_status_from_cache() -> Option<SmartControlStatus> {
    SMART_CONTROL_STATUS_CACHE
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
}

pub(crate) fn set_smart_control_status_cache(status: SmartControlStatus) {
    if let Ok(mut guard) = SMART_CONTROL_STATUS_CACHE.lock() {
        *guard = Some(status);
    }
    SMART_CONTROL_PROBE_AT.store(chrono_timestamp_millis() as u64, Ordering::SeqCst);
}

/// 探测结果的有效期。Toolbox 页每秒刷新一次快照，而单次探测超时就有 1.5 秒，
/// 每次都真发请求会让探测互相追尾；窗口内复用上次结果，状态变化最多晚 3 秒可见。
pub(crate) const SMART_CONTROL_PROBE_TTL_MS: u64 = 3_000;

/// 带时间窗的探测：同一后端地址在 TTL 内直接复用缓存。
/// 地址变了或缓存过期才真正发请求。
pub(crate) fn probe_smart_control_backend_cached(backend_url: &str) -> SmartControlStatus {
    let last_at = SMART_CONTROL_PROBE_AT.load(Ordering::SeqCst);
    let now = chrono_timestamp_millis() as u64;
    if last_at > 0 && now.saturating_sub(last_at) < SMART_CONTROL_PROBE_TTL_MS {
        if let Some(cached) = smart_control_status_from_cache() {
            if cached.backend_url == backend_url {
                return cached;
            }
        }
    }
    probe_smart_control_backend(backend_url)
}

/// 启停控制服务后状态会立刻变化，作废缓存让下一次快照拿到真实结果
pub(crate) fn invalidate_smart_control_probe_cache() {
    SMART_CONTROL_PROBE_AT.store(0, Ordering::SeqCst);
}

pub(crate) fn probe_smart_control_backend(base_url: &str) -> SmartControlStatus {
    let backend_url = normalize_smart_control_backend_url(base_url);
    let checked_at = chrono_now();
    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(1500))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return SmartControlStatus {
                available: false,
                connected: false,
                backend_url,
                status: "高级控制通道不可用".into(),
                detail: format!("创建 HTTP 客户端失败: {error}"),
                checked_at,
            };
        }
    };

    let status_url = format!("{backend_url}/api/remote-control/status");
    let response = client.get(&status_url).send();
    let Ok(response) = response else {
        return SmartControlStatus {
            available: false,
            connected: false,
            backend_url,
            status: "未连接高级控制通道".into(),
            detail: "未检测到本机 Codex 控制服务；将自动使用兼容模式。".into(),
            checked_at,
        };
    };
    let http_status = response.status();
    let body = response.text().unwrap_or_default();
    if !http_status.is_success() {
        return SmartControlStatus {
            available: false,
            connected: false,
            backend_url,
            status: "高级控制通道响应异常".into(),
            detail: format!("HTTP {}：{}", http_status.as_u16(), body.trim()),
            checked_at,
        };
    }

    let json: serde_json::Value = serde_json::from_str(&body).unwrap_or_else(|_| {
        serde_json::json!({
            "raw": body,
        })
    });
    let connected = json
        .get("connected")
        .or_else(|| json.get("remoteControlConnected"))
        .or_else(|| json.get("ready"))
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let detail = if connected {
        json_string(
            &json,
            &["clientName", "deviceName", "activeClient", "detail"],
        )
    } else {
        json_string(&json, &["lastError", "error", "message", "detail"])
    };
    let last_event_summary = json
        .get("lastEvent")
        .and_then(|event| event.as_object())
        .map(|event| {
            let method = event
                .get("method")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let event_type = event
                .get("eventType")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let received_at = event
                .get("receivedAt")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            [event_type, method, received_at]
                .into_iter()
                .filter(|part| !part.trim().is_empty())
                .collect::<Vec<_>>()
                .join(" · ")
        })
        .filter(|summary| !summary.trim().is_empty())
        .unwrap_or_default();
    SmartControlStatus {
        available: true,
        connected,
        backend_url,
        status: if connected {
            "高级控制通道已连接".into()
        } else {
            "高级控制通道等待 Codex 连接".into()
        },
        detail: if detail.is_empty() {
            if connected {
                if last_event_summary.is_empty() {
                    "Codex 已连接，可以使用协议级手机控制。".into()
                } else {
                    format!("Codex 已连接，最近事件：{last_event_summary}")
                }
            } else {
                "本机控制服务已响应，但尚未检测到 Codex App / CLI remote-control 连接。".into()
            }
        } else {
            detail
        },
        checked_at,
    }
}

pub(crate) fn refresh_smart_control_status_for_state(state: &mut ToolboxState) -> SmartControlStatus {
    let backend_url =
        normalize_smart_control_backend_url(&state.mobile_remote.remote_control_backend_url);
    let status = probe_smart_control_backend_cached(&backend_url);
    state.mobile_remote.remote_control_backend_url = status.backend_url.clone();
    state.mobile_remote.remote_control_connected = status.connected;
    state.mobile_remote.remote_control_status = status.status.clone();
    state.mobile_remote.remote_control_detail = status.detail.clone();
    state.mobile_remote.remote_control_preferred = true;
    set_smart_control_status_cache(status.clone());
    status
}

pub(crate) fn smart_control_bind_addr_from_url(base_url: &str) -> String {
    let normalized = normalize_smart_control_backend_url(base_url);
    // 安全：本机控制服务（HTTP/WebSocket）没有鉴权，只从 URL 中取端口，
    // 强制绑定到回环地址，避免因 URL 中出现 0.0.0.0 或局域网 IP 而把无鉴权
    // 的控制通道暴露给同一局域网内的其他设备。
    let port = reqwest::Url::parse(&normalized)
        .ok()
        .and_then(|url| url.port_or_known_default())
        .unwrap_or(3847);
    format!("127.0.0.1:{port}")
}

pub(crate) fn smart_control_http_response(status: &str, content_type: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}; charset=utf-8\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: authorization, content-type, openai-sentinel-chat-requirements-token\r\nConnection: close\r\n\r\n{body}",
        body.as_bytes().len()
    )
}

pub(crate) fn http_header_value<'a>(head: &'a str, name: &str) -> Option<&'a str> {
    let target = name.to_ascii_lowercase();
    for line in head.lines().skip(1) {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if key.trim().to_ascii_lowercase() == target {
            return Some(value.trim());
        }
    }
    None
}

pub(crate) fn is_websocket_upgrade(head: &str) -> bool {
    let upgrade = http_header_value(head, "upgrade")
        .map(|value| value.to_ascii_lowercase().contains("websocket"))
        .unwrap_or(false);
    let connection = http_header_value(head, "connection")
        .map(|value| value.to_ascii_lowercase().contains("upgrade"))
        .unwrap_or(false);
    upgrade && connection && http_header_value(head, "sec-websocket-key").is_some()
}

pub(crate) fn websocket_accept_key(key: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(key.trim().as_bytes());
    hasher.update(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    let digest = hasher.finalize();
    base64_encode_bytes(&digest)
}

pub(crate) fn websocket_upgrade_response(key: &str) -> String {
    format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
        websocket_accept_key(key)
    )
}

pub(crate) fn websocket_send_text(stream: &mut TcpStream, text: &str) -> Result<(), String> {
    let data = text.as_bytes();
    let mut frame = Vec::with_capacity(data.len() + 10);
    frame.push(0x81);
    if data.len() < 126 {
        frame.push(data.len() as u8);
    } else if data.len() <= u16::MAX as usize {
        frame.push(126);
        frame.extend_from_slice(&(data.len() as u16).to_be_bytes());
    } else {
        frame.push(127);
        frame.extend_from_slice(&(data.len() as u64).to_be_bytes());
    }
    frame.extend_from_slice(data);
    stream
        .write_all(&frame)
        .map_err(|e| format!("发送 WebSocket 文本帧失败: {e}"))
}

pub(crate) fn websocket_send_pong(stream: &mut TcpStream, payload: &[u8]) -> Result<(), String> {
    let payload = &payload[..payload.len().min(125)];
    let mut frame = Vec::with_capacity(payload.len() + 2);
    frame.push(0x8a);
    frame.push(payload.len() as u8);
    frame.extend_from_slice(payload);
    stream
        .write_all(&frame)
        .map_err(|e| format!("发送 WebSocket pong 失败: {e}"))
}

// B6: timeout 与 EOF 使用不同返回值区分：
//   Ok(None)         → 对端正常关闭（EOF/ConnectionReset）
//   Err("__timeout__") → 读超时，调用方应发 ping 继续等
//   Err(msg)         → 其他错误，断开
// B19: fragment_buf 用于累积分片帧（opcode 0x0 continuation），
//      首片(opcode=0x1, FIN=0)开始写入，末片(opcode=0x0, FIN=1)完成后返回完整消息
pub(crate) fn websocket_read_text_frame(
    stream: &mut TcpStream,
    fragment_buf: &mut Vec<u8>,
) -> Result<Option<String>, String> {
    let mut head = [0u8; 2];
    match stream.read_exact(&mut head) {
        Ok(()) => {}
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ) =>
        {
            // B6: 超时 → 返回特殊标记，调用方发 ping 继续等
            return Err("__timeout__".into());
        }
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::ConnectionReset
            ) =>
        {
            return Ok(None);
        }
        Err(error) => return Err(format!("读取 WebSocket 帧头失败: {error}")),
    }
    let fin = (head[0] & 0x80) != 0;
    let opcode = head[0] & 0x0f;
    let masked = (head[1] & 0x80) != 0;
    let mut len = (head[1] & 0x7f) as usize;
    if len == 126 {
        let mut ext = [0u8; 2];
        stream
            .read_exact(&mut ext)
            .map_err(|e| format!("读取 WebSocket 16-bit 长度失败: {e}"))?;
        len = u16::from_be_bytes(ext) as usize;
    } else if len == 127 {
        let mut ext = [0u8; 8];
        stream
            .read_exact(&mut ext)
            .map_err(|e| format!("读取 WebSocket 64-bit 长度失败: {e}"))?;
        len = u64::from_be_bytes(ext) as usize;
        if len > 8 * 1024 * 1024 {
            return Err("WebSocket 帧过大".into());
        }
    }
    let mut mask = [0u8; 4];
    if masked {
        stream
            .read_exact(&mut mask)
            .map_err(|e| format!("读取 WebSocket mask 失败: {e}"))?;
    }
    let mut payload = vec![0u8; len];
    if len > 0 {
        stream
            .read_exact(&mut payload)
            .map_err(|e| format!("读取 WebSocket payload 失败: {e}"))?;
    }
    if masked {
        for (idx, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask[idx % 4];
        }
    }
    // 安全：分片消息在 fragment_buf 中累积。单帧大小已受 8MB 上限约束，但需要额外
    // 限制整条消息的累积总量，防止对端用大量续帧持续累积导致内存耗尽。
    const WS_MAX_MESSAGE: usize = 64 * 1024 * 1024;
    if opcode == 0x0 && fragment_buf.len().saturating_add(payload.len()) > WS_MAX_MESSAGE {
        fragment_buf.clear();
        return Err("WebSocket 分片消息过大".into());
    }
    match opcode {
        0x1 if fin => {
            // 单帧完整文本消息
            fragment_buf.clear();
            Ok(Some(String::from_utf8_lossy(&payload).to_string()))
        }
        0x1 => {
            // B19: 首片，FIN=0，开始累积
            fragment_buf.clear();
            fragment_buf.extend_from_slice(&payload);
            Ok(Some(String::new())) // 还未完整，返回空表示继续
        }
        0x0 if fin => {
            // B19: 末片，拼完整消息
            fragment_buf.extend_from_slice(&payload);
            let complete = String::from_utf8_lossy(fragment_buf).to_string();
            fragment_buf.clear();
            Ok(Some(complete))
        }
        0x0 => {
            // B19: 中间片，继续累积
            fragment_buf.extend_from_slice(&payload);
            Ok(Some(String::new()))
        }
        0x8 => Ok(None),
        0x9 => {
            let _ = websocket_send_pong(stream, &payload);
            Ok(Some(String::new()))
        }
        0xA => Ok(Some(String::new())),
        _ => Ok(Some(String::new())),
    }
}

pub(crate) fn smart_control_preview(text: &str, limit: usize) -> String {
    let mut preview = text.chars().take(limit).collect::<String>();
    if text.chars().count() > limit {
        preview.push('…');
    }
    preview
}

pub(crate) fn smart_control_client_state_store() -> &'static Mutex<SmartControlClientState> {
    SMART_CONTROL_CLIENT_STATE.get_or_init(|| Mutex::new(SmartControlClientState::default()))
}

pub(crate) fn smart_control_chunk_store() -> &'static Mutex<HashMap<String, SmartControlChunkAssembly>> {
    SMART_CONTROL_SERVER_CHUNKS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn smart_control_turn_store() -> &'static Mutex<HashMap<u64, SmartControlTurnAccumulator>> {
    SMART_CONTROL_TURN_STREAMS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn smart_control_approval_store() -> &'static Mutex<Vec<SmartControlApprovalRequest>> {
    SMART_CONTROL_APPROVALS.get_or_init(|| Mutex::new(Vec::new()))
}

pub(crate) fn smart_control_client_snapshot() -> SmartControlClientState {
    smart_control_client_state_store()
        .lock()
        .map(|state| state.clone())
        .unwrap_or_default()
}

pub(crate) fn smart_control_reset_client_state() {
    if let Ok(mut state) = smart_control_client_state_store().lock() {
        *state = SmartControlClientState::default();
    }
    if let Ok(mut chunks) = smart_control_chunk_store().lock() {
        chunks.clear();
    }
    if let Ok(mut turns) = smart_control_turn_store().lock() {
        turns.clear();
    }
}

pub(crate) fn smart_control_next_envelope_parts() -> (String, String, u64, Option<String>) {
    let mut state = smart_control_client_state_store()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if state.client_id.trim().is_empty() {
        state.client_id = format!("varswitch-client-{}", uuid::Uuid::new_v4());
    }
    if state.stream_id.trim().is_empty() {
        state.stream_id = format!("varswitch-stream-{}", uuid::Uuid::new_v4());
    }
    let seq_id = state.next_seq_id;
    state.next_seq_id = state.next_seq_id.saturating_add(1);
    let cursor = if state.cursor.trim().is_empty() {
        None
    } else {
        Some(state.cursor.clone())
    };
    (
        state.client_id.clone(),
        state.stream_id.clone(),
        seq_id,
        cursor,
    )
}

pub(crate) fn smart_control_mark_initialized(request_id: u64, result: &serde_json::Value) {
    if let Ok(mut state) = smart_control_client_state_store().lock() {
        if state.last_initialize_id == request_id || state.last_initialize_id == 0 {
            state.initialized = true;
        }
        if let Some(cursor) = result
            .get("cursor")
            .or_else(|| result.get("subscribeCursor"))
            .and_then(|v| v.as_str())
        {
            state.cursor = cursor.to_string();
        }
    }
}

pub(crate) fn smart_control_set_stream_identity(client_id: &str, stream_id: &str) {
    if let Ok(mut state) = smart_control_client_state_store().lock() {
        if !client_id.trim().is_empty() {
            state.client_id = client_id.trim().to_string();
        }
        if !stream_id.trim().is_empty() {
            state.stream_id = stream_id.trim().to_string();
        }
    }
}

pub(crate) fn smart_control_update_pong_status(status: &str) {
    if let Ok(mut state) = smart_control_client_state_store().lock() {
        state.last_pong_status = status.to_string();
        if status.eq_ignore_ascii_case("unknown") || status.contains("unknown") {
            state.initialized = false;
        }
    }
}

pub(crate) fn smart_control_envelope_message(value: &serde_json::Value) -> Option<&serde_json::Value> {
    value.get("message").or_else(|| {
        if value.get("type").and_then(|v| v.as_str()) == Some("server_message")
            || value.get("type").and_then(|v| v.as_str()) == Some("client_message")
        {
            value.get("message")
        } else {
            None
        }
    })
}

pub(crate) fn smart_control_is_server_envelope(value: &serde_json::Value) -> bool {
    matches!(
        value.get("type").and_then(|v| v.as_str()),
        Some("server_message") | Some("server_message_chunk") | Some("ack") | Some("pong")
    ) && value.get("client_id").is_some()
}

pub(crate) fn smart_control_build_client_envelope(
    client_id: &str,
    stream_id: &str,
    seq_id: u64,
    message: serde_json::Value,
    cursor: Option<&str>,
) -> serde_json::Value {
    let mut envelope = serde_json::json!({
        "type": "client_message",
        "client_id": client_id,
        "stream_id": stream_id,
        "seq_id": seq_id,
        "message": message,
    });
    if let Some(cursor) = cursor.filter(|value| !value.trim().is_empty()) {
        envelope["cursor"] = serde_json::json!(cursor);
    }
    envelope
}

pub(crate) fn smart_control_build_ack_envelope(
    client_id: &str,
    stream_id: &str,
    seq_id: u64,
    segment_id: Option<u64>,
) -> serde_json::Value {
    let mut envelope = serde_json::json!({
        "type": "ack",
        "client_id": client_id,
        "stream_id": stream_id,
        "seq_id": seq_id,
    });
    if let Some(segment_id) = segment_id {
        envelope["segment_id"] = serde_json::json!(segment_id);
    }
    envelope
}

pub(crate) fn smart_control_build_ping_envelope() -> serde_json::Value {
    let state = smart_control_client_snapshot();
    let mut envelope = serde_json::json!({
        "type": "ping",
        "client_id": state.client_id,
        "stream_id": state.stream_id,
    });
    if !state.cursor.trim().is_empty() {
        envelope["cursor"] = serde_json::json!(state.cursor);
    }
    envelope
}

pub(crate) fn smart_control_reassemble_server_chunk(value: &serde_json::Value) -> Option<serde_json::Value> {
    let client_id = value
        .get("client_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let stream_id = value
        .get("stream_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let seq_id = value.get("seq_id").and_then(|v| v.as_u64()).unwrap_or(0);
    let segment_id = value
        .get("segment_id")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let segment_count = value
        .get("segment_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let message_size_bytes = value
        .get("message_size_bytes")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let chunk_b64 = value
        .get("message_chunk_base64")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if client_id.is_empty()
        || stream_id.is_empty()
        || seq_id == 0
        || segment_count == 0
        || segment_id >= segment_count
        || message_size_bytes == 0
        || message_size_bytes > 100 * 1024 * 1024
        || segment_count > 1024
        || chunk_b64.is_empty()
    {
        return None;
    }
    let chunk = base64::engine::general_purpose::STANDARD
        .decode(chunk_b64)
        .ok()?;
    let key = format!("{client_id}:{stream_id}:{seq_id}");
    let mut chunks = smart_control_chunk_store().lock().ok()?;
    let assembly = chunks
        .entry(key.clone())
        .or_insert_with(|| SmartControlChunkAssembly {
            segment_count,
            message_size_bytes,
            raw: Vec::new(),
            next_segment_id: 0,
        });
    if assembly.segment_count != segment_count
        || assembly.message_size_bytes != message_size_bytes
        || assembly.next_segment_id != segment_id
        || assembly.raw.len().saturating_add(chunk.len()) > message_size_bytes
    {
        chunks.remove(&key);
        return None;
    }
    assembly.raw.extend_from_slice(&chunk);
    assembly.next_segment_id = assembly.next_segment_id.saturating_add(1);
    if assembly.next_segment_id < assembly.segment_count {
        return None;
    }
    let assembly = chunks.remove(&key)?;
    if assembly.raw.len() != assembly.message_size_bytes {
        return None;
    }
    serde_json::from_slice::<serde_json::Value>(&assembly.raw).ok()
}

pub(crate) fn json_u64(value: &serde_json::Value, keys: &[&str]) -> Option<u64> {
    for key in keys {
        if let Some(number) = value.get(*key).and_then(|item| item.as_u64()) {
            return Some(number);
        }
        if let Some(text) = value.get(*key).and_then(|item| item.as_str()) {
            if let Ok(number) = text.trim().parse::<u64>() {
                return Some(number);
            }
        }
    }
    None
}

pub(crate) fn smart_control_extract_text(value: &serde_json::Value) -> String {
    if let Some(message) = smart_control_envelope_message(value) {
        let nested = smart_control_extract_text(message);
        if !nested.trim().is_empty() {
            return nested;
        }
    }
    for path in [
        &["result", "text"][..],
        &["result", "message"][..],
        &["result", "outputText"][..],
        &["result", "response", "text"][..],
        &["response", "text"][..],
        &["response", "message"][..],
        &["text"][..],
        &["message"][..],
        &["delta", "text"][..],
        &["delta", "content"][..],
        &["item", "text"][..],
        &["item", "content", "text"][..],
        &["params", "text"][..],
        &["params", "delta", "text"][..],
        &["params", "item", "text"][..],
    ] {
        let mut current = value;
        let mut found = true;
        for key in path {
            if let Some(next) = current.get(*key) {
                current = next;
            } else {
                found = false;
                break;
            }
        }
        if found {
            if let Some(text) = current.as_str() {
                if !text.trim().is_empty() {
                    return text.trim().to_string();
                }
            }
        }
    }
    String::new()
}

pub(crate) fn smart_control_extract_error(value: &serde_json::Value) -> String {
    if let Some(message) = smart_control_envelope_message(value) {
        let nested = smart_control_extract_error(message);
        if !nested.trim().is_empty() {
            return nested;
        }
    }
    for path in [
        &["error", "message"][..],
        &["error"][..],
        &["result", "error"][..],
        &["response", "error"][..],
    ] {
        let mut current = value;
        let mut found = true;
        for key in path {
            if let Some(next) = current.get(*key) {
                current = next;
            } else {
                found = false;
                break;
            }
        }
        if found {
            if let Some(text) = current.as_str() {
                if !text.trim().is_empty() {
                    return text.trim().to_string();
                }
            }
            if current.is_object() || current.is_array() {
                return current.to_string();
            }
        }
    }
    String::new()
}

pub(crate) fn smart_control_extract_delta_text_lossless(value: &serde_json::Value) -> String {
    for path in [
        &["delta", "text"][..],
        &["delta", "content"][..],
        &["params", "delta", "text"][..],
        &["item", "text"][..],
        &["params", "item", "text"][..],
    ] {
        let mut current = value;
        let mut found = true;
        for key in path {
            if let Some(next) = current.get(*key) {
                current = next;
            } else {
                found = false;
                break;
            }
        }
        if found {
            if let Some(text) = current.as_str() {
                if !text.is_empty() {
                    return text.to_string();
                }
            }
        }
    }
    String::new()
}

pub(crate) fn smart_control_message_id(value: &serde_json::Value) -> Option<u64> {
    json_u64(value, &["id", "requestId", "responseTo"]).or_else(|| {
        smart_control_envelope_message(value)
            .and_then(|message| json_u64(message, &["id", "requestId", "responseTo"]))
    })
}

pub(crate) fn smart_control_message_method(value: &serde_json::Value) -> String {
    let direct = json_string(value, &["method", "name", "op"]);
    if !direct.is_empty() {
        return direct;
    }
    smart_control_envelope_message(value)
        .map(|message| json_string(message, &["method", "name", "op"]))
        .unwrap_or_default()
}

pub(crate) fn smart_control_is_turn_done(value: &serde_json::Value) -> bool {
    let method = smart_control_message_method(value);
    let status = json_string(value, &["status", "state"]);
    let event_type = json_string(value, &["type", "event", "kind"]);
    method.contains("turn/complete")
        || method.contains("turn/done")
        || method.contains("turn/finished")
        || status.eq_ignore_ascii_case("completed")
        || status.eq_ignore_ascii_case("done")
        || status.eq_ignore_ascii_case("failed")
        || event_type.contains("completed")
        || event_type.contains("done")
}

pub(crate) fn smart_control_observe_turn_item(value: &serde_json::Value) {
    let Some(id) = smart_control_message_id(value) else {
        return;
    };
    let lossless_delta = smart_control_extract_delta_text_lossless(value);
    let text = if lossless_delta.is_empty() {
        smart_control_extract_text(value)
    } else {
        lossless_delta
    };
    let error = smart_control_extract_error(value);
    let done = smart_control_is_turn_done(value)
        || value
            .get("result")
            .is_some_and(|result| result.is_object() || result.is_string());
    if text.is_empty() && error.is_empty() && !done {
        return;
    }
    let mut turns = match smart_control_turn_store().lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    let acc = turns.entry(id).or_default();
    if !text.is_empty() {
        if done && value.get("result").is_some() && value.get("delta").is_none() {
            acc.final_text = text;
        } else {
            acc.text_parts.push(text);
        }
    }
    if !error.is_empty() {
        acc.error = error;
    }
    if done || !acc.error.is_empty() {
        acc.done = true;
        let final_text = if !acc.final_text.trim().is_empty() {
            acc.final_text.trim().to_string()
        } else {
            acc.text_parts.join("").trim().to_string()
        };
        smart_control_complete_pending(id, final_text, acc.error.clone());
        turns.remove(&id);
    }
}

pub(crate) fn smart_control_extract_approval(
    value: &serde_json::Value,
) -> Option<SmartControlApprovalRequest> {
    let message = smart_control_envelope_message(value).unwrap_or(value);
    let method = smart_control_message_method(message);
    let looks_like_approval = method.contains("approval")
        || method.contains("confirm")
        || message.get("approval").is_some()
        || message.get("decisions").is_some()
        || message.get("options").is_some_and(|v| v.is_array());
    if !looks_like_approval {
        return None;
    }
    let request_id = message
        .get("id")
        .map(|v| v.to_string().trim_matches('"').to_string())
        .or_else(|| {
            value
                .get("seq_id")
                .map(|v| v.to_string().trim_matches('"').to_string())
        })
        .unwrap_or_default();
    let params = message.get("params").unwrap_or(message);
    let title = json_string(params, &["title", "name", "command", "action"]);
    let body = [
        json_string(params, &["body", "message", "description", "reason"]),
        params
            .get("approval")
            .map(|v| v.to_string())
            .unwrap_or_default(),
    ]
    .into_iter()
    .find(|v| !v.trim().is_empty())
    .unwrap_or_else(|| smart_control_preview(&message.to_string(), 360));
    let mut options = Vec::new();
    for key in ["options", "decisions", "choices"] {
        if let Some(items) = params.get(key).and_then(|v| v.as_array()) {
            for item in items {
                if let Some(text) = item.as_str() {
                    options.push(text.to_string());
                } else {
                    let label = json_string(item, &["label", "title", "decision", "value"]);
                    if !label.is_empty() {
                        options.push(label);
                    }
                }
            }
        }
    }
    if options.is_empty() {
        options = vec!["approve".into(), "deny".into()];
    }
    Some(SmartControlApprovalRequest {
        request_id,
        method,
        title: if title.is_empty() {
            "需要审批".into()
        } else {
            title
        },
        body,
        options,
        received_at: chrono_now(),
        raw_preview: smart_control_preview(&message.to_string(), 800),
    })
}

pub(crate) fn smart_control_remember_approval(value: &serde_json::Value) {
    let Some(approval) = smart_control_extract_approval(value) else {
        return;
    };
    if let Ok(mut approvals) = smart_control_approval_store().lock() {
        approvals.push(approval);
        if approvals.len() > 20 {
            let drain_count = approvals.len().saturating_sub(20);
            approvals.drain(0..drain_count);
        }
    }
}

pub(crate) fn smart_control_maybe_complete_pending(value: &serde_json::Value) {
    let Some(id) = smart_control_message_id(value) else {
        return;
    };
    let method = smart_control_message_method(value);
    if method == "initialize" {
        if let Some(result) = value.get("result") {
            smart_control_mark_initialized(id, result);
        }
    }
    smart_control_observe_turn_item(value);
    if !smart_control_extract_delta_text_lossless(value).is_empty()
        && value.get("result").is_none()
        && value.get("response").is_none()
    {
        return;
    }
    let text = smart_control_extract_text(value);
    let error = smart_control_extract_error(value);
    if !text.is_empty() || !error.is_empty() {
        smart_control_complete_pending(id, text, error);
    }
}

pub(crate) fn remember_smart_control_event(text: &str) {
    let parsed = serde_json::from_str::<serde_json::Value>(text).ok();
    let event_type = parsed
        .as_ref()
        .map(|value| json_string(value, &["type", "event", "kind", "messageType"]))
        .unwrap_or_default();
    let message_id = parsed
        .as_ref()
        .map(|value| json_string(value, &["id", "messageId", "requestId"]))
        .unwrap_or_default();
    let method = parsed
        .as_ref()
        .map(|value| json_string(value, &["method", "name", "op"]))
        .unwrap_or_default();
    let event = SmartControlEvent {
        received_at: chrono_now(),
        event_type,
        message_id,
        method,
        raw_preview: smart_control_preview(text, 800),
    };
    push_smart_control_event(event);
    if let Some(value) = parsed.as_ref() {
        smart_control_handle_inbound_value(value);
    }
}

pub(crate) fn smart_control_handle_inbound_value(value: &serde_json::Value) {
    if smart_control_is_server_envelope(value) {
        let client_id = value
            .get("client_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let stream_id = value
            .get("stream_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let seq_id = value.get("seq_id").and_then(|v| v.as_u64()).unwrap_or(0);
        smart_control_set_stream_identity(client_id, stream_id);
        match value.get("type").and_then(|v| v.as_str()) {
            Some("pong") => {
                smart_control_update_pong_status(&json_string(value, &["status"]));
                let _ = smart_control_send_json(&smart_control_build_ack_envelope(
                    client_id, stream_id, seq_id, None,
                ));
            }
            Some("ack") => {}
            Some("server_message") => {
                let _ = smart_control_send_json(&smart_control_build_ack_envelope(
                    client_id, stream_id, seq_id, None,
                ));
                if let Some(message) = value.get("message") {
                    smart_control_remember_approval(message);
                    smart_control_maybe_complete_pending(message);
                }
            }
            Some("server_message_chunk") => {
                let segment_id = value.get("segment_id").and_then(|v| v.as_u64());
                let _ = smart_control_send_json(&smart_control_build_ack_envelope(
                    client_id, stream_id, seq_id, segment_id,
                ));
                if let Some(message) = smart_control_reassemble_server_chunk(value) {
                    smart_control_remember_approval(&message);
                    smart_control_maybe_complete_pending(&message);
                }
            }
            _ => {}
        }
        return;
    }
    smart_control_remember_approval(value);
    smart_control_maybe_complete_pending(value);
}

pub(crate) fn smart_control_last_event() -> Option<SmartControlEvent> {
    SMART_CONTROL_LAST_EVENT
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
}

pub(crate) fn smart_control_event_log() -> &'static Mutex<Vec<SmartControlEvent>> {
    SMART_CONTROL_EVENT_LOG.get_or_init(|| Mutex::new(Vec::new()))
}

pub(crate) fn push_smart_control_event(event: SmartControlEvent) {
    if let Ok(mut last) = SMART_CONTROL_LAST_EVENT.lock() {
        *last = Some(event.clone());
    }
    if let Ok(mut events) = smart_control_event_log().lock() {
        events.push(event);
        if events.len() > 80 {
            let drain_count = events.len().saturating_sub(80);
            events.drain(0..drain_count);
        }
    }
}

pub(crate) fn smart_control_debug_snapshot() -> SmartControlDebugSnapshot {
    let pending_count = smart_control_pending_store()
        .0
        .lock()
        .map(|pending| pending.len())
        .unwrap_or(0);
    let events = smart_control_event_log()
        .lock()
        .map(|events| events.clone())
        .unwrap_or_default();
    let approvals = smart_control_approval_store()
        .lock()
        .map(|items| items.clone())
        .unwrap_or_default();
    SmartControlDebugSnapshot {
        connected: SMART_CONTROL_REMOTE_CONNECTED.load(Ordering::SeqCst),
        pending_count,
        last_event: smart_control_last_event(),
        events,
        client: smart_control_client_snapshot(),
        approvals,
    }
}

pub(crate) fn next_smart_control_request_id() -> u64 {
    let lock = SMART_CONTROL_NEXT_REQUEST_ID.get_or_init(|| Mutex::new(1));
    let mut guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let id = *guard;
    *guard = guard.saturating_add(1);
    id
}

pub(crate) fn smart_control_pending_store(
) -> &'static (Mutex<HashMap<u64, SmartControlPendingResult>>, Condvar) {
    SMART_CONTROL_PENDING.get_or_init(|| (Mutex::new(HashMap::new()), Condvar::new()))
}

pub(crate) fn smart_control_register_pending(id: u64) {
    let (lock, _) = smart_control_pending_store();
    if let Ok(mut guard) = lock.lock() {
        guard.insert(id, SmartControlPendingResult::default());
    }
}

pub(crate) fn smart_control_complete_pending(id: u64, text: String, error: String) {
    let (lock, condvar) = smart_control_pending_store();
    if let Ok(mut guard) = lock.lock() {
        guard.insert(
            id,
            SmartControlPendingResult {
                done: true,
                text,
                error,
            },
        );
        condvar.notify_all();
    }
}

pub(crate) fn smart_control_wait_pending(id: u64, timeout: Duration) -> Option<SmartControlPendingResult> {
    let (lock, condvar) = smart_control_pending_store();
    let guard = lock.lock().ok()?;
    let (mut guard, _) = condvar
        .wait_timeout_while(guard, timeout, |pending| {
            pending.get(&id).map(|result| !result.done).unwrap_or(true)
        })
        .ok()?;
    guard.remove(&id).filter(|result| result.done)
}

pub(crate) fn smart_control_initialize_message(id: u64) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "method": "initialize",
        "params": {
            "clientInfo": {
                "name": "VarSwitch",
                "version": env!("CARGO_PKG_VERSION")
            },
            "capabilities": {
                "threads": true,
                "turns": true,
                "streaming": true,
                "approvals": true
            }
        }
    })
}

pub(crate) fn smart_control_turn_start_message(id: u64, thread_id: &str, text: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "method": "turn/start",
        "params": {
            "threadId": thread_id.trim(),
            "input": [
                {
                    "type": "text",
                    "text": text.trim()
                }
            ]
        }
    })
}

pub(crate) fn smart_control_wrap_client_message(message: serde_json::Value) -> serde_json::Value {
    let (client_id, stream_id, seq_id, cursor) = smart_control_next_envelope_parts();
    smart_control_build_client_envelope(&client_id, &stream_id, seq_id, message, cursor.as_deref())
}

pub(crate) fn smart_control_send_json(value: &serde_json::Value) -> Result<(), String> {
    let text = serde_json::to_string(value).map_err(|e| format!("序列化协议消息失败: {e}"))?;
    let mut guard = SMART_CONTROL_WS_WRITER
        .lock()
        .map_err(|_| "高级控制通道写入锁已损坏".to_string())?;
    let Some(stream) = guard.as_mut() else {
        return Err("高级控制通道未连接".into());
    };
    websocket_send_text(stream, &text)?;
    push_smart_control_event(SmartControlEvent {
        received_at: chrono_now(),
        event_type: "outbound".into(),
        message_id: value
            .get("id")
            .or_else(|| value.get("seq_id"))
            .map(|id| id.to_string())
            .unwrap_or_default(),
        method: smart_control_message_method(value),
        raw_preview: smart_control_preview(&text, 800),
    });
    Ok(())
}

pub(crate) fn smart_control_ensure_initialized() -> Result<(), String> {
    if smart_control_client_snapshot().initialized {
        return Ok(());
    }
    let request_id = next_smart_control_request_id();
    {
        if let Ok(mut state) = smart_control_client_state_store().lock() {
            state.last_initialize_id = request_id;
        }
    }
    smart_control_register_pending(request_id);
    let message = smart_control_initialize_message(request_id);
    let envelope = smart_control_wrap_client_message(message);
    smart_control_send_json(&envelope)?;
    match smart_control_wait_pending(request_id, Duration::from_secs(30)) {
        Some(result) if !result.error.trim().is_empty() => Err(result.error),
        Some(_) => {
            if let Ok(mut state) = smart_control_client_state_store().lock() {
                state.initialized = true;
            }
            Ok(())
        }
        None => Err("高级控制 initialize 超时".into()),
    }
}

pub(crate) fn try_smart_control_dispatch(thread_id: &str, text: &str) -> Result<Option<String>, String> {
    if !SMART_CONTROL_REMOTE_CONNECTED.load(Ordering::SeqCst) {
        return Ok(None);
    }
    smart_control_ensure_initialized()?;
    let request_id = next_smart_control_request_id();
    smart_control_register_pending(request_id);
    let message = smart_control_turn_start_message(request_id, thread_id, text);
    let envelope = smart_control_wrap_client_message(message);
    smart_control_send_json(&envelope)?;
    log_info!(
        "[smart-control][dispatch] sent turn/start over protocol channel preview={}",
        smart_control_preview(text, 240)
    );
    match smart_control_wait_pending(request_id, Duration::from_secs(180)) {
        Some(result) if !result.error.trim().is_empty() => Err(result.error),
        Some(result) if !result.text.trim().is_empty() => Ok(Some(result.text)),
        Some(_) => Ok(Some("高级控制通道已完成，但没有返回可显示文本。".into())),
        None => Err("高级控制通道等待 Codex 回复超时".into()),
    }
}

pub(crate) fn smart_control_json_response(value: serde_json::Value) -> String {
    let body = serde_json::to_string(&value).unwrap_or_else(|_| "{}".into());
    smart_control_http_response("200 OK", "application/json", &body)
}

pub(crate) fn smart_control_not_found(path: &str) -> String {
    smart_control_json_response(serde_json::json!({
        "ok": false,
        "error": "not_found",
        "path": path,
    }))
    .replacen("HTTP/1.1 200 OK", "HTTP/1.1 404 Not Found", 1)
}

/// 校验 HTTP Host 头是否指向本机回环地址。
/// 用于 DNS 重绑定防护：服务虽只绑定 127.0.0.1，但恶意网页可把某个域名
/// 重绑定到 127.0.0.1 后访问本地服务，因此需要额外校验 Host 头。
pub(crate) fn smart_control_host_is_local(host_header: &str) -> bool {
    let host = host_header.trim();
    if host.is_empty() {
        return false;
    }
    // 去掉端口；IPv6 形如 [::1]:3847
    let hostname = if let Some(rest) = host.strip_prefix('[') {
        rest.split(']').next().unwrap_or("")
    } else {
        host.split(':').next().unwrap_or("")
    };
    matches!(hostname, "127.0.0.1" | "localhost" | "::1")
}

pub(crate) fn read_http_request_head(stream: &mut TcpStream) -> Result<String, String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|e| format!("设置读取超时失败: {e}"))?;
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    while buf.len() < 64 * 1024 {
        let n = stream
            .read(&mut byte)
            .map_err(|e| format!("读取 HTTP 请求失败: {e}"))?;
        if n == 0 {
            break;
        }
        buf.push(byte[0]);
        if buf.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    Ok(String::from_utf8_lossy(&buf).to_string())
}

pub(crate) fn handle_smart_control_http_connection(mut stream: TcpStream) -> Result<(), String> {
    let head = read_http_request_head(&mut stream)?;
    let request_line = head.lines().next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let raw_path = parts.next().unwrap_or("/");
    let path = raw_path.split('?').next().unwrap_or(raw_path);
    // 安全：DNS 重绑定防护。仅接受指向本机回环的 Host，阻止恶意网页
    // 通过把域名解析到 127.0.0.1 来访问本地无鉴权控制服务。
    if !smart_control_host_is_local(http_header_value(&head, "host").unwrap_or("")) {
        let response = smart_control_http_response(
            "403 Forbidden",
            "application/json",
            "{\"ok\":false,\"error\":\"forbidden_host\"}",
        );
        let _ = stream.write_all(response.as_bytes());
        return Ok(());
    }
    if method == "GET"
        && path == "/backend-api/wham/remote/control/server"
        && is_websocket_upgrade(&head)
    {
        let key = http_header_value(&head, "sec-websocket-key")
            .ok_or("WebSocket 缺少 Sec-WebSocket-Key")?;
        let response = websocket_upgrade_response(key);
        stream
            .write_all(response.as_bytes())
            .map_err(|e| format!("写入 WebSocket 握手响应失败: {e}"))?;
        SMART_CONTROL_REMOTE_CONNECTED.store(true, Ordering::SeqCst);
        smart_control_reset_client_state();
        // B6: 每次新连接分配唯一代际 id，退出时校验后再清全局状态，避免新连接被旧连接收尾代码打翻
        let my_gen = SMART_CONTROL_WS_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
        if let Ok(writer) = stream.try_clone() {
            if let Ok(mut guard) = SMART_CONTROL_WS_WRITER.lock() {
                *guard = Some(writer);
            }
        }
        set_smart_control_status_cache(SmartControlStatus {
            available: true,
            connected: true,
            backend_url: default_smart_control_backend_url().into(),
            status: "高级控制通道已连接".into(),
            detail: "Codex 已建立 remote-control WebSocket，等待协议事件。".into(),
            checked_at: chrono_now(),
        });
        // B6: 每 30 秒超时后发 ping，连续 3 次无应答（90 秒空闲）才判定断线
        stream.set_read_timeout(Some(Duration::from_secs(30))).ok();
        let _ = smart_control_send_json(&smart_control_build_ping_envelope());
        let mut consecutive_timeouts: u32 = 0;
        // B19: 分片帧累积缓冲区
        let mut fragment_buf: Vec<u8> = Vec::new();
        loop {
            match websocket_read_text_frame(&mut stream, &mut fragment_buf) {
                Ok(Some(text)) if text.trim().is_empty() => {
                    // pong 或空帧 → 重置超时计数
                    consecutive_timeouts = 0;
                    continue;
                }
                Ok(Some(text)) => {
                    consecutive_timeouts = 0;
                    log_info!(
                        "[smart-control][ws] received frame: {}",
                        smart_control_preview(&text, 240)
                    );
                    remember_smart_control_event(&text);
                }
                Ok(None) => break, // 正常关闭（EOF / ConnectionReset）
                Err(e) if e == "__timeout__" => {
                    // B6: 超时不等于断线，发 ping 继续等；连续 3 次（90 秒）无响应才断
                    consecutive_timeouts += 1;
                    if consecutive_timeouts >= 3 {
                        log_info!("[smart-control][ws] 持续 90s 无活动，主动关闭连接");
                        break;
                    }
                    let _ = smart_control_send_json(&smart_control_build_ping_envelope());
                }
                Err(error) => {
                    log_warn!("[smart-control][ws] frame read failed: {}", error);
                    break;
                }
            }
        }
        // B6: 仅在代际匹配时才清全局状态，避免新连接状态被旧连接收尾代码覆盖
        let current_gen = SMART_CONTROL_WS_GENERATION.load(Ordering::SeqCst);
        if current_gen == my_gen {
            SMART_CONTROL_REMOTE_CONNECTED.store(false, Ordering::SeqCst);
            if let Ok(mut guard) = SMART_CONTROL_WS_WRITER.lock() {
                *guard = None;
            }
        }
        return Ok(());
    }
    let response = match (method, path) {
        ("OPTIONS", _) => smart_control_http_response("204 No Content", "text/plain", ""),
        ("GET", "/api/status") => smart_control_json_response(serde_json::json!({
            "ok": true,
            "service": "varswitch-control",
            "running": true,
            "time": chrono_now(),
        })),
        ("GET", "/api/remote-control/status") => smart_control_json_response(serde_json::json!({
            "ok": true,
            "available": true,
            "connected": SMART_CONTROL_REMOTE_CONNECTED.load(Ordering::SeqCst),
            "ready": SMART_CONTROL_REMOTE_CONNECTED.load(Ordering::SeqCst) && smart_control_client_snapshot().initialized,
            "service": "varswitch-control",
            "lastEvent": smart_control_last_event(),
            "client": smart_control_client_snapshot(),
            "approvals": smart_control_approval_store().lock().map(|items| items.clone()).unwrap_or_default(),
            "message": if SMART_CONTROL_REMOTE_CONNECTED.load(Ordering::SeqCst) {
                "Codex 已连接 VarSwitch 本机控制服务。"
            } else {
                "VarSwitch 本机控制服务已启动，等待 Codex remote-control 连接。"
            },
            "time": chrono_now(),
        })),
        ("POST", "/backend-api/wham/remote/control/server/enroll") => {
            smart_control_json_response(serde_json::json!({
                "ok": true,
                "server": {
                    "id": "varswitch-local-control",
                    "name": "VarSwitch Local Control"
                },
                "message": "VarSwitch 已收到 Codex remote-control enroll 请求，完整双向协议将在控制通道建立后接管。"
            }))
        }
        ("GET", "/backend-api/wham/remote/control/server") => {
            smart_control_json_response(serde_json::json!({
                "ok": true,
                "connected": false,
                "message": "VarSwitch 本机控制服务已就绪，当前轻量接口用于状态探测。"
            }))
        }
        _ => smart_control_not_found(path),
    };
    stream
        .write_all(response.as_bytes())
        .map_err(|e| format!("写入 HTTP 响应失败: {e}"))?;
    Ok(())
}

pub(crate) fn start_smart_control_server(app: tauri::AppHandle, backend_url: String) {
    if SMART_CONTROL_SERVER_ACTIVE.swap(true, Ordering::SeqCst) {
        return;
    }
    // 服务状态即将改变，作废探测缓存，让下一次快照拿到真实结果
    invalidate_smart_control_probe_cache();
    SMART_CONTROL_SERVER_CANCEL_REQUESTED.store(false, Ordering::SeqCst);
    std::thread::spawn(move || {
        let bind_addr = smart_control_bind_addr_from_url(&backend_url);
        log_info!("[smart-control] starting local service at {}", bind_addr);
        let listener = match TcpListener::bind(&bind_addr) {
            Ok(listener) => listener,
            Err(error) => {
                log_warn!("[smart-control] bind failed at {}: {}", bind_addr, error);
                SMART_CONTROL_SERVER_ACTIVE.store(false, Ordering::SeqCst);
                let mut state = read_toolbox_state(&app);
                state.mobile_remote.remote_control_connected = false;
                state.mobile_remote.remote_control_status = "高级控制服务启动失败".into();
                state.mobile_remote.remote_control_detail =
                    format!("监听 {bind_addr} 失败：{error}");
                let _ = write_toolbox_state(&app, &state);
                return;
            }
        };
        if let Err(error) = listener.set_nonblocking(true) {
            log_warn!("[smart-control] set_nonblocking failed: {}", error);
        }
        {
            let mut state = read_toolbox_state(&app);
            state.mobile_remote.remote_control_backend_url =
                normalize_smart_control_backend_url(&backend_url);
            state.mobile_remote.remote_control_status = "高级控制服务已启动".into();
            state.mobile_remote.remote_control_detail =
                format!("正在监听 {bind_addr}，等待 Codex 连接。");
            state.mobile_remote.remote_control_connected = false;
            let _ = write_toolbox_state(&app, &state);
        }
        while !SMART_CONTROL_SERVER_CANCEL_REQUESTED.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((stream, _addr)) => {
                    // nonblocking listener 接受的 socket 在 Windows 上也会保持非阻塞；
                    // 请求刚到达时直接 read 会返回 WSAEWOULDBLOCK (10035)。
                    if let Err(error) = stream.set_nonblocking(false) {
                        log_warn!(
                            "[smart-control] set accepted socket blocking failed: {}",
                            error
                        );
                        continue;
                    }
                    std::thread::spawn(move || {
                        if let Err(error) = handle_smart_control_http_connection(stream) {
                            log_warn!("[smart-control] request failed: {}", error);
                        }
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(80));
                }
                Err(error) => {
                    log_warn!("[smart-control] accept failed: {}", error);
                    std::thread::sleep(Duration::from_millis(300));
                }
            }
        }
        log_info!("[smart-control] local service stopped");
        SMART_CONTROL_SERVER_ACTIVE.store(false, Ordering::SeqCst);
    });
}

pub(crate) fn stop_smart_control_server() {
    SMART_CONTROL_SERVER_CANCEL_REQUESTED.store(true, Ordering::SeqCst);
    invalidate_smart_control_probe_cache();
}

pub(crate) fn smart_control_backend_api_url() -> String {
    format!("{}/backend-api", default_smart_control_backend_url())
}


pub(crate) fn read_toolbox_state(app: &tauri::AppHandle) -> ToolboxState {
    let path = toolbox_state_path(app);
    if !path.exists() {
        let mut state = ToolboxState::default();
        state.plugin_marketplace_input = default_plugin_marketplace_url().to_string();
        state.mobile_remote.discovery_token = uuid::Uuid::new_v4().to_string();
        state.mobile_remote.device_name = local_device_name();
        state.mobile_remote.local_ip = detect_local_ip();
        return normalize_toolbox_state(state);
    }
    let mut state: ToolboxState = fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    if state.plugin_marketplace_input.trim().is_empty() {
        state.plugin_marketplace_input = default_plugin_marketplace_url().to_string();
    }
    if state.mobile_remote.discovery_token.trim().is_empty() {
        state.mobile_remote.discovery_token = uuid::Uuid::new_v4().to_string();
    }
    if state.mobile_remote.device_name.trim().is_empty() {
        state.mobile_remote.device_name = local_device_name();
    }
    state.mobile_remote.local_ip = detect_local_ip();
    normalize_toolbox_state(state)
}

pub(crate) fn trashed_codex_thread_ids(state: &ToolboxState) -> HashSet<String> {
    state
        .trashed_codex_threads
        .iter()
        .map(|thread| thread.id.clone())
        .filter(|id| !id.trim().is_empty())
        .collect()
}

pub(crate) fn visible_codex_threads(state: &ToolboxState) -> Vec<CodexThreadRecord> {
    let trashed = trashed_codex_thread_ids(state);
    state
        .synced_codex_threads
        .iter()
        .filter(|thread| !trashed.contains(&thread.id))
        .cloned()
        .collect()
}

pub(crate) fn refresh_selected_mobile_thread_after_session_change(state: &mut ToolboxState) {
    let visible = visible_codex_threads(state);
    let selected_visible = visible
        .iter()
        .any(|thread| thread.id == state.selected_mobile_thread_id);
    if !selected_visible {
        state.selected_mobile_thread_id = visible
            .first()
            .map(|thread| thread.id.clone())
            .unwrap_or_default();
    }
    if let Some(thread) = visible
        .iter()
        .find(|thread| thread.id == state.selected_mobile_thread_id)
    {
        state.mobile_remote.active_thread_id = thread.id.clone();
        state.mobile_remote.active_thread_name = thread.thread_name.clone();
    } else {
        state.mobile_remote.active_thread_id.clear();
        state.mobile_remote.active_thread_name.clear();
    }
}

pub(crate) fn codex_thread_to_trash_record(
    thread: CodexThreadRecord,
    deleted_at: &str,
) -> TrashedCodexThreadRecord {
    TrashedCodexThreadRecord {
        id: thread.id,
        thread_name: thread.thread_name,
        updated_at: thread.updated_at,
        session_file: thread.session_file,
        cwd: thread.cwd,
        last_user_message: thread.last_user_message,
        last_assistant_message: thread.last_assistant_message,
        deleted_at: deleted_at.to_string(),
    }
}

pub(crate) fn trash_record_to_codex_thread(thread: &TrashedCodexThreadRecord) -> CodexThreadRecord {
    CodexThreadRecord {
        id: thread.id.clone(),
        thread_name: thread.thread_name.clone(),
        updated_at: thread.updated_at.clone(),
        session_file: thread.session_file.clone(),
        cwd: thread.cwd.clone(),
        last_user_message: thread.last_user_message.clone(),
        last_assistant_message: thread.last_assistant_message.clone(),
    }
}

pub(crate) fn normalize_toolbox_state(mut state: ToolboxState) -> ToolboxState {
    state.plugin_marketplace_input = default_plugin_marketplace_url().to_string();
    if state.mobile_remote.discovery_token.trim().is_empty() {
        state.mobile_remote.discovery_token = uuid::Uuid::new_v4().to_string();
    }
    if state.mobile_remote.device_name.trim().is_empty() {
        state.mobile_remote.device_name = local_device_name();
    }
    if state.mobile_remote.mode.trim().is_empty() {
        state.mobile_remote.mode = "platform_bot".into();
    }
    if state
        .mobile_remote
        .remote_control_backend_url
        .trim()
        .is_empty()
    {
        state.mobile_remote.remote_control_backend_url = "http://127.0.0.1:3847".into();
    }
    if state.mobile_remote.remote_control_status.trim().is_empty() {
        state.mobile_remote.remote_control_status = "未检测".into();
    }
    state.mobile_remote.remote_control_preferred = true;
    if state.mobile_remote.listen_addr.trim().is_empty() {
        state.mobile_remote.listen_addr = "platform".into();
    }
    state.mobile_remote.local_ip = detect_local_ip();

    for channel in ["lark", "wechat", "qq"] {
        if !state
            .mobile_channels
            .iter()
            .any(|binding| binding.channel == channel)
        {
            state.mobile_channels.push(default_mobile_channel(channel));
        }
    }
    for binding in state.mobile_channels.iter_mut() {
        binding.channel = normalize_mobile_channel(&binding.channel);
        if binding.status.trim().is_empty() {
            binding.status = if binding.thread_id.trim().is_empty() {
                "未绑定".into()
            } else {
                "已绑定，等待开启平台连接".into()
            };
        }
        if binding.base_url.trim().is_empty() {
            binding.base_url = default_mobile_channel_base_url(&binding.channel).to_string();
        }
        normalize_mobile_channel_qr_cache(binding);
    }

    state
        .trashed_codex_threads
        .retain(|thread| !thread.id.trim().is_empty());
    let mut seen_trashed = HashSet::new();
    state
        .trashed_codex_threads
        .retain(|thread| seen_trashed.insert(thread.id.clone()));

    refresh_selected_mobile_thread_after_session_change(&mut state);

    state
}

pub(crate) fn normalize_mobile_channel(channel: &str) -> String {
    match channel.trim().to_ascii_lowercase().as_str() {
        "feishu" | "lark" => "lark".into(),
        "wechat" | "weixin" | "wx" => "wechat".into(),
        "qq" => "qq".into(),
        other => other.to_string(),
    }
}

pub(crate) const MOBILE_QR_CACHE_TTL_MS: u128 = 10 * 60 * 1000;

pub(crate) fn mobile_qr_cache_is_fresh(started_at: &str) -> bool {
    let Ok(started_at) = started_at.trim().parse::<u128>() else {
        return false;
    };
    chrono_timestamp_millis().saturating_sub(started_at) <= MOBILE_QR_CACHE_TTL_MS
}

pub(crate) fn is_qq_authorization_target(value: &str) -> bool {
    let lower = value.trim().to_ascii_lowercase();
    lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("mqqapi://")
        || lower.starts_with("qqbot://")
}

pub(crate) fn clear_stale_mobile_qr(binding: &mut MobileChannelBinding, message: &str) {
    binding.qr_url.clear();
    binding.qr_data_url.clear();
    binding.qr_device_code.clear();
    binding.qr_status = message.into();
}

pub(crate) fn normalize_mobile_channel_qr_cache(binding: &mut MobileChannelBinding) {
    let has_qr = !binding.qr_url.trim().is_empty() || !binding.qr_data_url.trim().is_empty();
    if !has_qr {
        return;
    }
    match binding.channel.as_str() {
        "qq" => {
            let has_image_qr = binding.qr_data_url.trim().starts_with("data:image/");
            let has_openable_target = is_qq_authorization_target(&binding.qr_url);
            if !(has_image_qr || has_openable_target)
                || !mobile_qr_cache_is_fresh(&binding.qr_started_at)
            {
                clear_stale_mobile_qr(binding, "QQ 二维码已失效，请重新绑定");
            }
        }
        "wechat" => {
            let qr_data = binding.qr_data_url.trim().to_ascii_lowercase();
            if binding.qr_device_code.trim().is_empty()
                || !mobile_qr_cache_is_fresh(&binding.qr_started_at)
                || qr_data.starts_with("data:image/svg+xml")
            {
                clear_stale_mobile_qr(binding, "微信二维码已失效，请重新生成");
            }
        }
        _ => {}
    }
}

pub(crate) fn default_mobile_channel_base_url(channel: &str) -> &'static str {
    match normalize_mobile_channel(channel).as_str() {
        "lark" => "https://open.feishu.cn",
        "wechat" => "https://ilinkai.weixin.qq.com",
        _ => "",
    }
}

pub(crate) fn default_mobile_channel(channel: &str) -> MobileChannelBinding {
    let normalized = normalize_mobile_channel(channel);
    MobileChannelBinding {
        channel: normalized.clone(),
        status: "未绑定".into(),
        base_url: default_mobile_channel_base_url(&normalized).to_string(),
        ..MobileChannelBinding::default()
    }
}

pub(crate) fn write_toolbox_state(app: &tauri::AppHandle, state: &ToolboxState) -> Result<(), String> {
    // B8: 全局写锁，保证多线程并发时不互相丢更新
    let _write_guard = TOOLBOX_STATE_WRITE_LOCK
        .lock()
        .map_err(|e| format!("获取状态写锁失败: {e}"))?;
    let path = toolbox_state_path(app);
    let json = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    // B8: 临时文件写入后 rename 原子替换，避免进程崩溃时写一半清空文件；
    // B5: Unix 设 0600，Windows 依赖 AppData 默认 ACL。统一复用 write_private_file。
    write_private_file(&path, &json).map_err(|e| format!("原子替换状态文件失败: {e}"))
}

pub(crate) const OPENAI_BUNDLED_MARKETPLACE_NAME: &str = "openai-bundled";

pub(crate) fn codex_plugin_config_id(name: &str, marketplace: &str) -> String {
    format!("{}@{}", name.trim(), marketplace.trim())
}

pub(crate) fn important_codex_builtin_plugin(
    name: &str,
    description: &str,
    skills: &[CodexBuiltinPluginSkill],
) -> bool {
    let haystack = format!(
        "{} {} {}",
        name,
        description,
        skills
            .iter()
            .map(|skill| format!("{} {}", skill.name, skill.description))
            .collect::<Vec<_>>()
            .join(" ")
    )
    .to_ascii_lowercase();
    ["computer", "chrome", "browser", "devtools", "fast", "speed"]
        .iter()
        .any(|needle| haystack.contains(needle))
}

pub(crate) fn yaml_front_matter_value(contents: &str, key: &str) -> Option<String> {
    let mut lines = contents.lines();
    if lines.next()? != "---" {
        return None;
    }
    for line in lines {
        if line == "---" {
            break;
        }
        let Some((left, right)) = line.split_once(':') else {
            continue;
        };
        if left.trim() == key {
            return Some(
                right
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string(),
            )
            .filter(|value| !value.is_empty());
        }
    }
    None
}

pub(crate) fn load_codex_builtin_plugin_skills(
    root: &Path,
    manifest: &serde_json::Value,
) -> Vec<CodexBuiltinPluginSkill> {
    let skills_path = manifest
        .get("skills")
        .and_then(|value| value.as_str())
        .map(|value| root.join(value.trim_start_matches("./")))
        .unwrap_or_else(|| root.join("skills"));
    let Ok(skill_dirs) = fs::read_dir(skills_path) else {
        return Vec::new();
    };
    let mut skills = skill_dirs
        .flatten()
        .filter_map(|entry| {
            let path = entry.path().join("SKILL.md");
            let contents = fs::read_to_string(path).ok()?;
            let name = yaml_front_matter_value(&contents, "name")
                .unwrap_or_else(|| entry.file_name().to_string_lossy().to_string());
            let description =
                yaml_front_matter_value(&contents, "description").unwrap_or_else(|| name.clone());
            Some(CodexBuiltinPluginSkill { name, description })
        })
        .collect::<Vec<_>>();
    skills.sort_by(|left, right| left.name.cmp(&right.name));
    skills
}

pub(crate) fn codex_builtin_plugin_from_root(
    marketplace: &str,
    root: PathBuf,
    enabled_ids: &HashSet<String>,
) -> Option<CodexBuiltinPluginItem> {
    let manifest_path = root.join(".codex-plugin").join("plugin.json");
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(manifest_path).ok()?).ok()?;
    let name = manifest.get("name")?.as_str()?.trim().to_string();
    if name.is_empty() || manifest.get("apps").is_some() {
        return None;
    }
    let skills = load_codex_builtin_plugin_skills(&root, &manifest);
    if skills.is_empty() {
        return None;
    }
    let interface = manifest
        .get("interface")
        .cloned()
        .unwrap_or_else(|| serde_json::Value::Object(Default::default()));
    let display_name = interface
        .get("displayName")
        .and_then(|value| value.as_str())
        .or_else(|| manifest.get("displayName").and_then(|value| value.as_str()))
        .unwrap_or(&name)
        .to_string();
    let description = manifest
        .get("description")
        .and_then(|value| value.as_str())
        .or_else(|| {
            interface
                .get("shortDescription")
                .and_then(|value| value.as_str())
        })
        .unwrap_or_default()
        .to_string();
    let version = manifest
        .get("version")
        .and_then(|value| value.as_str())
        .unwrap_or("local")
        .to_string();
    let id = codex_plugin_config_id(&name, marketplace);
    let important = important_codex_builtin_plugin(&name, &description, &skills);
    Some(CodexBuiltinPluginItem {
        id: id.clone(),
        name,
        display_name,
        marketplace: marketplace.to_string(),
        version,
        description,
        root: root.to_string_lossy().to_string(),
        enabled: enabled_ids.contains(&id),
        important,
        skills,
    })
}

pub(crate) fn newest_codex_builtin_plugin_entry(
    marketplace: &str,
    plugin_path: &Path,
    enabled_ids: &HashSet<String>,
) -> Option<CodexBuiltinPluginItem> {
    if plugin_path
        .join(".codex-plugin")
        .join("plugin.json")
        .is_file()
    {
        return codex_builtin_plugin_from_root(marketplace, plugin_path.to_path_buf(), enabled_ids);
    }
    let mut versions = fs::read_dir(plugin_path)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && path.join(".codex-plugin").join("plugin.json").is_file())
        .collect::<Vec<_>>();
    versions.sort();
    versions.reverse();
    versions
        .into_iter()
        .find_map(|root| codex_builtin_plugin_from_root(marketplace, root, enabled_ids))
}

pub(crate) fn openai_bundled_plugins_root_from_install_location(install_location: PathBuf) -> Option<PathBuf> {
    let root = install_location
        .join("app")
        .join("resources")
        .join("plugins");
    root.join(OPENAI_BUNDLED_MARKETPLACE_NAME)
        .join(".agents")
        .join("plugins")
        .join("marketplace.json")
        .is_file()
        .then_some(root)
}

#[cfg(target_os = "windows")]
pub(crate) fn find_openai_codex_install_locations_from_appx() -> Vec<PathBuf> {
    let mut command = std::process::Command::new("powershell");
    command.args([
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        "Get-AppxPackage -Name OpenAI.Codex | Select-Object -ExpandProperty InstallLocation",
    ]);
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);
    let Ok(output) = command.output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect()
}

#[cfg(target_os = "windows")]
pub(crate) fn find_openai_codex_install_locations_from_windows_apps() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut bases = Vec::new();
    if let Some(program_files) = std::env::var_os("ProgramFiles").map(PathBuf::from) {
        bases.push(program_files.join("WindowsApps"));
    }
    bases.push(PathBuf::from("C:/Program Files/WindowsApps"));
    for windows_apps in bases {
        let Ok(entries) = fs::read_dir(windows_apps) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("OpenAI.Codex_") {
                roots.push(entry.path());
            }
        }
    }
    roots.sort();
    roots.dedup();
    roots
}

pub(crate) fn find_openai_bundled_plugins_root() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let mut install_locations = find_openai_codex_install_locations_from_appx();
        install_locations.extend(find_openai_codex_install_locations_from_windows_apps());
        install_locations.sort();
        install_locations.dedup();
        install_locations
            .into_iter()
            .rev()
            .find_map(openai_bundled_plugins_root_from_install_location)
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

pub(crate) fn codex_plugin_cache_root() -> PathBuf {
    codex_config_dir().join("plugins").join("cache")
}

pub(crate) fn cached_marketplace_has_plugins(path: &Path) -> bool {
    let Ok(entries) = fs::read_dir(path) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let plugin_path = entry.path();
        plugin_path.is_dir()
            && plugin_path.file_name().and_then(|n| n.to_str()) != Some(".agents")
            && newest_codex_builtin_plugin_entry("probe", &plugin_path, &HashSet::new()).is_some()
    })
}

pub(crate) fn find_cached_codex_plugin_marketplaces() -> Vec<(String, PathBuf)> {
    let cache_root = codex_plugin_cache_root();
    let Ok(entries) = fs::read_dir(cache_root) else {
        return Vec::new();
    };
    let mut marketplaces = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_dir() || !cached_marketplace_has_plugins(&path) {
                return None;
            }
            let name = entry.file_name().to_string_lossy().trim().to_string();
            (!name.is_empty()).then_some((name, path))
        })
        .collect::<Vec<_>>();
    marketplaces
        .sort_by(|left, right| marketplace_priority(&left.0).cmp(&marketplace_priority(&right.0)));
    marketplaces
}

pub(crate) fn marketplace_priority(name: &str) -> usize {
    match name {
        "openai-bundled" => 0,
        "varswitch-plugins" => 1,
        "cooper-plugins" => 2,
        "openai-primary-runtime" => 3,
        "openai-curated" | "openai-curated-remote" | "openai-api-curated" => 4,
        "personal" => 8,
        _ => 9,
    }
}

pub(crate) fn find_openai_bundled_marketplace_root() -> Option<PathBuf> {
    find_openai_bundled_plugins_root()
        .map(|root| root.join(OPENAI_BUNDLED_MARKETPLACE_NAME))
        .or_else(|| {
            let cached = codex_plugin_cache_root().join(OPENAI_BUNDLED_MARKETPLACE_NAME);
            cached_marketplace_has_plugins(&cached).then_some(cached)
        })
}

/// 插件市场发现结果的有效期。Windows 上这一步要起一个 PowerShell 进程查 Appx，
/// 还要遍历 WindowsApps 目录，开销以百毫秒计；而 Codex App 的安装路径在应用运行期间
/// 基本不变，因此缓存 60 秒，避免 Toolbox 页每秒轮询都重扫一遍。
pub(crate) const PLUGIN_MARKETPLACE_TTL_MS: u64 = 60_000;
pub(crate) static PLUGIN_MARKETPLACE_CACHE: Mutex<Option<(u64, Vec<(String, PathBuf)>)>> =
    Mutex::new(None);

/// 安装/修复插件市场后目录结构会变，作废缓存让下一次发现重新扫描
pub(crate) fn invalidate_plugin_marketplace_cache() {
    if let Ok(mut guard) = PLUGIN_MARKETPLACE_CACHE.lock() {
        *guard = None;
    }
}

pub(crate) fn discover_codex_plugin_marketplaces() -> Vec<(String, PathBuf)> {
    let now = chrono_timestamp_millis() as u64;
    if let Ok(guard) = PLUGIN_MARKETPLACE_CACHE.lock() {
        if let Some((cached_at, cached)) = guard.as_ref() {
            if now.saturating_sub(*cached_at) < PLUGIN_MARKETPLACE_TTL_MS {
                return cached.clone();
            }
        }
    }
    let discovered = discover_codex_plugin_marketplaces_uncached();
    if let Ok(mut guard) = PLUGIN_MARKETPLACE_CACHE.lock() {
        *guard = Some((now, discovered.clone()));
    }
    discovered
}

fn discover_codex_plugin_marketplaces_uncached() -> Vec<(String, PathBuf)> {
    let mut marketplaces = Vec::new();
    if let Some(root) = find_openai_bundled_marketplace_root() {
        marketplaces.push((OPENAI_BUNDLED_MARKETPLACE_NAME.to_string(), root));
    }
    marketplaces.extend(find_cached_codex_plugin_marketplaces());
    marketplaces.sort_by(|left, right| {
        marketplace_priority(&left.0)
            .cmp(&marketplace_priority(&right.0))
            .then_with(|| left.1.cmp(&right.1))
    });
    let mut seen = HashSet::new();
    marketplaces
        .into_iter()
        .filter(|(_, path)| seen.insert(path.clone()))
        .collect()
}

pub(crate) fn enabled_codex_plugin_ids_from_config(config_text: &str) -> HashSet<String> {
    let mut ids = HashSet::new();
    let lines = config_text.lines().collect::<Vec<_>>();
    let mut index = 0;
    while index < lines.len() {
        if lines[index].trim() != "[plugins]" {
            index += 1;
            continue;
        }
        index += 1;
        while index < lines.len() {
            let trimmed = lines[index].trim();
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                break;
            }
            if trimmed.contains("enabled") && trimmed.contains("true") {
                if let Some((left, _)) = trimmed.split_once('=') {
                    let id = left.trim().trim_matches('"').to_string();
                    if !id.is_empty() {
                        ids.insert(id);
                    }
                }
            }
            index += 1;
        }
        break;
    }
    ids
}

pub(crate) fn list_codex_builtin_plugins(config_text: &str) -> CodexBuiltinPluginStatus {
    let marketplaces = discover_codex_plugin_marketplaces();
    if marketplaces.is_empty() {
        return CodexBuiltinPluginStatus {
            available: false,
            last_error: "未找到 Codex App 自带插件或本地插件缓存；已检查 Codex App openai-bundled 与 ~/.codex/plugins/cache".into(),
            ..Default::default()
        };
    }

    let enabled_ids = enabled_codex_plugin_ids_from_config(config_text);
    let mut plugins = Vec::new();
    let mut sources = Vec::new();
    let mut configured_any = false;
    let mut seen_names = HashSet::new();

    for (marketplace, marketplace_root) in marketplaces {
        let marketplace_source = marketplace_root.to_string_lossy().to_string();
        sources.push(format!("{}={}", marketplace, marketplace_source));
        configured_any |= config_has_plugin_marketplace_source(config_text, &marketplace_source);

        let Ok(plugin_dirs) = fs::read_dir(&marketplace_root) else {
            continue;
        };
        for plugin_dir in plugin_dirs.flatten() {
            let plugin_path = plugin_dir.path();
            if !plugin_path.is_dir()
                || plugin_path.file_name().and_then(|n| n.to_str()) == Some(".agents")
            {
                continue;
            }
            if let Some(entry) =
                newest_codex_builtin_plugin_entry(&marketplace, &plugin_path, &enabled_ids)
            {
                let key = entry.name.clone();
                if seen_names.insert(key) {
                    plugins.push(entry);
                }
            }
        }
    }

    plugins.sort_by(|left, right| {
        right
            .important
            .cmp(&left.important)
            .then_with(|| {
                marketplace_priority(&left.marketplace)
                    .cmp(&marketplace_priority(&right.marketplace))
            })
            .then_with(|| left.display_name.cmp(&right.display_name))
    });
    let total_count = plugins.len();
    let enabled_count = plugins.iter().filter(|plugin| plugin.enabled).count();
    let important_total_count = plugins.iter().filter(|plugin| plugin.important).count();
    let important_enabled_count = plugins
        .iter()
        .filter(|plugin| plugin.important && plugin.enabled)
        .count();
    CodexBuiltinPluginStatus {
        available: total_count > 0,
        marketplace_source: sources.join("; "),
        marketplace_configured: configured_any,
        enabled_count,
        total_count,
        important_enabled_count,
        important_total_count,
        plugins,
        last_error: if total_count > 0 {
            String::new()
        } else {
            "已发现插件来源，但没有解析到可启用的 Codex 插件".into()
        },
    }
}

pub(crate) fn ensure_discovered_plugin_marketplaces_config(config_text: &str) -> Result<String, String> {
    let marketplaces = discover_codex_plugin_marketplaces();
    let mut next = remove_invalid_local_plugin_marketplace_sections(config_text);
    for (marketplace, root) in marketplaces {
        if !plugin_marketplace_root_has_supported_manifest(&root) {
            continue;
        }
        next = ensure_plugin_marketplace_section(
            &next,
            &marketplace,
            &root.to_string_lossy(),
            "local",
        );
    }
    Ok(next)
}

pub(crate) fn toml_double_quoted_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

pub(crate) fn plugin_inline_entry_matches(line: &str, plugin_id: &str) -> bool {
    let trimmed = line.trim_start();
    let quoted = format!("\"{}\"", toml_double_quoted_value(plugin_id));
    trimmed.starts_with(&quoted) || trimmed.starts_with(plugin_id)
}

pub(crate) fn plugin_table_header_matches(line: &str, plugin_id: &str) -> bool {
    let trimmed = line.trim();
    let double_header = format!("[plugins.\"{}\"]", toml_double_quoted_value(plugin_id));
    let single_header = format!("[plugins.'{}']", plugin_id.replace('\'', "\\'"));
    trimmed == double_header || trimmed == single_header
}

pub(crate) fn write_enabled_codex_plugin_config(config_text: &str, plugin_id: &str) -> String {
    let plugins_header = "[plugins]";
    let plugin_line = format!(
        "\"{}\" = {{ enabled = true }}",
        toml_double_quoted_value(plugin_id)
    );
    let mut lines = config_text.lines().map(str::to_string).collect::<Vec<_>>();

    let mut table_start = None;
    let mut table_end = lines.len();
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if plugin_table_header_matches(trimmed, plugin_id) {
            table_start = Some(index);
            table_end = lines.len();
            continue;
        }
        if table_start.is_some() && trimmed.starts_with('[') && trimmed.ends_with(']') {
            table_end = index;
            break;
        }
    }

    if let Some(start) = table_start {
        let mut enabled_index = None;
        for index in (start + 1)..table_end {
            if lines[index].trim_start().starts_with("enabled") {
                enabled_index = Some(index);
                break;
            }
        }
        if let Some(index) = enabled_index {
            lines[index] = "enabled = true".to_string();
        } else {
            lines.insert(table_end, "enabled = true".to_string());
        }

        let mut plugins_start = None;
        let mut plugins_end = lines.len();
        for (index, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed == plugins_header {
                plugins_start = Some(index);
                continue;
            }
            if plugins_start.is_some() && trimmed.starts_with('[') && trimmed.ends_with(']') {
                plugins_end = index;
                break;
            }
        }
        if let Some(start) = plugins_start {
            for index in ((start + 1)..plugins_end).rev() {
                if plugin_inline_entry_matches(&lines[index], plugin_id) {
                    lines.remove(index);
                }
            }
        }
        return lines.join("\n");
    }

    let mut plugins_start = None;
    let mut plugins_end = lines.len();
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed == plugins_header {
            plugins_start = Some(index);
            continue;
        }
        if plugins_start.is_some() && trimmed.starts_with('[') && trimmed.ends_with(']') {
            plugins_end = index;
            break;
        }
    }
    if let Some(start) = plugins_start {
        for index in (start + 1)..plugins_end {
            if plugin_inline_entry_matches(&lines[index], plugin_id) {
                lines[index] = plugin_line;
                return lines.join("\n");
            }
        }
        lines.insert(plugins_end, plugin_line);
        return lines.join("\n");
    }

    let mut text = config_text.trim_end().to_string();
    if !text.is_empty() {
        text.push_str("\n\n");
    }
    text.push_str(plugins_header);
    text.push('\n');
    text.push_str(&plugin_line);
    text.push('\n');
    text
}
pub(crate) fn default_plugin_marketplace_url() -> &'static str {
    "https://gitcode.com/2301_79703673/codex-plugins.git"
}

pub(crate) fn varswitch_github_plugin_marketplace_url() -> &'static str {
    "https://github.com/ConcertoNotes/codex-plugins.git"
}

pub(crate) fn upstream_gitcode_plugin_marketplace_url() -> &'static str {
    "https://gitcode.com/weixin_65003717/codex-plugin.git"
}

pub(crate) fn awesome_plugin_marketplace_url() -> &'static str {
    "https://github.com/hashgraph-online/awesome-codex-plugins.git"
}

pub(crate) fn default_plugin_marketplace_name() -> &'static str {
    "varswitch-plugins"
}

pub(crate) fn supported_plugin_marketplace_source(source: &str) -> &'static str {
    let trimmed = source.trim();
    if trimmed == varswitch_github_plugin_marketplace_url()
        || trimmed == "git@github.com:ConcertoNotes/codex-plugins.git"
    {
        varswitch_github_plugin_marketplace_url()
    } else if trimmed == upstream_gitcode_plugin_marketplace_url() {
        upstream_gitcode_plugin_marketplace_url()
    } else if trimmed == awesome_plugin_marketplace_url()
        || trimmed == "git@github.com:hashgraph-online/awesome-codex-plugins.git"
    {
        awesome_plugin_marketplace_url()
    } else {
        default_plugin_marketplace_url()
    }
}

pub(crate) fn lark_create_bot_launcher_url() -> &'static str {
    "https://open.feishu.cn/page/launcher?from=sdk&source=node-sdk%2Fvarswitch&tp=sdk&addons=H4sIAAAAAAAC_2XLMQqAMAwF0LtkloJrriJSav1Ih6RiQpfSuyvi5v5eJ8v1hBF3cmhSJ16oCAvM0gE26B6Txa06rWMiNKi_vDjk98L3woWM0hDb_LQxbv9jmpZoAAAA&createOnly=true&name=VarSwitch+%E6%99%BA%E8%83%BD%E4%BD%93&desc=%E6%8A%8A%E9%A3%9E%E4%B9%A6%E6%B6%88%E6%81%AF%E8%BD%AC%E5%8F%91%E7%BB%99+Codex%EF%BC%8C%E5%B9%B6%E6%8A%8A+Codex+%E5%9B%9E%E5%A4%8D%E5%90%8C%E6%AD%A5%E5%9B%9E%E9%A3%9E%E4%B9%A6%E3%80%82"
}

pub(crate) fn lark_registration_base_url() -> &'static str {
    "https://accounts.feishu.cn"
}

pub(crate) fn lark_registration_endpoint() -> String {
    format!("{}/oauth/v1/app/registration", lark_registration_base_url())
}

pub(crate) fn lark_registration_addons() -> &'static str {
    "H4sIAAAAAAAC_2XLMQqAMAwF0LtkloJrriJSav1Ih6RiQpfSuyvi5v5eJ8v1hBF3cmhSJ16oCAvM0gE26B6Txa06rWMiNKi_vDjk98L3woWM0hDb_LQxbv9jmpZoAAAA"
}

pub(crate) fn percent_encode_query_value(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(*byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

pub(crate) fn append_query_params(url: &str, params: &[(&str, &str)]) -> String {
    let mut next = url.trim().to_string();
    if next.is_empty() {
        return String::new();
    }
    let mut first = !next.contains('?');
    for (key, value) in params {
        if key.trim().is_empty() {
            continue;
        }
        if first {
            next.push('?');
            first = false;
        } else if !next.ends_with('?') && !next.ends_with('&') {
            next.push('&');
        }
        next.push_str(&percent_encode_query_value(key));
        next.push('=');
        next.push_str(&percent_encode_query_value(value));
    }
    next
}

pub(crate) fn base64_encode_bytes(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(((bytes.len() + 2) / 3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0b0000_0011) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[(((b1 & 0b0000_1111) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(b2 & 0b0011_1111) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// 将文本(通常是 URL)生成二维码图片的 PNG data URL。用于微信/QQ 等需本地生成二维码的场景。
pub(crate) fn generate_qr_code_data_url(content: &str) -> Result<String, String> {
    let code = QrCode::with_error_correction_level(content, EcLevel::M)
        .map_err(|e| format!("生成二维码失败: {e}"))?;
    let image = code.render::<Luma<u8>>().min_dimensions(240, 240).build();
    let dyn_img = DynamicImage::ImageLuma8(image);
    let mut png_bytes = Vec::new();
    dyn_img
        .write_to(
            &mut std::io::Cursor::new(&mut png_bytes),
            image::ImageFormat::Png,
        )
        .map_err(|e| format!("PNG 编码失败: {e}"))?;
    let b64 = base64_encode_bytes(&png_bytes);
    Ok(format!("data:image/png;base64,{b64}"))
}

pub(crate) fn official_plugin_marketplace_names() -> &'static [&'static str] {
    &["openai-bundled", "openai-primary-runtime"]
}

pub(crate) fn local_device_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "VarSwitch".to_string())
}

pub(crate) fn detect_local_ip() -> String {
    if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
        let _ = socket.connect("8.8.8.8:80");
        if let Ok(addr) = socket.local_addr() {
            return addr.ip().to_string();
        }
    }
    "127.0.0.1".to_string()
}

pub(crate) fn read_lines_if_exists(path: &PathBuf) -> Vec<String> {
    fs::read_to_string(path)
        .ok()
        .map(|text| {
            text.lines()
                .map(|line| line.trim().to_string())
                .filter(|line| !line.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn codex_session_relative_file(thread_id: &str) -> Option<String> {
    let root = codex_sessions_root();
    if !root.exists() {
        return None;
    }
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                    continue;
                };
                if !name.ends_with(".jsonl") {
                    continue;
                }
                if name.contains(thread_id) {
                    if let Ok(relative) = path.strip_prefix(codex_config_dir()) {
                        return Some(relative.to_string_lossy().replace('\\', "/"));
                    }
                }
            }
        }
    }
    None
}

pub(crate) fn codex_session_path_from_relative(relative: &str) -> PathBuf {
    let text = relative.replace('/', "\\");
    codex_config_dir().join(text)
}

pub(crate) fn extract_text_from_content_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Array(items) => items
            .iter()
            .map(extract_text_from_content_value)
            .filter(|text| !text.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        serde_json::Value::Object(map) => {
            if let Some(text) = map.get("text").and_then(|value| value.as_str()) {
                return text.to_string();
            }
            if let Some(text) = map.get("content").map(extract_text_from_content_value) {
                if !text.trim().is_empty() {
                    return text;
                }
            }
            if let Some(text) = map.get("message").map(extract_text_from_content_value) {
                if !text.trim().is_empty() {
                    return text;
                }
            }
            String::new()
        }
        _ => String::new(),
    }
}

pub(crate) fn shorten_preview(text: &str, max_chars: usize) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = compact.chars();
    let preview: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{preview}...")
    } else {
        preview
    }
}

pub(crate) fn read_codex_thread_preview(session_file: &str) -> (String, String, String) {
    let path = codex_session_path_from_relative(session_file);
    let text = fs::read_to_string(path).unwrap_or_default();
    let mut cwd = String::new();
    let mut last_user_message = String::new();
    let mut last_assistant_message = String::new();

    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if cwd.is_empty() {
            cwd = value
                .pointer("/payload/cwd")
                .and_then(|value| value.as_str())
                .or_else(|| {
                    value
                        .pointer("/payload/origin/cwd")
                        .and_then(|value| value.as_str())
                })
                .unwrap_or("")
                .to_string();
        }
        let role = value
            .pointer("/payload/role")
            .and_then(|value| value.as_str())
            .or_else(|| {
                value
                    .pointer("/payload/message/role")
                    .and_then(|value| value.as_str())
            })
            .unwrap_or("");

        let text = value
            .pointer("/payload/content")
            .map(extract_text_from_content_value)
            .or_else(|| {
                value
                    .pointer("/payload/message/content")
                    .map(extract_text_from_content_value)
            })
            .or_else(|| {
                value
                    .pointer("/payload/text")
                    .map(extract_text_from_content_value)
            })
            .unwrap_or_default();

        if text.trim().is_empty() {
            continue;
        }

        match role {
            "user" => last_user_message = shorten_preview(&text, 140),
            "assistant" => last_assistant_message = shorten_preview(&text, 180),
            _ => {}
        }
    }

    (cwd, last_user_message, last_assistant_message)
}

pub(crate) fn read_codex_threads(limit: usize) -> Vec<CodexThreadRecord> {
    let mut items = Vec::new();
    for line in read_lines_if_exists(&codex_session_index_path())
        .into_iter()
        .rev()
    {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        let id = value
            .get("id")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if id.is_empty() || items.iter().any(|item: &CodexThreadRecord| item.id == id) {
            continue;
        }
        let thread_name = value
            .get("thread_name")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let updated_at = value
            .get("updated_at")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let Some(session_file) = codex_session_relative_file(&id) else {
            continue;
        };
        let (cwd, last_user_message, last_assistant_message) =
            read_codex_thread_preview(&session_file);
        items.push(CodexThreadRecord {
            id,
            thread_name,
            updated_at,
            session_file,
            cwd,
            last_user_message,
            last_assistant_message,
        });
        if items.len() >= limit {
            break;
        }
    }
    items
}

// ============================================================
// 会话管理器：浏览/搜索 Claude Code 与 Codex 的本机历史会话并一键恢复
// ============================================================


pub(crate) fn list_plugin_marketplaces(config_text: &str, current_source: &str) -> Vec<PluginMarketplaceItem> {
    let mut result = Vec::new();
    let lines: Vec<&str> = config_text.lines().collect();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index].trim();
        if !(line.starts_with("[marketplaces.") && line.ends_with(']')) {
            index += 1;
            continue;
        }
        let name = line
            .trim_start_matches("[marketplaces.")
            .trim_end_matches(']')
            .trim_matches('"')
            .trim_matches('\'')
            .to_string();
        let mut source = String::new();
        let mut source_type = String::new();
        index += 1;
        while index < lines.len() {
            let current = lines[index].trim();
            if current.starts_with('[') && current.ends_with(']') {
                break;
            }
            if let Some(value) = current.strip_prefix("source =") {
                source = parse_toml_string_value(value);
            } else if let Some(value) = current.strip_prefix("source_type =") {
                source_type = parse_toml_string_value(value);
            }
            index += 1;
        }
        result.push(PluginMarketplaceItem {
            name: name.clone(),
            source: source.clone(),
            source_type: source_type.clone(),
            is_official: official_plugin_marketplace_names().contains(&name.as_str()),
            is_current: !current_source.trim().is_empty() && current_source.trim() == source.trim(),
        });
    }
    result
}


pub(crate) fn ensure_plugin_marketplace_section(
    existing: &str,
    name: &str,
    source: &str,
    source_type: &str,
) -> String {
    let header = format!("[marketplaces.{name}]");
    let section = format!(
        "{header}\nlast_updated = \"{}\"\nsource_type = \"{}\"\nsource = \"{}\"\n",
        chrono_now(),
        source_type,
        source.replace('\\', "\\\\").replace('"', "\\\"")
    );

    let lines: Vec<&str> = existing.lines().collect();
    let mut output = Vec::new();
    let mut index = 0;
    let mut replaced = false;

    while index < lines.len() {
        let line = lines[index].trim();
        if line == header {
            replaced = true;
            output.push(section.trim_end().to_string());
            index += 1;
            while index < lines.len() {
                let current = lines[index].trim();
                if current.starts_with('[') && current.ends_with(']') {
                    break;
                }
                index += 1;
            }
            continue;
        }
        output.push(lines[index].to_string());
        index += 1;
    }

    let mut text = output.join("\n").trim().to_string();
    if text.is_empty() {
        return section;
    }
    if !replaced {
        text.push_str("\n\n");
        text.push_str(&section);
        return text;
    }
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

pub(crate) fn remove_all_plugin_marketplace_sections(existing: &str) -> String {
    let lines: Vec<&str> = existing.lines().collect();
    let mut output = Vec::new();
    let mut index = 0;
    let mut skipping = false;

    while index < lines.len() {
        let trimmed = lines[index].trim();
        if trimmed.starts_with("[marketplaces.") && trimmed.ends_with(']') {
            skipping = true;
            index += 1;
            continue;
        }
        if skipping && trimmed.starts_with('[') && trimmed.ends_with(']') {
            skipping = false;
        }
        if !skipping {
            output.push(lines[index].to_string());
        }
        index += 1;
    }

    let text = output.join("\n");
    let mut compact = Vec::new();
    let mut last_blank = false;
    for line in text.lines() {
        let blank = line.trim().is_empty();
        if blank && last_blank {
            continue;
        }
        compact.push(line.to_string());
        last_blank = blank;
    }
    compact.join("\n").trim().to_string()
}

pub(crate) fn plugin_marketplace_root_has_supported_manifest(path: &Path) -> bool {
    [
        path.join(".codex-plugin").join("marketplace.json"),
        path.join(".agents")
            .join("plugins")
            .join("marketplace.json"),
        path.join("marketplace.json"),
    ]
    .into_iter()
    .any(|candidate| candidate.is_file())
}

pub(crate) fn marketplace_config_value(line: &str, key: &str) -> Option<String> {
    let (left, right) = line.split_once('=')?;
    if left.trim() != key {
        return None;
    }
    let raw = right.trim();
    if raw.len() >= 2 && raw.starts_with('"') && raw.ends_with('"') {
        return Some(
            raw[1..raw.len() - 1]
                .replace("\\\"", "\"")
                .replace("\\\\", "\\"),
        );
    }
    Some(raw.trim_matches('\'').to_string())
}

pub(crate) fn remove_invalid_local_plugin_marketplace_sections(existing: &str) -> String {
    let lines = existing.lines().collect::<Vec<_>>();
    let mut output = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let trimmed = lines[index].trim();
        if !(trimmed.starts_with("[marketplaces.") && trimmed.ends_with(']')) {
            output.push(lines[index].to_string());
            index += 1;
            continue;
        }

        let section_start = index;
        index += 1;
        while index < lines.len() {
            let next = lines[index].trim();
            if next.starts_with('[') && next.ends_with(']') {
                break;
            }
            index += 1;
        }
        let section = &lines[section_start..index];
        let source_type = section
            .iter()
            .find_map(|line| marketplace_config_value(line, "source_type"))
            .unwrap_or_default();
        let source = section
            .iter()
            .find_map(|line| marketplace_config_value(line, "source"))
            .unwrap_or_default();
        let invalid_local = source_type.eq_ignore_ascii_case("local")
            && !source.is_empty()
            && !plugin_marketplace_root_has_supported_manifest(Path::new(&source));

        if !invalid_local {
            output.extend(section.iter().map(|line| (*line).to_string()));
        }
    }

    output.join("\n").trim().to_string()
}

pub(crate) fn plugin_marketplace_source_type(source: &str) -> &'static str {
    let lowered = source.to_ascii_lowercase();
    if lowered.starts_with("http://")
        || lowered.starts_with("https://")
        || lowered.starts_with("ssh://")
        || lowered.starts_with("git@")
    {
        "git"
    } else {
        "local"
    }
}

pub(crate) fn run_codex_plugin_marketplace_add(source: &str) -> Result<(), String> {
    let executable = resolve_codex_command()?;
    let mut command = codex_command(&executable);
    command
        .args(["plugin", "marketplace", "add", source, "--json"])
        .env("CODEX_HOME", codex_config_dir())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = command.output().map_err(|e| {
        format!(
            "启动 Codex CLI 安装插件市场失败({}): {e}",
            executable.display()
        )
    })?;
    if output.status.success() {
        return Ok(());
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let detail = [stdout, stderr]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    Err(format!(
        "Codex CLI 安装插件市场失败(exit {})：{}",
        output.status.code().unwrap_or(-1),
        if detail.is_empty() {
            "无错误输出".to_string()
        } else {
            detail
        }
    ))
}

pub(crate) fn run_codex_plugin_marketplace_remove(name: &str) -> Result<(), String> {
    let executable = resolve_codex_command()?;
    let mut command = codex_command(&executable);
    command
        .args(["plugin", "marketplace", "remove", name, "--json"])
        .env("CODEX_HOME", codex_config_dir())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = command.output().map_err(|e| {
        format!(
            "启动 Codex CLI 移除插件市场失败({}): {e}",
            executable.display()
        )
    })?;
    if output.status.success() {
        return Ok(());
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let detail = [stdout, stderr]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if detail.to_ascii_lowercase().contains("not found")
        || detail.to_ascii_lowercase().contains("unknown marketplace")
    {
        return Ok(());
    }
    Err(format!(
        "Codex CLI 移除插件市场失败(exit {})：{}",
        output.status.code().unwrap_or(-1),
        if detail.is_empty() {
            "无错误输出".to_string()
        } else {
            detail
        }
    ))
}

pub(crate) fn configure_git_longpaths_for_windows() {
    if !cfg!(windows) {
        return;
    }
    let mut command = Command::new("git");
    command.args(["config", "--global", "core.longpaths", "true"]);
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);
    let _ = command.output();
}

pub(crate) fn clean_plugin_marketplace_cache(name: &str) {
    let codex_home = codex_config_dir();
    let paths = [
        codex_home.join(".tmp").join("marketplaces").join(name),
        codex_home
            .join(".tmp")
            .join("marketplaces")
            .join(".staging"),
        codex_home.join("plugins").join("cache").join(name),
    ];
    for path in paths {
        if path.exists() {
            let _ = fs::remove_dir_all(path);
        }
    }
}

pub(crate) fn plugin_marketplace_snapshot_exists(name: &str) -> bool {
    let config_dir = codex_config_dir();
    cached_marketplace_has_plugins(&config_dir.join("plugins").join("cache").join(name))
        || plugin_marketplace_root_has_supported_manifest(
            &config_dir
                .join("plugins")
                .join(".tmp")
                .join("marketplaces")
                .join(name),
        )
        || plugin_marketplace_root_has_supported_manifest(
            &config_dir.join(".tmp").join("marketplaces").join(name),
        )
}

pub(crate) fn is_marketplace_different_source_error(error: &str) -> bool {
    let lowered = error.to_ascii_lowercase();
    lowered.contains("already added from a different source")
        || (lowered.contains("different source") && lowered.contains("marketplace"))
}

pub(crate) fn repair_and_add_codex_plugin_marketplace(source: &str) -> Result<(), String> {
    configure_git_longpaths_for_windows();
    match run_codex_plugin_marketplace_add(source) {
        Ok(()) => return Ok(()),
        Err(first_error) if is_marketplace_different_source_error(&first_error) => {
            let name = default_plugin_marketplace_name();
            let _ = run_codex_plugin_marketplace_remove(name);
            clean_plugin_marketplace_cache(name);
            configure_git_longpaths_for_windows();
            run_codex_plugin_marketplace_add(source).map_err(|second_error| {
                format!(
                    "{second_error}\n已检测到同名插件市场来源冲突，并自动执行 remove + 清理缓存 + 重新 add，但重新添加仍失败。\n原始错误：{first_error}"
                )
            })?;
            Ok(())
        }
        Err(error) => Err(error),
    }
}

pub(crate) fn config_has_plugin_marketplace_source(config_text: &str, source: &str) -> bool {
    list_plugin_marketplaces(config_text, source)
        .iter()
        .any(|item| item.source.trim() == source.trim())
}

pub(crate) fn normalize_mobile_base_url(value: &str, channel: &str) -> String {
    let fallback = default_mobile_channel_base_url(channel);
    let mut raw = value.trim().trim_end_matches('/').to_string();
    if raw.is_empty() {
        raw = fallback.to_string();
    }
    if !raw.is_empty() && !raw.starts_with("http://") && !raw.starts_with("https://") {
        raw = format!("https://{raw}");
    }
    // B5: Bearer token 走明文 http 会泄露凭据，强制升级为 https
    if raw.starts_with("http://") {
        raw = format!("https://{}", &raw["http://".len()..]);
    }
    raw.trim_end_matches('/').to_string()
}

pub(crate) fn mobile_channel_credential_hint(channel: &str) -> &'static str {
    match normalize_mobile_channel(channel).as_str() {
        "lark" => "请填写飞书 App ID 和 App Secret",
        "wechat" => "请填写微信 iLink Bot Token",
        "qq" => "请填写 QQ Bot AppID 和 AppSecret",
        _ => "请填写平台凭据",
    }
}

pub(crate) fn mobile_channel_has_credentials(binding: &MobileChannelBinding) -> bool {
    match normalize_mobile_channel(&binding.channel).as_str() {
        "lark" | "qq" => !binding.app_id.trim().is_empty() && !binding.app_secret.trim().is_empty(),
        "wechat" => !binding.bot_token.trim().is_empty(),
        _ => false,
    }
}

pub(crate) fn is_platform_code_ok(value: &serde_json::Value) -> bool {
    match value.get("code").or_else(|| value.get("errcode")) {
        None | Some(serde_json::Value::Null) => true,
        Some(serde_json::Value::Number(number)) => number.as_i64().unwrap_or(0) == 0,
        Some(serde_json::Value::String(text)) => text.trim().is_empty() || text.trim() == "0",
        _ => false,
    }
}

pub(crate) fn platform_error_message(value: &serde_json::Value, fallback: &str) -> String {
    value
        .get("msg")
        .or_else(|| value.get("message"))
        .or_else(|| value.get("errmsg"))
        .and_then(|item| item.as_str())
        .filter(|text| !text.trim().is_empty())
        .unwrap_or(fallback)
        .to_string()
}

pub(crate) fn gateway_url_label(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|parsed| {
            parsed
                .host_str()
                .map(|host| format!("{}://{}", parsed.scheme(), host))
        })
        .unwrap_or_else(|| "平台网关".into())
}

pub(crate) fn post_json_value(
    client: &reqwest::blocking::Client,
    url: String,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let response = client
        .post(&url)
        .json(&payload)
        .send()
        .map_err(|e| format!("平台接口请求失败: {e}"))?;
    let status = response.status();
    let body = response
        .text()
        .map_err(|e| format!("平台接口响应读取失败: {e}"))?;
    if !status.is_success() {
        return Err(format!("平台接口返回 HTTP {}：{}", status.as_u16(), body));
    }
    if body.trim().is_empty() {
        return Ok(serde_json::json!({}));
    }
    serde_json::from_str(&body).map_err(|e| format!("平台接口返回不是 JSON: {e}; {body}"))
}

pub(crate) fn probe_lark_channel(
    client: &reqwest::blocking::Client,
    binding: &MobileChannelBinding,
) -> Result<(String, String), String> {
    if !mobile_channel_has_credentials(binding) {
        return Err(mobile_channel_credential_hint("lark").into());
    }
    let base_url = normalize_mobile_base_url(&binding.base_url, "lark");
    let result = post_json_value(
        client,
        format!("{base_url}/callback/ws/endpoint"),
        serde_json::json!({
            "AppID": binding.app_id.trim(),
            "AppSecret": binding.app_secret.trim(),
        }),
    )?;
    if !is_platform_code_ok(&result) {
        return Err(format!(
            "飞书 WebSocket 网关获取失败：{}",
            platform_error_message(&result, "接口返回异常")
        ));
    }
    let data = result.get("data").and_then(|item| item.as_object());
    let gateway_url = data
        .and_then(|item| item.get("URL").or_else(|| item.get("url")))
        .and_then(|item| item.as_str())
        .unwrap_or_default()
        .trim()
        .to_string();
    if gateway_url.is_empty() {
        return Err("飞书 WebSocket 网关地址为空".into());
    }
    Ok((
        gateway_url.clone(),
        format!(
            "飞书 WebSocket 网关已验证：{}",
            gateway_url_label(&gateway_url)
        ),
    ))
}

pub(crate) fn probe_qq_channel(
    client: &reqwest::blocking::Client,
    binding: &MobileChannelBinding,
) -> Result<(String, String), String> {
    if !mobile_channel_has_credentials(binding) {
        return Err(mobile_channel_credential_hint("qq").into());
    }
    let token_result = post_json_value(
        client,
        "https://bots.qq.com/app/getAppAccessToken".into(),
        serde_json::json!({
            "appId": binding.app_id.trim(),
            "clientSecret": binding.app_secret.trim(),
        }),
    )?;
    let token = token_result
        .get("access_token")
        .and_then(|item| item.as_str())
        .unwrap_or_default()
        .trim()
        .to_string();
    if token.is_empty() {
        return Err(format!(
            "QQ Bot 鉴权失败：{}",
            platform_error_message(&token_result, "未返回 access_token")
        ));
    }
    let response = client
        .get("https://api.sgroup.qq.com/gateway")
        .header("Authorization", format!("QQBot {token}"))
        .send()
        .map_err(|e| format!("QQ 网关请求失败: {e}"))?;
    let status = response.status();
    let body = response
        .text()
        .map_err(|e| format!("QQ 网关响应读取失败: {e}"))?;
    if !status.is_success() {
        return Err(format!("QQ 网关返回 HTTP {}：{}", status.as_u16(), body));
    }
    let result: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("QQ 网关返回不是 JSON: {e}; {body}"))?;
    let gateway_url = result
        .get("url")
        .and_then(|item| item.as_str())
        .unwrap_or_default()
        .trim()
        .to_string();
    if gateway_url.is_empty() {
        return Err("QQ 网关地址为空".into());
    }
    Ok((
        gateway_url.clone(),
        format!(
            "QQ WebSocket 网关已验证：{}",
            gateway_url_label(&gateway_url)
        ),
    ))
}

pub(crate) fn probe_wechat_channel(
    _client: &reqwest::blocking::Client,
    binding: &MobileChannelBinding,
) -> Result<(String, String), String> {
    // B17: 不再用 notifystart 当探活（语义重复且会让 start_wechat_listener 再次 notifystart 两次）
    // 只做凭据格式校验 + URL 规范化，真正的 notifystart 由 start_wechat_listener 调用
    if !mobile_channel_has_credentials(binding) {
        return Err(mobile_channel_credential_hint("wechat").into());
    }
    let base_url = normalize_mobile_base_url(&binding.base_url, "wechat");
    Ok((
        base_url.clone(),
        format!("微信 iLink 凭据已验证：{}", gateway_url_label(&base_url)),
    ))
}

pub(crate) fn probe_mobile_channel(binding: &MobileChannelBinding) -> Result<(String, String), String> {
    let client = build_http_client(15)?;
    match normalize_mobile_channel(&binding.channel).as_str() {
        "lark" => probe_lark_channel(&client, binding),
        "wechat" => probe_wechat_channel(&client, binding),
        "qq" => probe_qq_channel(&client, binding),
        _ => Err("不支持的手机通道".into()),
    }
}

pub(crate) fn command_available(name: &str) -> bool {
    let mut cmd = Command::new(name);
    cmd.arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd.status().map(|status| status.success()).unwrap_or(false)
}

pub(crate) fn node_command_name() -> Result<&'static str, String> {
    for candidate in ["node", "node.exe"] {
        if command_available(candidate) {
            return Ok(candidate);
        }
    }
    Err("没有找到 node，请先安装 Node.js，或手动填写 QQ AppID/AppSecret".into())
}

/// 在 PATH 各目录下按文件名查找可执行文件，返回首个存在的完整路径。
/// 用于解决 Windows 下 Rust 的 Command 只识别 .exe、找不到 npm 包装脚本(.cmd)的问题。
pub(crate) fn which_in_path(file_names: &[&str]) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        for name in file_names {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// 解析 Codex CLI 的可执行文件完整路径。
///
/// 优先级:
/// 1. PATH 中的原生二进制 `codex.exe`(Rust 可直接 spawn，最稳)
/// 2. PATH 中 npm 安装的包装脚本 `codex.cmd`(Windows 下 Rust 可经 cmd 执行)
/// 3. PATH 中的无扩展名 `codex`(类 Unix 平台)
/// 4. 若 PATH 仅有包装脚本，尝试解析其内部的原生 codex.exe
pub(crate) fn resolve_codex_command() -> Result<PathBuf, String> {
    // Windows 优先原生 exe，其次 .cmd；非 Windows 直接找 codex。
    let candidates: &[&str] = if cfg!(windows) {
        &["codex.exe", "codex.cmd", "codex.bat", "codex"]
    } else {
        &["codex", "codex.exe"]
    };

    if let Some(found) = which_in_path(candidates) {
        // 找到的是 npm 包装脚本时，尝试定位其内部的原生 codex.exe 以获得更稳定的执行。
        if let Some(native) = resolve_bundled_codex_exe(&found) {
            return Ok(native);
        }
        return Ok(found);
    }

    Err(
        "没有找到 Codex CLI。请先安装 Codex（npm i -g @openai/codex），或将 codex 加入 PATH 后重试。"
            .into(),
    )
}

/// 如果传入的是 npm 包装脚本(codex / codex.cmd)，尝试在其同级 node_modules 中
/// 找到平台原生的 codex.exe，避免每次都经过 node 启动包装层。找不到则返回 None。
pub(crate) fn resolve_bundled_codex_exe(wrapper: &std::path::Path) -> Option<PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    let ext = wrapper
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    // 只有包装脚本(无扩展名或 .cmd/.bat)才需要进一步解析；本身就是 .exe 直接用。
    if matches!(ext.as_deref(), Some("exe")) {
        return None;
    }
    let base = wrapper.parent()?;
    let native = base
        .join("node_modules")
        .join("@openai")
        .join("codex")
        .join("node_modules")
        .join("@openai")
        .join("codex-win32-x64")
        .join("vendor")
        .join("x86_64-pc-windows-msvc")
        .join("bin")
        .join("codex.exe");
    if native.is_file() {
        Some(native)
    } else {
        None
    }
}

pub(crate) fn npm_command_name() -> Result<&'static str, String> {
    for candidate in ["npm.cmd", "npm", "npm.exe"] {
        if command_available(candidate) {
            return Ok(candidate);
        }
    }
    Err("没有找到 npm，请先安装 Node.js/npm，或手动填写 QQ AppID/AppSecret".into())
}

pub(crate) fn qq_qr_runner_text() -> &'static str {
    r#"import { startQrConnect } from '@tencent-connect/qqbot-connector';
import QRCode from 'qrcode';

const emit = (payload) => process.stdout.write(`${JSON.stringify(payload)}\n`);

let stop = null;
try {
  stop = startQrConnect({
    async onSuccess(credentials) {
      emit({ type: 'success', credentials });
      setTimeout(() => process.exit(0), 80);
    },
    onFailure(error) {
      emit({ type: 'failure', message: error?.message || String(error || '扫码绑定失败') });
      setTimeout(() => process.exit(1), 80);
    },
    async onQrDisplayed(url) {
      let dataUrl = '';
      const value = String(url || '').trim();
      if (!/^(https?:\/\/|mqqapi:\/\/|qqbot:\/\/)/i.test(value)) {
        emit({ type: 'failure', message: `QQ connector 返回的不是可扫码授权链接，而是内部字符串：${value.slice(0, 24)}...` });
        setTimeout(() => process.exit(1), 80);
        return;
      }
      try {
        dataUrl = await QRCode.toDataURL(value, { width: 240, margin: 1 });
      } catch (error) {
        emit({ type: 'warning', message: error?.message || String(error || '二维码生成失败') });
      }
      emit({ type: 'qr', url: value, dataUrl });
    },
    onQrExpired() {
      emit({ type: 'expired' });
    },
  }, { displayQrCodeToConsole: false });
} catch (error) {
  emit({ type: 'failure', message: error?.message || String(error || '扫码服务启动失败') });
  process.exit(1);
}

const shutdown = () => {
  try {
    if (typeof stop === 'function') stop();
  } catch {}
  process.exit(0);
};

process.on('SIGTERM', shutdown);
process.on('SIGINT', shutdown);
"#
}

pub(crate) fn qq_gateway_runner_text() -> &'static str {
    r#"import WebSocket from 'ws';
import https from 'node:https';

const emit = (payload) => process.stdout.write(`${JSON.stringify(payload)}\n`);
const appId = process.env.QQ_APP_ID || '';
const appSecret = process.env.QQ_APP_SECRET || '';
const intents = Number(process.env.QQ_INTENTS || '33554432');
let accessToken = '';
let expiresAt = 0;
let stopping = false;
let ws = null;
let heartbeatTimer = null;

function requestJson(method, url, payload, headers = {}) {
  return new Promise((resolve, reject) => {
    const body = payload == null ? '' : JSON.stringify(payload);
    const req = https.request(url, {
      method,
      headers: {
        'content-type': 'application/json; charset=utf-8',
        ...(body ? { 'content-length': Buffer.byteLength(body) } : {}),
        ...headers,
      },
    }, (res) => {
      const chunks = [];
      res.on('data', (chunk) => chunks.push(chunk));
      res.on('end', () => {
        const text = Buffer.concat(chunks).toString('utf8');
        if (res.statusCode < 200 || res.statusCode >= 300) {
          reject(new Error(`HTTP ${res.statusCode}: ${text}`));
          return;
        }
        try {
          resolve(text.trim() ? JSON.parse(text) : {});
        } catch {
          reject(new Error(`接口返回不是 JSON: ${text.slice(0, 200)}`));
        }
      });
    });
    req.on('error', reject);
    if (body) req.write(body);
    req.end();
  });
}

async function token() {
  if (accessToken && Date.now() < expiresAt - 60000) return accessToken;
  const result = await requestJson('POST', 'https://bots.qq.com/app/getAppAccessToken', {
    appId,
    clientSecret: appSecret,
  });
  accessToken = String(result.access_token || '').trim();
  if (!accessToken) throw new Error(`QQ 鉴权未返回 access_token: ${JSON.stringify(result)}`);
  expiresAt = Date.now() + Math.max(0, Number(result.expires_in || 0)) * 1000;
  return accessToken;
}

async function api(method, path, payload) {
  const t = await token();
  return requestJson(method, `https://api.sgroup.qq.com${path}`, payload, {
    authorization: `QQBot ${t}`,
  });
}

function normalizeContent(data) {
  for (const key of ['content', 'text', 'message', 'msg']) {
    let text = String(data?.[key] || '').trim();
    if (!text) continue;
    text = text.replace(/<@!?\d+>/g, '').replace(/\s+/g, ' ').trim();
    if (text) return text;
  }
  const attachments = Array.isArray(data?.attachments) ? data.attachments : [];
  return attachments.map((item, index) => {
    const url = String(item?.url || item?.file_url || item?.image_url || '').trim();
    const name = String(item?.filename || item?.name || item?.file_name || '').trim();
    return `[QQ 附件 ${index + 1}: ${[name, url].filter(Boolean).join(' ') || '未提供链接'}]`;
  }).join('\n').trim();
}

function parseMessage(payload) {
  const eventType = String(payload?.t || '');
  if (!['C2C_MESSAGE_CREATE', 'GROUP_MSG_RECEIVE', 'GROUP_AT_MESSAGE_CREATE', 'C2C_MSG_RECEIVE'].includes(eventType)) return null;
  const data = payload?.d || {};
  const content = normalizeContent(data);
  if (!content) return null;
  const author = data.author || {};
  const groupOpenid = String(data.group_openid || data.group_id || '').trim();
  const openid = String(data.openid || data.user_openid || author.user_openid || author.member_openid || author.id || '').trim();
  return {
    type: 'message',
    eventType,
    eventId: String(payload.id || data.event_id || '').trim(),
    messageId: String(data.id || data.msg_id || '').trim(),
    content,
    scene: groupOpenid || eventType.startsWith('GROUP_') ? 'group' : 'c2c',
    openid,
    groupOpenid,
  };
}

function clearHeartbeat() {
  if (heartbeatTimer) clearInterval(heartbeatTimer);
  heartbeatTimer = null;
}

async function connectOnce() {
  const gateway = await api('GET', '/gateway');
  const url = String(gateway.url || '').trim();
  if (!url) throw new Error(`QQ 网关地址为空: ${JSON.stringify(gateway)}`);
  emit({ type: 'status', message: '正在连接 QQ 网关...' });
  await new Promise((resolve, reject) => {
    ws = new WebSocket(url);
    let settled = false;
    let lastPongAt = Date.now(); // B14: 记录最近一次收到心跳 ACK 的时间
    const fail = (error) => {
      clearHeartbeat();
      if (!settled) {
        settled = true;
        reject(error instanceof Error ? error : new Error(String(error || 'QQ 网关断开')));
      }
    };
    ws.on('open', () => emit({ type: 'status', message: 'QQ 网关已连接' }));
    ws.on('message', async (data) => {
      let payload = {};
      try {
        payload = JSON.parse(Buffer.isBuffer(data) ? data.toString('utf8') : String(data));
      } catch {
        return;
      }
      if (payload.s != null) ws.nextSeq = payload.s;
      // B14: op 11 = HeartbeatACK，更新最近 pong 时间
      if (payload.op === 11) {
        lastPongAt = Date.now();
        return;
      }
      if (payload.op === 10) {
        const heartbeatMs = Number(payload?.d?.heartbeat_interval || payload?.d?.heartbeatInterval || 30000);
        const t = await token();
        ws.send(JSON.stringify({
          op: 2,
          d: {
            token: `QQBot ${t}`,
            intents,
            shard: [0, 1],
            properties: { '$os': process.platform, '$browser': 'VarSwitch', '$device': 'VarSwitch' },
          },
        }));
        clearHeartbeat();
        lastPongAt = Date.now(); // 刚连接重置计时
        heartbeatTimer = setInterval(() => {
          try {
            if (ws?.readyState === WebSocket.OPEN) {
              // B14: 连续两个心跳周期没收到 ACK 则主动断线重连
              if (Date.now() - lastPongAt > heartbeatMs * 2 + 5000) {
                fail(new Error(`QQ 心跳超时（${Math.round((Date.now() - lastPongAt) / 1000)}s 无应答）`));
                return;
              }
              ws.send(JSON.stringify({ op: 1, d: ws.nextSeq ?? null }));
            }
          } catch {}
        }, Math.max(1000, heartbeatMs));
        return;
      }
      if (payload.op === 0 && payload.t === 'READY') {
        emit({ type: 'ready' });
        return;
      }
      if (payload.op === 7 || payload.op === 9) {
        fail(new Error('QQ 网关要求重新连接'));
        return;
      }
      const message = parseMessage(payload);
      if (message) emit(message);
    });
    ws.on('error', fail);
    ws.on('close', () => fail(new Error('QQ 网关连接已关闭')));
  });
}

async function main() {
  if (!appId || !appSecret) throw new Error('缺少 QQ AppID/AppSecret');
  let reconnectDelay = 1000; // B14: 指数退避初始值1s，上限60s
  while (!stopping) {
    try {
      await connectOnce();
      reconnectDelay = 1000; // B14: 连接成功后重置退避
    } catch (error) {
      if (stopping) break;
      const msg = error?.message || String(error);
      // B14: 鉴权类错误直接 failure 退出，无需重连（避免无限用错误凭据刷接口）
      if (/401|unauthorized|access_token|invalid_client|appId|appSecret|鉴权/i.test(msg)) {
        emit({ type: 'failure', message: `QQ 鉴权失败，请重新绑定：${msg}` });
        process.exit(1);
      }
      emit({ type: 'status', message: `QQ 网关断开，${reconnectDelay / 1000}s 后重连：${msg}` });
      await new Promise((resolve) => setTimeout(resolve, reconnectDelay));
      reconnectDelay = Math.min(reconnectDelay * 2, 60000); // B14: 指数退避
    } finally {
      clearHeartbeat();
      try { ws?.close(); } catch {}
      ws = null;
    }
  }
}

const shutdown = () => {
  stopping = true;
  clearHeartbeat();
  try { ws?.close(); } catch {}
  setTimeout(() => process.exit(0), 50);
};
process.on('SIGTERM', shutdown);
process.on('SIGINT', shutdown);

main().catch((error) => {
  emit({ type: 'failure', message: error?.message || String(error || 'QQ 网关启动失败') });
  process.exit(1);
});
"#
}

pub(crate) fn lark_bridge_runner_text() -> &'static str {
    r#"import WebSocket from 'ws';
import https from 'node:https';

const emit = (payload) => process.stdout.write(`${JSON.stringify(payload)}\n`);

const appId = String(process.env.LARK_APP_ID || '').trim();
const appSecret = String(process.env.LARK_APP_SECRET || '').trim();
const baseUrl = String(process.env.LARK_BASE_URL || 'https://open.feishu.cn').replace(/\/+$/, '');
const botOpenId = String(process.env.LARK_BOT_OPEN_ID || '').trim();
const WS_METHOD_CONTROL = 0;
const WS_METHOD_DATA = 1;
const WS_EVENT_TYPE = 'event';
const WS_CARD_TYPE = 'card';
const WS_PING_TYPE = 'ping';
const WS_PONG_TYPE = 'pong';
let stopping = false;
let ws = null;
let pingTimer = null;
let pingIntervalMs = 30000;
let serviceId = 0;
const chunkCache = new Map();

if (!appId || !appSecret) {
  emit({ type: 'failure', message: '缺少飞书 AppID/AppSecret' });
  process.exit(1);
}

function requestJson(method, url, payload, headers = {}) {
  return new Promise((resolve, reject) => {
    const body = payload == null ? '' : JSON.stringify(payload);
    const req = https.request(url, {
      method,
      headers: {
        'content-type': 'application/json; charset=utf-8',
        'user-agent': 'VarSwitch hi-codex-compatible',
        ...(body ? { 'content-length': Buffer.byteLength(body) } : {}),
        ...headers,
      },
    }, (res) => {
      const chunks = [];
      res.on('data', (chunk) => chunks.push(chunk));
      res.on('end', () => {
        const text = Buffer.concat(chunks).toString('utf8');
        if (res.statusCode < 200 || res.statusCode >= 300) {
          reject(new Error(`HTTP ${res.statusCode}: ${text}`));
          return;
        }
        try {
          resolve(text.trim() ? JSON.parse(text) : {});
        } catch {
          reject(new Error(`接口返回不是 JSON: ${text.slice(0, 200)}`));
        }
      });
    });
    req.on('error', reject);
    if (body) req.write(body);
    req.end();
  });
}

function resultOk(result) {
  return result?.code == null || result.code === 0 || result.code === '0';
}

function apiError(result, fallback = '接口返回异常') {
  const message = String(result?.msg || result?.message || fallback).trim();
  return result?.code == null || result.code === 0 || result.code === '0'
    ? message
    : `code=${result.code} ${message}`.trim();
}

async function pullWsConfig() {
  const result = await requestJson('POST', `${baseUrl}/callback/ws/endpoint`, {
    AppID: appId,
    AppSecret: appSecret,
  });
  if (!resultOk(result)) throw new Error(`飞书 WebSocket 网关获取失败：${apiError(result)}`);
  const data = result?.data || {};
  const url = String(data.URL || data.url || '').trim();
  if (!url) throw new Error(`飞书 WebSocket 网关地址为空：${JSON.stringify(result)}`);
  const parsed = new URL(url);
  serviceId = Number(parsed.searchParams.get('service_id') || parsed.searchParams.get('ServiceID') || 0);
  const clientConfig = data.ClientConfig || data.clientConfig || {};
  const seconds = Number(clientConfig.PingInterval || clientConfig.pingInterval || 30);
  pingIntervalMs = Math.max(1000, seconds * 1000);
  return url;
}

function readVarint(buffer, start) {
  let value = 0;
  let shift = 0;
  let pos = start;
  while (pos < buffer.length) {
    const byte = buffer[pos++];
    value += (byte & 0x7f) * Math.pow(2, shift);
    if ((byte & 0x80) === 0) return [value, pos];
    shift += 7;
    if (shift > 70) break;
  }
  throw new Error('invalid protobuf varint');
}

function writeVarint(value) {
  let next = Number(value || 0);
  const out = [];
  while (true) {
    let byte = next & 0x7f;
    next = Math.floor(next / 128);
    if (next) out.push(byte | 0x80);
    else {
      out.push(byte);
      break;
    }
  }
  return Buffer.from(out);
}

function readBytes(buffer, pos) {
  const [length, afterLength] = readVarint(buffer, pos);
  const end = afterLength + length;
  if (end > buffer.length) throw new Error('invalid protobuf length');
  return [buffer.subarray(afterLength, end), end];
}

function skipValue(buffer, pos, wireType) {
  if (wireType === 0) return readVarint(buffer, pos)[1];
  if (wireType === 1) return Math.min(buffer.length, pos + 8);
  if (wireType === 2) return readBytes(buffer, pos)[1];
  if (wireType === 5) return Math.min(buffer.length, pos + 4);
  throw new Error(`unsupported protobuf wire type: ${wireType}`);
}

function protoBytes(fieldId, value) {
  const raw = Buffer.isBuffer(value) ? value : Buffer.from(value || '');
  return Buffer.concat([writeVarint((fieldId << 3) | 2), writeVarint(raw.length), raw]);
}

function protoString(fieldId, value) {
  return protoBytes(fieldId, Buffer.from(String(value || ''), 'utf8'));
}

function protoVarint(fieldId, value) {
  return Buffer.concat([writeVarint((fieldId << 3) | 0), writeVarint(value)]);
}

function encodeHeader(key, value) {
  return Buffer.concat([protoString(1, key), protoString(2, value)]);
}

function decodeHeader(buffer) {
  let pos = 0;
  let key = '';
  let value = '';
  while (pos < buffer.length) {
    const [tag, afterTag] = readVarint(buffer, pos);
    pos = afterTag;
    const fieldId = tag >> 3;
    const wireType = tag & 7;
    if (fieldId === 1 && wireType === 2) {
      const [raw, next] = readBytes(buffer, pos);
      key = raw.toString('utf8');
      pos = next;
    } else if (fieldId === 2 && wireType === 2) {
      const [raw, next] = readBytes(buffer, pos);
      value = raw.toString('utf8');
      pos = next;
    } else {
      pos = skipValue(buffer, pos, wireType);
    }
  }
  return [key, value];
}

function encodeFrame(frame) {
  const parts = [
    protoVarint(1, frame.seqId || 0),
    protoVarint(2, frame.logId || 0),
    protoVarint(3, frame.service || 0),
    protoVarint(4, frame.method || 0),
  ];
  for (const [key, value] of frame.headers || []) {
    parts.push(protoBytes(5, encodeHeader(key, value)));
  }
  if (frame.payloadEncoding) parts.push(protoString(6, frame.payloadEncoding));
  if (frame.payloadType) parts.push(protoString(7, frame.payloadType));
  if (frame.payload?.length) parts.push(protoBytes(8, frame.payload));
  if (frame.logIdNew) parts.push(protoString(9, frame.logIdNew));
  return Buffer.concat(parts);
}

function decodeFrame(input) {
  const buffer = Buffer.isBuffer(input) ? input : Buffer.from(input);
  const frame = { seqId: 0, logId: 0, service: 0, method: 0, headers: [], payloadEncoding: '', payloadType: '', payload: Buffer.alloc(0), logIdNew: '' };
  let pos = 0;
  while (pos < buffer.length) {
    const [tag, afterTag] = readVarint(buffer, pos);
    pos = afterTag;
    const fieldId = tag >> 3;
    const wireType = tag & 7;
    if (fieldId === 1 && wireType === 0) [frame.seqId, pos] = readVarint(buffer, pos);
    else if (fieldId === 2 && wireType === 0) [frame.logId, pos] = readVarint(buffer, pos);
    else if (fieldId === 3 && wireType === 0) [frame.service, pos] = readVarint(buffer, pos);
    else if (fieldId === 4 && wireType === 0) [frame.method, pos] = readVarint(buffer, pos);
    else if (fieldId === 5 && wireType === 2) {
      const [raw, next] = readBytes(buffer, pos);
      frame.headers.push(decodeHeader(raw));
      pos = next;
    } else if (fieldId === 6 && wireType === 2) {
      const [raw, next] = readBytes(buffer, pos);
      frame.payloadEncoding = raw.toString('utf8');
      pos = next;
    } else if (fieldId === 7 && wireType === 2) {
      const [raw, next] = readBytes(buffer, pos);
      frame.payloadType = raw.toString('utf8');
      pos = next;
    } else if (fieldId === 8 && wireType === 2) {
      const [raw, next] = readBytes(buffer, pos);
      frame.payload = raw;
      pos = next;
    } else if (fieldId === 9 && wireType === 2) {
      const [raw, next] = readBytes(buffer, pos);
      frame.logIdNew = raw.toString('utf8');
      pos = next;
    } else {
      pos = skipValue(buffer, pos, wireType);
    }
  }
  return frame;
}

function headersObject(frame) {
  return Object.fromEntries((frame.headers || []).map(([key, value]) => [String(key), String(value)]));
}

function safeJson(value) {
  if (value && typeof value === 'object') return value;
  const text = String(value || '').trim();
  if (!text) return {};
  try {
    return JSON.parse(text);
  } catch {
    return {};
  }
}

function stripAtTags(text) {
  return String(text || '')
    .replace(/<at\s+[^>]*>(.*?)<\/at>/g, (_, name) => `@${String(name || '').trim()}`)
    .replace(/\s+/g, ' ')
    .trim();
}

function collectCardText(value, out = []) {
  if (Array.isArray(value)) {
    for (const item of value) collectCardText(item, out);
  } else if (value && typeof value === 'object') {
    for (const key of ['text', 'content', 'title', 'subtitle', 'alt', 'value']) {
      if (typeof value[key] === 'string' && value[key].trim()) out.push(value[key].trim());
    }
    for (const item of Object.values(value)) collectCardText(item, out);
  }
  return out;
}

function normalizeContent(message) {
  const type = String(message?.message_type || message?.msg_type || '').toLowerCase();
  const parsed = safeJson(message?.content || message?.body || {});
  if (type === 'text') {
    const text = typeof parsed.text === 'string' ? parsed.text : String(message?.text || message?.content || '');
    return stripAtTags(text);
  }
  if (type === 'post') {
    const body = parsed.zh_cn || parsed.en_us || parsed.ja_jp || parsed;
    const parts = [];
    if (body?.title) parts.push(String(body.title).trim());
    for (const paragraph of body?.content || []) {
      if (!Array.isArray(paragraph)) continue;
      const line = paragraph.map((item) => String(item?.text || item?.href || '')).join('').trim();
      if (line) parts.push(line);
    }
    return parts.join('\n').trim();
  }
  if (type === 'interactive') return collectCardText(parsed).slice(0, 8).join('\n').trim() || '[飞书 卡片消息]';
  if (['img', 'image'].includes(type)) return `[飞书 图片: ${parsed.image_key || parsed.file_key || '未提供 file_key'}]`;
  if (['media', 'video', 'audio', 'file'].includes(type)) return `[飞书 文件: ${parsed.file_name || parsed.name || parsed.file_key || parsed.image_key || '未提供 file_key'}]`;
  if (typeof parsed.text === 'string') return stripAtTags(parsed.text);
  return stripAtTags(message?.content || '');
}

function parseEventPayload(data) {
  if (data?.schema) {
    const header = data.header || {};
    return [String(header.event_type || data.event_type || '').trim(), { ...(data.event || {}), app_id: header.app_id, tenant_key: header.tenant_key, event_id: header.event_id }];
  }
  if (data?.event && typeof data.event === 'object') {
    return [String(data.event.type || data.event_type || data.type || '').trim(), { ...data.event, app_id: data.app_id ?? data.event.app_id, tenant_key: data.tenant_key ?? data.event.tenant_key, event_id: data.event_id ?? data.event.event_id }];
  }
  return [String(data?.event_type || data?.type || '').trim(), data || {}];
}

function parseIncomingMessage(payload) {
  const [eventType, event] = parseEventPayload(payload);
  if (eventType && eventType !== 'im.message.receive_v1') return null;
  if (event?.app_id && String(event.app_id).trim() !== appId) return null;
  const senderId = event?.sender?.sender_id || {};
  const senderOpenId = String(senderId.open_id || senderId.user_id || '').trim();
  if (botOpenId && senderOpenId && senderOpenId === botOpenId) return null;
  const message = event?.message || {};
  const text = normalizeContent(message);
  const messageId = String(message.message_id || '').trim();
  const chatId = String(message.chat_id || '').trim();
  if (!messageId || !text) return null;
  return {
    type: 'message',
    eventId: String(event.event_id || messageId).trim(),
    messageId,
    chatId,
    senderId: senderOpenId,
    chatType: String(message.chat_type || '').trim(),
    threadId: String(message.thread_id || message.root_id || '').trim(),
    rootId: String(message.root_id || '').trim(),
    text,
  };
}

function mergeFramePayload(frame) {
  const headers = headersObject(frame);
  const messageId = headers.message_id || headers.trace_id || String(frame.seqId || '');
  const total = Math.max(1, Number(headers.sum || 1));
  const seq = Number(headers.seq || 0);
  if (total <= 1) return safeJson(frame.payload.toString('utf8'));
  const index = seq >= 0 && seq < total ? seq : Math.max(0, Math.min(total - 1, seq - 1));
  const cache = chunkCache.get(messageId) || { chunks: Array(total).fill(null), created: Date.now() };
  cache.chunks[index] = frame.payload;
  chunkCache.set(messageId, cache);
  if (cache.chunks.every(Boolean)) {
    chunkCache.delete(messageId);
    return safeJson(Buffer.concat(cache.chunks).toString('utf8'));
  }
  for (const [key, value] of chunkCache.entries()) {
    if (Date.now() - value.created > 20000) chunkCache.delete(key);
  }
  return null;
}

function sendFrame(frame) {
  if (ws?.readyState === WebSocket.OPEN) ws.send(encodeFrame(frame));
}

function sendPing() {
  sendFrame({ seqId: 0, logId: 0, service: serviceId, method: WS_METHOD_CONTROL, headers: [['type', WS_PING_TYPE]] });
}

function sendAck(frame, code = 200) {
  sendFrame({
    seqId: frame.seqId,
    logId: frame.logId,
    service: frame.service,
    method: frame.method,
    headers: [...(frame.headers || []), ['biz_rt', '0']],
    payloadEncoding: frame.payloadEncoding,
    payloadType: frame.payloadType,
    payload: Buffer.from(JSON.stringify({ code }), 'utf8'),
    logIdNew: frame.logIdNew,
  });
}

function resetPingTimer() {
  if (pingTimer) clearInterval(pingTimer);
  pingTimer = setInterval(sendPing, Math.max(1000, pingIntervalMs));
}

function handleFrame(frame) {
  const headers = headersObject(frame);
  const frameType = headers.type || '';
  if (frame.method === WS_METHOD_CONTROL) {
    if (frameType === WS_PONG_TYPE && frame.payload?.length) {
      const data = safeJson(frame.payload.toString('utf8'));
      if (data.PingInterval) {
        pingIntervalMs = Math.max(1000, Number(data.PingInterval) * 1000);
        resetPingTimer();
      }
    }
    return;
  }
  if (frame.method !== WS_METHOD_DATA || ![WS_EVENT_TYPE, WS_CARD_TYPE].includes(frameType)) return;
  const payload = mergeFramePayload(frame);
  if (!payload) return;
  let code = 200;
  try {
    const message = parseIncomingMessage(payload);
    if (message) emit(message);
  } catch (error) {
    code = 500;
    emit({ type: 'status', message: `飞书消息解析失败：${error?.message || String(error)}` });
  } finally {
    sendAck(frame, code);
  }
}

async function connectOnce() {
  const url = await pullWsConfig();
  emit({ type: 'status', message: '正在连接飞书 WebSocket...' });
  await new Promise((resolve, reject) => {
    let settled = false;
    ws = new WebSocket(url);
    const fail = (error) => {
      if (pingTimer) clearInterval(pingTimer);
      pingTimer = null;
      if (settled) return;
      settled = true;
      if (stopping) resolve();
      else reject(error instanceof Error ? error : new Error(String(error || '飞书 WebSocket 已断开')));
    };
    ws.on('open', () => {
      emit({ type: 'ready' });
      sendPing();
      resetPingTimer();
    });
    ws.on('message', (data) => {
      try {
        handleFrame(decodeFrame(Buffer.isBuffer(data) ? data : Buffer.from(data)));
      } catch (error) {
        emit({ type: 'status', message: `飞书 WebSocket 帧处理失败：${error?.message || String(error)}` });
      }
    });
    ws.on('error', fail);
    ws.on('close', () => fail(new Error('飞书 WebSocket 已关闭')));
  });
}

async function main() {
  let reconnectDelay = 1000; // B14: 指数退避初始值1s，上限60s
  while (!stopping) {
    try {
      await connectOnce();
      reconnectDelay = 1000; // B14: 连接成功后重置退避
    } catch (error) {
      if (stopping) break;
      const msg = error?.message || String(error);
      // B14: 飞书鉴权类错误（token无效/AppID错误）直接 failure 退出，不重连
      if (/99991663|99991672|token.*invalid|invalid.*token|AppID.*不|unauthorized|10003/i.test(msg)) {
        emit({ type: 'failure', message: `飞书鉴权失败，请重新绑定：${msg}` });
        process.exit(1);
      }
      emit({ type: 'status', message: `飞书 WebSocket 异常，${reconnectDelay / 1000}s 后重连：${msg}` });
      await new Promise((resolve) => setTimeout(resolve, reconnectDelay));
      reconnectDelay = Math.min(reconnectDelay * 2, 60000); // B14: 指数退避
    } finally {
      if (pingTimer) clearInterval(pingTimer);
      pingTimer = null;
      try { ws?.close(); } catch {}
      ws = null;
    }
  }
}

const shutdown = () => {
  stopping = true;
  if (pingTimer) clearInterval(pingTimer);
  try { ws?.close(); } catch {}
  setTimeout(() => process.exit(0), 50);
};

process.on('SIGTERM', shutdown);
process.on('SIGINT', shutdown);

main().catch((error) => {
  emit({ type: 'failure', message: error?.message || String(error || '飞书消息桥启动失败') });
  process.exit(1);
});
"#
}

pub(crate) fn ensure_lark_bridge_connector(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let connector_dir = data_dir(app).join("larkbot-connector");
    fs::create_dir_all(&connector_dir).map_err(|e| e.to_string())?;
    let package_path = connector_dir.join("package.json");
    let runner_path = connector_dir.join("lark-bridge-runner.mjs");
    let package_text = serde_json::to_string_pretty(&serde_json::json!({
        "private": true,
        "type": "module",
        "dependencies": {
            "ws": "latest"
        }
    }))
    .map_err(|e| e.to_string())?;
    fs::write(&package_path, package_text).map_err(|e| e.to_string())?;
    fs::write(&runner_path, lark_bridge_runner_text()).map_err(|e| e.to_string())?;

    let ws_module = connector_dir.join("node_modules").join("ws");
    if !ws_module.exists() {
        let npm = npm_command_name()?;
        let mut cmd = Command::new(npm);
        cmd.args([
            "install",
            "--omit=dev",
            "--no-audit",
            "--no-fund",
            "--registry=https://registry.npmmirror.com",
        ])
        .current_dir(&connector_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
        #[cfg(target_os = "windows")]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let output = cmd
            .output()
            .map_err(|e| format!("启动 npm 安装飞书 WebSocket 依赖失败: {e}"))?;
        if !output.status.success() {
            let mut detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if detail.is_empty() {
                detail = String::from_utf8_lossy(&output.stdout).trim().to_string();
            }
            return Err(format!("安装飞书 WebSocket 依赖失败: {detail}"));
        }
    }
    Ok(runner_path)
}

pub(crate) fn ensure_qq_qr_connector(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let connector_dir = data_dir(app).join("qqbot-connector");
    fs::create_dir_all(&connector_dir).map_err(|e| e.to_string())?;
    let package_path = connector_dir.join("package.json");
    let runner_path = connector_dir.join("qqbot-qr-runner.mjs");
    let package_text = serde_json::to_string_pretty(&serde_json::json!({
        "private": true,
        "type": "module",
        "dependencies": {
            "@tencent-connect/qqbot-connector": "latest",
            "qrcode": "latest",
            "ws": "latest"
        }
    }))
    .map_err(|e| e.to_string())?;
    fs::write(&package_path, package_text).map_err(|e| e.to_string())?;
    fs::write(&runner_path, qq_qr_runner_text()).map_err(|e| e.to_string())?;

    let connector_module = connector_dir
        .join("node_modules")
        .join("@tencent-connect")
        .join("qqbot-connector");
    let qrcode_module = connector_dir.join("node_modules").join("qrcode");
    let ws_module = connector_dir.join("node_modules").join("ws");
    if !connector_module.exists() || !qrcode_module.exists() || !ws_module.exists() {
        let npm = npm_command_name()?;
        let mut cmd = Command::new(npm);
        cmd.args([
            "install",
            "--omit=dev",
            "--no-audit",
            "--no-fund",
            "--registry=https://registry.npmmirror.com",
        ])
        .current_dir(&connector_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
        #[cfg(target_os = "windows")]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let output = cmd
            .output()
            .map_err(|e| format!("启动 npm 安装 QQ 扫码依赖失败: {e}"))?;
        if !output.status.success() {
            let mut detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if detail.is_empty() {
                detail = String::from_utf8_lossy(&output.stdout).trim().to_string();
            }
            return Err(format!("安装 QQ 扫码依赖失败: {detail}"));
        }
    }
    Ok(runner_path)
}

pub(crate) fn ensure_qq_gateway_connector(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let connector_dir = data_dir(app).join("qqbot-connector");
    let _ = ensure_qq_qr_connector(app)?;
    let runner_path = connector_dir.join("qqbot-gateway-runner.mjs");
    fs::write(&runner_path, qq_gateway_runner_text()).map_err(|e| e.to_string())?;
    Ok(runner_path)
}

pub(crate) fn write_mobile_channel_qr_state(
    app: &tauri::AppHandle,
    channel: &str,
    status: &str,
    qr_url: &str,
    qr_data_url: &str,
    error: &str,
) -> Result<ToolboxSnapshot, String> {
    let normalized = normalize_mobile_channel(channel);
    let mut state = read_toolbox_state(app);
    if !state
        .mobile_channels
        .iter()
        .any(|binding| binding.channel == normalized)
    {
        state
            .mobile_channels
            .push(default_mobile_channel(&normalized));
    }
    if let Some(binding) = state
        .mobile_channels
        .iter_mut()
        .find(|binding| binding.channel == normalized)
    {
        let now = chrono_now();
        let now_ms = chrono_timestamp_millis().to_string();
        binding.qr_status = status.to_string();
        binding.qr_url = qr_url.to_string();
        binding.qr_data_url = qr_data_url.to_string();
        if qr_url.trim().is_empty() && qr_data_url.trim().is_empty() {
            binding.qr_device_code.clear();
        }
        binding.last_error = error.to_string();
        if !status.trim().is_empty()
            && (!qr_url.trim().is_empty()
                || !qr_data_url.trim().is_empty()
                || binding.qr_started_at.trim().is_empty())
        {
            binding.qr_started_at = now_ms;
        }
        binding.updated_at = now;
    }
    write_toolbox_state(app, &state)?;
    Ok(build_toolbox_snapshot(app))
}

pub(crate) fn selected_thread_for_mobile_state(state: &ToolboxState) -> Option<CodexThreadRecord> {
    if state.synced_codex_threads.is_empty() {
        return None;
    }
    let selected = state.selected_mobile_thread_id.trim();
    if !selected.is_empty() {
        if let Some(thread) = state
            .synced_codex_threads
            .iter()
            .find(|thread| thread.id == selected)
        {
            return Some(thread.clone());
        }
    }
    state.synced_codex_threads.first().cloned()
}

pub(crate) fn attach_selected_thread_to_mobile_channel(state: &mut ToolboxState, channel: &str) {
    let normalized = normalize_mobile_channel(channel);
    let Some(thread) = selected_thread_for_mobile_state(state) else {
        return;
    };

    state
        .session_bindings
        .retain(|binding| normalize_mobile_channel(&binding.channel) != normalized);
    state.session_bindings.push(CodexSessionBinding {
        channel: normalized.clone(),
        thread_id: thread.id.clone(),
        thread_name: thread.thread_name.clone(),
        session_file: thread.session_file.clone(),
        updated_at: thread.updated_at.clone(),
        cwd: thread.cwd.clone(),
        sync_enabled: true,
        last_synced_at: String::new(),
        note: "mobile-control".into(),
    });
    state.selected_mobile_thread_id = thread.id.clone();
    state.mobile_remote.active_thread_id = thread.id.clone();
    state.mobile_remote.active_thread_name = thread.thread_name.clone();

    if let Some(binding) = state
        .mobile_channels
        .iter_mut()
        .find(|binding| binding.channel == normalized)
    {
        binding.thread_id = thread.id;
        binding.thread_name = thread.thread_name;
        binding.session_file = thread.session_file;
        binding.enabled = true;
        binding.status = "已绑定，等待开启平台连接".into();
        binding.updated_at = chrono_now();
    }
}

#[derive(Clone, Debug)]
pub(crate) struct LarkRegistrationPollResult {
    pub(crate) status: String,
    pub(crate) client_id: String,
    pub(crate) client_secret: String,
    pub(crate) tenant_brand: String,
    pub(crate) message: String,
    pub(crate) interval_secs: u64,
}

pub(crate) fn post_lark_registration_form(
    client: &reqwest::blocking::Client,
    params: &[(&str, &str)],
) -> Result<serde_json::Value, String> {
    let response = client
        .post(lark_registration_endpoint())
        .header("locale", "zh")
        .form(params)
        .send()
        .map_err(|e| format!("飞书机器人注册请求失败: {e}"))?;
    let status = response.status();
    let body = response
        .text()
        .map_err(|e| format!("飞书机器人注册响应读取失败: {e}"))?;
    if !status.is_success() && body.trim().is_empty() {
        return Err(format!("飞书机器人注册返回 HTTP {}", status.as_u16()));
    }
    serde_json::from_str(&body).map_err(|e| format!("飞书机器人注册返回不是 JSON: {e}; {body}"))
}

pub(crate) fn request_lark_registration(
    client: &reqwest::blocking::Client,
    create_only: bool,
    app_id: &str,
) -> Result<(String, String, u64), String> {
    let result = post_lark_registration_form(
        client,
        &[
            ("action", "begin"),
            ("archetype", "PersonalAgent"),
            ("auth_method", "client_secret"),
            ("request_user_info", "open_id"),
        ],
    )?;
    let device_code = json_string(&result, &["device_code"]);
    let verification_url = json_string(
        &result,
        &[
            "verification_uri_complete",
            "verification_uri",
            "qrcode_url",
        ],
    );
    if device_code.is_empty() || verification_url.is_empty() {
        return Err(format!("飞书注册接口未返回授权信息：{result}"));
    }
    let mut params = vec![
        ("from", "sdk"),
        ("source", "node-sdk/varswitch"),
        ("tp", "sdk"),
        ("addons", lark_registration_addons()),
        ("name", "VarSwitch 智能体"),
        (
            "desc",
            "把飞书消息转发给 Codex，并把 Codex 回复同步回飞书。",
        ),
    ];
    if create_only {
        params.push(("createOnly", "true"));
    }
    if !app_id.trim().is_empty() {
        params.push(("clientID", app_id.trim()));
    }
    let launcher_url = append_query_params(&verification_url, &params);
    let interval = result
        .get("interval")
        .and_then(|value| value.as_u64())
        .unwrap_or(5)
        .clamp(2, 10);
    Ok((device_code, launcher_url, interval))
}

pub(crate) fn poll_lark_registration_device(
    client: &reqwest::blocking::Client,
    device_code: &str,
) -> Result<LarkRegistrationPollResult, String> {
    let result = post_lark_registration_form(
        client,
        &[("action", "poll"), ("device_code", device_code.trim())],
    )?;
    let user_info = result
        .get("user_info")
        .and_then(|value| value.as_object())
        .cloned()
        .unwrap_or_default();
    let tenant_brand = user_info
        .get("tenant_brand")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let client_id = json_string(&result, &["client_id"]);
    let client_secret = json_string(&result, &["client_secret"]);
    if !client_id.is_empty() && !client_secret.is_empty() {
        return Ok(LarkRegistrationPollResult {
            status: "confirmed".into(),
            client_id,
            client_secret,
            tenant_brand,
            message: "飞书机器人创建完成，AppID/AppSecret 已自动填充".into(),
            interval_secs: 0,
        });
    }

    let error = json_string(&result, &["error"]);
    let description = json_string(&result, &["error_description", "message", "msg"]);
    let (status, message, interval_secs) = match error.as_str() {
        "" | "authorization_pending" => ("pending", "等待飞书创建/授权确认", 5),
        "slow_down" => ("slow_down", "飞书要求降低轮询频率，稍后继续检查", 8),
        "access_denied" => ("access_denied", "飞书授权被取消", 0),
        "expired_token" => ("expired_token", "飞书创建授权已过期，请重新创建", 0),
        _ => ("error", description.as_str(), 0),
    };
    Ok(LarkRegistrationPollResult {
        status: status.into(),
        client_id: String::new(),
        client_secret: String::new(),
        tenant_brand,
        message: if message.is_empty() {
            "等待飞书创建/授权确认".into()
        } else {
            message.into()
        },
        interval_secs,
    })
}

pub(crate) fn apply_lark_registration_poll(
    app: &tauri::AppHandle,
    poll: LarkRegistrationPollResult,
) -> Result<ToolboxSnapshot, String> {
    let mut state = read_toolbox_state(app);
    if let Some(binding) = state
        .mobile_channels
        .iter_mut()
        .find(|binding| binding.channel == "lark")
    {
        binding.qr_status = poll.message.clone();
        binding.last_error.clear();
        binding.updated_at = chrono_now();
        if poll.status == "confirmed" {
            binding.app_id = poll.client_id;
            binding.app_secret = poll.client_secret;
            binding.base_url = if poll.tenant_brand == "lark" {
                "https://open.larksuite.com".into()
            } else {
                "https://open.feishu.cn".into()
            };
            binding.qr_device_code.clear();
            binding.qr_url.clear();
            binding.qr_data_url.clear();
            binding.credential_status = "飞书 AppID/AppSecret 已自动保存".into();
        } else if matches!(
            poll.status.as_str(),
            "access_denied" | "expired_token" | "error"
        ) {
            binding.last_error = poll.message;
        }
    }
    if poll.status == "confirmed" {
        attach_selected_thread_to_mobile_channel(&mut state, "lark");
    }
    write_toolbox_state(app, &state)?;
    Ok(build_toolbox_snapshot(app))
}

pub(crate) fn start_lark_registration_poll_worker(
    app: tauri::AppHandle,
    device_code: String,
    initial_interval_secs: u64,
    generation: u64,
) {
    std::thread::spawn(move || {
        let client = match build_http_client(15) {
            Ok(client) => client,
            Err(error) => {
                update_channel_status(&app, "lark", "飞书注册轮询失败", &error);
                LARK_REGISTRATION_ACTIVE.store(false, Ordering::SeqCst);
                return;
            }
        };
        let mut interval = initial_interval_secs.clamp(2, 10);
        let mut consecutive_errors = 0u32;
        for _ in 0..120 {
            std::thread::sleep(Duration::from_secs(interval));
            // B13: 代际检查，新注册流程已启动时旧 worker 应退出，不覆盖新凭据
            if LARK_REGISTRATION_GENERATION.load(Ordering::SeqCst) != generation {
                log_info!("[mobile-control][lark-reg] generation changed, worker exiting");
                return;
            }
            let poll = match poll_lark_registration_device(&client, &device_code) {
                Ok(poll) => {
                    consecutive_errors = 0; // 成功则重置连续错误计数
                    poll
                }
                Err(error) => {
                    consecutive_errors += 1;
                    // 已被新注册流程取代的旧 worker 不再回写错误状态，避免覆盖新流程的 UI 提示
                    if LARK_REGISTRATION_GENERATION.load(Ordering::SeqCst) != generation {
                        log_info!(
                            "[mobile-control][lark-reg] generation changed during error, worker exiting"
                        );
                        return;
                    }
                    update_channel_status(&app, "lark", "飞书注册轮询失败", &error);
                    // B13: 最多容忍 3 次连续网络错误，避免瞬时抖动中断整个注册流程
                    if consecutive_errors >= 3 {
                        log_info!(
                            "[mobile-control][lark-reg] 3 consecutive errors, worker exiting"
                        );
                        LARK_REGISTRATION_ACTIVE.store(false, Ordering::SeqCst);
                        return;
                    }
                    continue;
                }
            };
            // 轮询请求可能持续数秒；请求返回后再次检查，避免清除绑定后旧 worker 回写凭据。
            if LARK_REGISTRATION_GENERATION.load(Ordering::SeqCst) != generation {
                log_info!(
                    "[mobile-control][lark-reg] generation changed after poll, worker exiting"
                );
                return;
            }
            interval = poll.interval_secs.clamp(2, 10);
            let final_status = matches!(
                poll.status.as_str(),
                "confirmed" | "access_denied" | "expired_token" | "error"
            );
            let _ = apply_lark_registration_poll(&app, poll);
            if final_status {
                LARK_REGISTRATION_ACTIVE.store(false, Ordering::SeqCst);
                return;
            }
        }
        update_channel_status(
            &app,
            "lark",
            "飞书注册轮询超时",
            "长时间没有拿到 AppID/AppSecret，请重新创建飞书机器人",
        );
        LARK_REGISTRATION_ACTIVE.store(false, Ordering::SeqCst);
    });
}

/// B9: 检查并记录消息ID，返回true=重复消息应忽略，false=新消息
pub(crate) fn check_and_record_msg_id(
    store: &OnceLock<Mutex<std::collections::VecDeque<String>>>,
    id: &str,
) -> bool {
    if id.is_empty() {
        return false;
    }
    let deque = store.get_or_init(|| Mutex::new(std::collections::VecDeque::new()));
    if let Ok(mut guard) = deque.lock() {
        if guard.iter().any(|s| s.as_str() == id) {
            return true;
        }
        guard.push_back(id.to_string());
        if guard.len() > 512 {
            guard.pop_front();
        }
    }
    false
}

pub(crate) fn json_string<'a>(value: &'a serde_json::Value, keys: &[&str]) -> String {
    for key in keys {
        if let Some(text) = value.get(*key).and_then(|item| item.as_str()) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }
    String::new()
}

pub(crate) fn split_platform_reply_text(content: &str, chunk_size: usize, max_parts: usize) -> Vec<String> {
    let mut text = content.trim().to_string();
    if text.is_empty() {
        text = "Codex 没有返回可发送的文本。".into();
    }
    let mut parts = Vec::new();
    while !text.is_empty() && parts.len() < max_parts {
        let char_count = text.chars().count();
        if char_count <= chunk_size {
            parts.push(text.trim().to_string());
            text.clear();
            break;
        }
        let mut split_at = 0usize;
        for (idx, (byte_index, ch)) in text.char_indices().enumerate() {
            if idx >= chunk_size {
                break;
            }
            if ch == '\n' || ch == '。' || ch == '.' {
                split_at = byte_index + ch.len_utf8();
            }
        }
        if split_at < chunk_size / 2 {
            split_at = text
                .char_indices()
                .nth(chunk_size)
                .map(|(idx, _)| idx)
                .unwrap_or_else(|| text.len());
        }
        let head = text[..split_at].trim().to_string();
        if !head.is_empty() {
            parts.push(head);
        }
        text = text[split_at..].trim().to_string();
    }
    if !text.is_empty() && !parts.is_empty() {
        let suffix = "\n\n[回复过长，已截断]";
        let mut last = parts.pop().unwrap_or_default();
        while last.chars().count() + suffix.chars().count() > chunk_size {
            last.pop();
        }
        parts.push(format!("{}{}", last.trim_end(), suffix));
    }
    if parts.is_empty() {
        vec!["Codex 没有返回可发送的文本。".into()]
    } else {
        parts
    }
}

pub(crate) fn update_mobile_channel_credentials_from_qr(
    app: &tauri::AppHandle,
    channel: &str,
    app_id: &str,
    app_secret: &str,
    bot_token: &str,
    account_id: &str,
    base_url: &str,
    user_id: &str,
) -> Result<ToolboxSnapshot, String> {
    let normalized = normalize_mobile_channel(channel);
    let mut state = read_toolbox_state(app);
    {
        let Some(binding) = state
            .mobile_channels
            .iter_mut()
            .find(|binding| binding.channel == normalized)
        else {
            write_toolbox_state(app, &state)?;
            return Ok(build_toolbox_snapshot(app));
        };
        if !app_id.trim().is_empty() {
            binding.app_id = app_id.trim().to_string();
        }
        // B5: build_toolbox_snapshot 把 secret 掩码成 "*"，防止该掩码值被当成真凭据回写
        if !app_secret.trim().is_empty() && app_secret.trim() != "*" {
            binding.app_secret = app_secret.trim().to_string();
        }
        if !bot_token.trim().is_empty() && bot_token.trim() != "*" {
            binding.bot_token = bot_token.trim().to_string();
        }
        if !account_id.trim().is_empty() {
            binding.account_id = account_id.trim().to_string();
        }
        if !base_url.trim().is_empty() {
            binding.base_url = normalize_mobile_base_url(base_url, &normalized);
        }
        if !user_id.trim().is_empty() {
            binding.user_id = user_id.trim().to_string();
        }
        binding.qr_status = "绑定成功".into();
        binding.qr_url.clear();
        binding.qr_data_url.clear();
        binding.qr_device_code.clear();
        binding.qr_started_at.clear();
        binding.credential_status = "平台凭据已保存".into();
        binding.last_error.clear();
        binding.updated_at = chrono_now();
    }
    attach_selected_thread_to_mobile_channel(&mut state, &normalized);
    write_toolbox_state(app, &state)?;
    Ok(build_toolbox_snapshot(app))
}

/// 用后台 Codex CLI `exec resume <thread_id>` 续接指定会话并返回最终回复。
/// 作为 App 注入不可用时的兜底路径（CLI 与桌面 App 共享 ~/.codex 会话存储）。
pub(crate) fn run_codex_cli_reply(prompt: &str, cwd: &str, thread_id: &str) -> Result<String, String> {
    let text = prompt.trim();
    if text.is_empty() {
        return Err("消息内容为空".into());
    }
    let executable = resolve_codex_command()?;
    let output_path = std::env::temp_dir().join(format!(
        "varswitch-codex-reply-{}-{}.txt",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let workdir = if cwd.trim().is_empty() {
        std::env::current_dir().unwrap_or_else(|_| home_dir())
    } else {
        PathBuf::from(cwd)
    };
    let mut command = codex_command(&executable);
    command.args([
        "--ask-for-approval",
        "never",
        "exec",
        "--sandbox",
        "workspace-write",
        "--color",
        "never",
        "--output-last-message",
    ]);
    command.arg(&output_path);
    // 选中了已同步的会话时，用 `resume <SESSION_ID>` 续接该会话，
    // 这样手机消息会接到用户在「控制对话」里选择的那个 Codex 线程中，
    // 并携带其完整历史上下文；没有选中线程才退回到全新临时会话。
    let thread = thread_id.trim();
    if thread.is_empty() {
        command.arg("--ephemeral");
    } else {
        command.arg("resume").arg(thread);
    }
    command.arg("-");
    let mut child = command
        .current_dir(workdir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("启动 Codex CLI 失败({}): {e}", executable.display()))?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|e| format!("写入 Codex CLI prompt 失败: {e}"))?;
    }
    // 关闭 stdin，通知 codex 输入结束，否则 `exec -` 会一直等待。
    drop(child.stdin.take());
    // B9: 分离 stdout/stderr 用后台线程排干，主线程以 try_wait() 轮询 + 5 分钟超时，
    // 避免 wait_with_output() 无限阻塞，也防止 pipe 缓冲区满后子进程反过来卡死。
    let child_stdout = child.stdout.take();
    let child_stderr = child.stderr.take();
    let stdout_handle = std::thread::spawn(move || {
        use std::io::Read;
        let mut buf = Vec::new();
        if let Some(mut out) = child_stdout {
            let _ = out.read_to_end(&mut buf);
        }
        buf
    });
    let stderr_handle = std::thread::spawn(move || {
        use std::io::Read;
        let mut buf = Vec::new();
        if let Some(mut err) = child_stderr {
            let _ = err.read_to_end(&mut buf);
        }
        buf
    });
    let deadline = Instant::now() + Duration::from_secs(300);
    let exit_status = loop {
        match child
            .try_wait()
            .map_err(|e| format!("等待 Codex CLI 失败: {e}"))?
        {
            Some(status) => break status,
            None => {
                if Instant::now() > deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = fs::remove_file(&output_path);
                    return Err("Codex CLI 执行超时（5分钟），已强制终止".into());
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    };
    let stdout_bytes = stdout_handle.join().unwrap_or_default();
    let stderr_bytes = stderr_handle.join().unwrap_or_default();
    let reply = fs::read_to_string(&output_path)
        .unwrap_or_else(|_| String::from_utf8_lossy(&stdout_bytes).to_string())
        .trim()
        .to_string();
    let _ = fs::remove_file(&output_path);
    if !exit_status.success() {
        let mut detail = String::from_utf8_lossy(&stderr_bytes).trim().to_string();
        if detail.is_empty() {
            detail = String::from_utf8_lossy(&stdout_bytes).trim().to_string();
        }
        return Err(format!("Codex CLI 执行失败: {detail}"));
    }
    if reply.is_empty() {
        // 退出码成功但没有 last-message 文件时，退回用 stdout 内容。
        let stdout_text = String::from_utf8_lossy(&stdout_bytes).trim().to_string();
        if !stdout_text.is_empty() {
            return Ok(stdout_text);
        }
        Err("Codex CLI 没有返回回复".into())
    } else {
        Ok(reply)
    }
}

/// 构造执行 Codex 的 Command。对于 .cmd/.bat 包装脚本，Windows 下需通过 `cmd /C` 调用，
/// 否则 Rust 会因为不是合法的 PE 可执行文件而报 "program not found" / "%1 不是有效的应用程序"。
pub(crate) fn codex_command(executable: &std::path::Path) -> Command {
    let ext = executable
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    let mut cmd = if cfg!(windows) && matches!(ext.as_deref(), Some("cmd") | Some("bat")) {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(executable);
        c
    } else {
        Command::new(executable)
    };
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

// ===================== Codex 桌面 App CDP 注入 =====================
//
// 把手机消息注入 Codex 桌面 App(Electron)里用户选中的对话，让 App 自己收发回复。
// 机制:Codex App 带 `--remote-debugging-port` 启动后暴露 Chrome DevTools Protocol，
// 通过 HTTP 拿到页面 target 的 webSocketDebuggerUrl，再用裸 WebSocket 发
// `Runtime.evaluate` 在页面里执行 JS，定位输入框→填消息→点发送→轮询回复。

/// Codex App 常见的调试端口候选。watcher 启动 Codex 时通常用 9229。
pub(crate) const CODEX_CDP_PORTS: &[u16] = &[9229, 9222, 9223, 9230];

/// 探测 Codex App 的调试端口。先扫候选端口(命中 /json/version 即可)，
/// 再退回读取 Codex.exe 进程命令行里的 `--remote-debugging-port=`。
pub(crate) fn codex_debug_port() -> Option<u16> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(800))
        .build()
        .ok()?;
    for &port in CODEX_CDP_PORTS {
        let url = format!("http://127.0.0.1:{port}/json/version");
        if let Ok(resp) = client.get(&url).send() {
            if resp.status().is_success() {
                return Some(port);
            }
        }
    }
    codex_debug_port_from_process()
}

/// 从正在运行的 Codex.exe / ChatGPT.exe 命令行里解析 `--remote-debugging-port=<port>`。
#[cfg(windows)]
pub(crate) fn codex_debug_port_from_process() -> Option<u16> {
    let mut cmd = Command::new("powershell");
    cmd.args([
            "-NoProfile",
            "-Command",
            "Get-CimInstance Win32_Process -Filter \"name='ChatGPT.exe' OR name='Codex.exe'\" | Select-Object -ExpandProperty CommandLine",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let output = cmd.output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let marker = "--remote-debugging-port=";
    for line in text.lines() {
        if let Some(idx) = line.find(marker) {
            let rest = &line[idx + marker.len()..];
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(port) = digits.parse::<u16>() {
                // 命令行里带参数不代表端口已在监听（进程可能刚启动、或是同名 CLI），
                // 必须实际探测 HTTP 端点，避免误报导致跳过重启流程。
                if port > 0 && codex_cdp_port_responds(port) {
                    return Some(port);
                }
            }
        }
    }
    None
}

#[cfg(not(windows))]
pub(crate) fn codex_debug_port_from_process() -> Option<u16> {
    None
}

/// 用于重启的目标调试端口。
#[allow(dead_code)]
pub(crate) const CODEX_PREFERRED_DEBUG_PORT: u16 = 9229;

/// 探测指定端口上的 CDP HTTP 端点是否真正可用。
/// 注意不能用「进程命令行里带 --remote-debugging-port」来判断就绪：
/// 参数在进程启动瞬间就存在，而端口要等页面初始化后才开始监听。
pub(crate) fn codex_cdp_port_responds(port: u16) -> bool {
    let Ok(client) = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(800))
        .build()
    else {
        return false;
    };
    client
        .get(format!("http://127.0.0.1:{port}/json/version"))
        .send()
        .map(|resp| resp.status().is_success())
        .unwrap_or(false)
}

/// 探测调试端口。
///
/// CDP 不可用时，自动重启当前桌面 App 并开启本地调试端口，
/// 确保手机消息进入用户选择的 Codex App 对话，而不是静默转到 CLI。
pub(crate) fn codex_debug_port_or_relaunch() -> Result<u16, String> {
    if let Some(port) = codex_debug_port() {
        return Ok(port);
    }
    let port = CODEX_PREFERRED_DEBUG_PORT;
    relaunch_codex_with_debug_port(port)?;
    // Windows 桌面 App 启动后端口通常需要几秒才出现，等待期间不回退到 CLI，
    // 确保本次消息仍然进入用户选择的 Codex App 对话。冷启动(尤其 MSIX 包)可能
    // 较慢，这里直接探测 HTTP 端点，最多等约 60 秒。
    for _ in 0..60 {
        if codex_cdp_port_responds(port) {
            return Ok(port);
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    Err(format!(
        "Codex App 已尝试开启本地调试端口 {port}，但端口仍未就绪；请手动关闭并重新打开 Codex 桌面应用后重试"
    ))
}

/// 从正在运行的 Codex/ChatGPT 桌面 App 拿到 PID 和完整路径(动态获取，不写死安装位置)。
/// 2026-07 起 Codex 桌面 App 并入统一的 ChatGPT 桌面应用：Windows 主程序从 Codex.exe
/// 改名为 ChatGPT.exe（商店包标识沿用 OpenAI.Codex_*）。这里同时匹配两个进程名，
/// 并按可信度排序，避免匹配到旧版残留或 ChatGPT Classic。
#[cfg(windows)]
pub(crate) fn running_codex_desktop_process() -> Option<(u32, String)> {
    fn is_desktop_app_path(path: &str) -> bool {
        let trimmed = path.trim();
        if trimmed.is_empty() || !Path::new(trimmed).is_file() {
            return false;
        }
        let lower = trimmed.to_ascii_lowercase();
        // 系统里有很多同名的 codex.exe 并非桌面 App 主程序：npm 全局包
        // (node_modules/vendor)、桌面 App 包内置的 CLI(app\resources\codex.exe)、
        // 编辑器扩展自带的 CLI(.cursor/.vscode 下 extensions\...\bin\...)。
        // 杀掉/重启这些进程不会让桌面 App 打开调试端口，反而会误杀其他工具的
        // 后台进程，或者让仍在运行的 GUI 单实例吞掉激活参数导致端口永远不就绪。
        const EXCLUDED_SEGMENTS: [&str; 7] = [
            "node_modules",
            "\\vendor\\",
            "\\resources\\",
            "\\.cursor\\",
            "\\.vscode\\",
            "\\extensions\\",
            "\\bin\\",
        ];
        !EXCLUDED_SEGMENTS
            .iter()
            .any(|segment| lower.contains(segment))
    }

    fn parse_process_line(line: &str) -> Option<(u32, String)> {
        let (pid, path) = line.trim().split_once('\t')?;
        let pid = pid.trim().parse::<u32>().ok()?;
        let path = path.trim().to_string();
        is_desktop_app_path(&path).then_some((pid, path))
    }

    /// 候选可信度：统一版包（OpenAI.Codex_*，即合并后的 ChatGPT 桌面应用）最优，
    /// 其次是 ChatGPT.exe，再次旧版 Codex.exe；路径含 classic 的（ChatGPT Classic
    /// 旧聊天版，没有 Codex 模式）殿后。同分保持启动时间先后（主进程先启动）。
    fn candidate_rank(path: &str) -> u32 {
        let lower = path.to_ascii_lowercase();
        let mut rank = if lower.contains("openai.codex") {
            0
        } else if lower.ends_with("\\chatgpt.exe") {
            1
        } else {
            2
        };
        if lower.contains("classic") {
            rank += 10;
        }
        rank
    }

    fn pick_best(text: &str) -> Option<(u32, String)> {
        let mut candidates: Vec<(u32, String)> =
            text.lines().filter_map(parse_process_line).collect();
        // 稳定排序：先按可信度，同分保持 PowerShell 输出顺序（已按启动时间排序）。
        candidates.sort_by_key(|(_, path)| candidate_rank(path));
        candidates.into_iter().next()
    }

    let mut cmd = Command::new("powershell");
    cmd.args([
            "-NoProfile",
            "-Command",
            // 强制 UTF-8，避免用户名含中文时 Rust 把 PowerShell 输出解码成乱码。
            // 列出全部候选（不能只取第一个：同名的 CLI 进程可能排在桌面 App 前面），
            // 由 Rust 侧按路径过滤后选出真正的桌面 App 主程序。
            "$OutputEncoding = [Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); Get-CimInstance Win32_Process -Filter \"name='ChatGPT.exe' OR name='Codex.exe'\" | Where-Object { $_.ExecutablePath -and $_.ExecutablePath -notlike '*node_modules*' -and $_.CommandLine -notlike '*--type=*' } | Sort-Object CreationDate | ForEach-Object { \"$($_.ProcessId)`t$($_.ExecutablePath)\" }",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);
    if let Ok(output) = cmd.output() {
        if let Some(process) = pick_best(&String::from_utf8_lossy(&output.stdout)) {
            return Some(process);
        }
    }
    // Windows 的 WMI 权限受限时，Get-Process 仍能读取桌面版 Store App 路径。
    let mut fallback = Command::new("powershell");
    fallback
        .args([
            "-NoProfile",
            "-Command",
            // 按启动时间排序：Electron 主进程先于渲染子进程启动，这里拿不到命令行
            // 无法按 --type= 排除子进程，取最早启动的同名进程即主进程。
            "$OutputEncoding = [Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); Get-Process -Name ChatGPT,Codex -ErrorAction SilentlyContinue | Where-Object { $_.Path -and $_.Path -notlike '*node_modules*' } | Sort-Object StartTime | ForEach-Object { \"$($_.Id)`t$($_.Path)\" }",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(target_os = "windows")]
    fallback.creation_flags(CREATE_NO_WINDOW);
    let output = fallback.output().ok()?;
    pick_best(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(not(windows))]
pub(crate) fn running_codex_desktop_process() -> Option<(u32, String)> {
    None
}

#[cfg(windows)]
pub(crate) fn windows_package_family_from_exe_path(exe_path: &str) -> Option<String> {
    // WindowsApps 包目录形如 <Name>_<Version>_<Arch>__<PublisherId>。
    // 合并后的 ChatGPT 桌面应用沿用 OpenAI.Codex_* 包标识，但为兼容今后
    // 可能的包名变更，这里接受任何 <identity>__<publisherId> 形态的目录段
    // （本函数只在路径位于 WindowsApps 下时被调用，该形态即包目录）。
    let package_dir = Path::new(exe_path)
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .find(|component| component.contains("__"))?;
    let (identity, publisher_id) = package_dir.rsplit_once("__")?;
    let package_name = identity.split('_').next()?;
    if package_name.is_empty() || publisher_id.is_empty() {
        return None;
    }
    Some(format!("{package_name}_{publisher_id}"))
}

/// WindowsApps 中的 MSIX 可执行文件受 ACL 保护，普通桌面进程直接 spawn 会得到
/// ERROR_ACCESS_DENIED。通过系统的 ApplicationActivationManager 按 AUMID 激活，
/// 同时把 Electron 的 CDP 参数作为 activation arguments 传入。
#[cfg(windows)]
pub(crate) fn activate_packaged_codex(exe_path: &str, port: u16) -> Result<(), String> {
    let package_family = windows_package_family_from_exe_path(exe_path)
        .ok_or_else(|| format!("未能从 Codex App 路径解析包标识: {exe_path}"))?;
    let arguments =
        format!("--remote-debugging-port={port} --remote-allow-origins=http://127.0.0.1:{port}");
    let script = r#"
$ErrorActionPreference = 'Stop'
$OutputEncoding = [Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$app = Get-StartApps | Where-Object { $_.AppID -like ($env:VARSWITCH_CODEX_PACKAGE_FAMILY + '!*') } | Select-Object -First 1
if (-not $app) { throw ('未找到 Codex AppUserModelId，package_family=' + $env:VARSWITCH_CODEX_PACKAGE_FAMILY) }
if (-not ('VarSwitch.ApplicationActivationManager' -as [type])) {
  Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace VarSwitch {
  [Flags] public enum ActivateOptions { None = 0 }
  [ComImport, Guid("2e941141-7f97-4756-ba1d-9decde894a3d"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
  public interface IApplicationActivationManager {
    [PreserveSig] int ActivateApplication(
      [MarshalAs(UnmanagedType.LPWStr)] string appUserModelId,
      [MarshalAs(UnmanagedType.LPWStr)] string arguments,
      ActivateOptions options, out uint processId);
  }
  [ComImport, Guid("45BA127D-10A8-46EA-8AB7-56EA9078943C")]
  public class ApplicationActivationManager {}
  public static class PackagedAppActivator {
    public static uint Activate(string appUserModelId, string arguments) {
      var manager = (IApplicationActivationManager)new ApplicationActivationManager();
      uint processId;
      int hr = manager.ActivateApplication(appUserModelId, arguments, ActivateOptions.None, out processId);
      if (hr < 0) Marshal.ThrowExceptionForHR(hr);
      return processId;
    }
  }
}
'@
}
$pidValue = [VarSwitch.PackagedAppActivator]::Activate($app.AppID, $env:VARSWITCH_CODEX_ACTIVATION_ARGS)
Write-Output ($app.AppID + "`t" + $pidValue)
"#;
    let mut command = Command::new("powershell");
    command
        .args(["-NoProfile", "-Sta", "-Command", script])
        .env("VARSWITCH_CODEX_PACKAGE_FAMILY", &package_family)
        .env("VARSWITCH_CODEX_ACTIVATION_ARGS", &arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command
        .output()
        .map_err(|e| format!("调用 Windows 应用激活器失败: {e}"))?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if error.is_empty() {
            format!("Windows 应用激活器返回退出码 {}", output.status)
        } else {
            format!("Windows 应用激活器失败: {error}")
        });
    }
    let detail = String::from_utf8_lossy(&output.stdout).trim().to_string();
    log_info!(
        "[mobile-control][codex-app] packaged App activated: package_family={}, detail={}",
        package_family,
        detail
    );
    Ok(())
}

#[cfg(windows)]
pub(crate) fn relaunch_codex_with_debug_port(port: u16) -> Result<(), String> {
    let (pid, exe_path) = running_codex_desktop_process()
        .ok_or("未找到正在运行的 Codex/ChatGPT 桌面应用，请先打开 ChatGPT 桌面应用（原 Codex App，2026-07 起已改名）")?;
    log_info!(
        "[mobile-control][codex-app] restarting selected desktop App with CDP: pid={}, executable={}, port={}",
        pid,
        exe_path,
        port
    );
    let mut kill_cmd = Command::new("taskkill");
    kill_cmd
        .args(["/F", "/T", "/PID", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(target_os = "windows")]
    kill_cmd.creation_flags(CREATE_NO_WINDOW);
    let status = kill_cmd
        .status()
        .map_err(|e| format!("终止 Codex App 失败(PID {pid}): {e}"))?;
    if !status.success() {
        return Err(format!("终止 Codex App 失败(PID {pid})"));
    }
    std::thread::sleep(Duration::from_millis(1500));
    if exe_path.to_ascii_lowercase().contains("\\windowsapps\\") {
        activate_packaged_codex(&exe_path, port)
            .map_err(|e| format!("重启 Codex App 失败({exe_path}): {e}"))?;
    } else {
        let allow_origin = format!("http://127.0.0.1:{port}");
        let mut launch_cmd = Command::new(&exe_path);
        launch_cmd
            .arg(format!("--remote-debugging-port={port}"))
            .arg(format!("--remote-allow-origins={allow_origin}"))
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        launch_cmd.creation_flags(CREATE_NO_WINDOW);
        launch_cmd
            .spawn()
            .map_err(|e| format!("重启 Codex App 失败({exe_path}): {e}"))?;
    }
    log_info!(
        "[mobile-control][codex-app] desktop App restart command started: port={}",
        port
    );
    Ok(())
}

#[cfg(not(windows))]
pub(crate) fn relaunch_codex_with_debug_port(_port: u16) -> Result<(), String> {
    Err("当前平台暂不支持自动重启 Codex App".into())
}

/// 在调试端口上找到 Codex 页面 target，返回其 webSocketDebuggerUrl。
pub(crate) fn codex_find_page_target(port: u16) -> Result<String, String> {
    // App 刚重启时端口已就绪但页面可能还没初始化，target 列表会暂时为空，故重试。
    let mut last_err = String::from("没有找到可注入的 Codex 页面");
    for attempt in 0..20 {
        match codex_find_page_target_once(port) {
            Ok(ws) => return Ok(ws),
            Err(e) => {
                last_err = e;
                if attempt < 19 {
                    std::thread::sleep(Duration::from_millis(500));
                }
            }
        }
    }
    Err(format!("{last_err}（请确认 Codex App 已打开且可见）"))
}

pub(crate) fn codex_find_page_target_once(port: u16) -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|e| format!("创建 CDP HTTP 客户端失败: {e}"))?;
    let url = format!("http://127.0.0.1:{port}/json");
    let resp = client
        .get(&url)
        .send()
        .map_err(|e| format!("连接 Codex 调试端口失败: {e}"))?;
    let targets: serde_json::Value = resp
        .json()
        .map_err(|e| format!("解析 Codex target 列表失败: {e}"))?;
    let list = targets
        .as_array()
        .ok_or("Codex 调试端口未返回 target 列表")?;
    // 优先选 type==page 的可见页面;Codex 主窗口 url 形如 app://-/index.html。
    let mut fallback: Option<String> = None;
    for target in list {
        let target_type = json_string(target, &["type"]);
        let ws = json_string(target, &["webSocketDebuggerUrl"]);
        if ws.is_empty() {
            continue;
        }
        if target_type == "page" {
            let url = json_string(target, &["url"]);
            let title = json_string(target, &["title"]);
            // 跳过 devtools 自身页面。
            if url.starts_with("devtools://") {
                continue;
            }
            if url.contains("index.html")
                || url.starts_with("app://")
                || title.to_lowercase().contains("codex")
                || title.to_lowercase().contains("chatgpt")
            {
                return Ok(ws);
            }
            if fallback.is_none() {
                fallback = Some(ws);
            }
        }
    }
    fallback.ok_or_else(|| "Codex 页面尚未就绪".into())
}

/// 极简 CDP WebSocket 客户端(手写裸 WebSocket，仅满足注入需求)。
pub(crate) struct CdpClient {
    pub(crate) stream: TcpStream,
    pub(crate) next_id: i64,
}

impl CdpClient {
    /// 连接 webSocketDebuggerUrl(形如 ws://127.0.0.1:9229/devtools/page/<id>)并完成握手。
    fn connect(ws_url: &str) -> Result<CdpClient, String> {
        let rest = ws_url
            .strip_prefix("ws://")
            .ok_or("仅支持 ws:// 的 CDP 地址")?;
        let slash = rest.find('/').unwrap_or(rest.len());
        let host_port = &rest[..slash];
        let path = if slash < rest.len() {
            &rest[slash..]
        } else {
            "/"
        };
        let (host, port) = match host_port.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p.parse::<u16>().unwrap_or(9229)),
            None => (host_port.to_string(), 9229),
        };
        let mut stream = TcpStream::connect((host.as_str(), port))
            .map_err(|e| format!("连接 Codex 调试端口失败: {e}"))?;
        // 读超时设短（30s），recv_text 在帧边界超时时发 ping 保活并继续等待；
        // 整体等待上限由 recv_text 的 deadline 控制（须大于注入脚本的 300s 轮询上限）。
        stream.set_read_timeout(Some(Duration::from_secs(30))).ok();
        stream.set_write_timeout(Some(Duration::from_secs(10))).ok();
        // 随机 16 字节做握手 key。
        let key_bytes = uuid::Uuid::new_v4().into_bytes();
        let key = base64_encode_bytes(&key_bytes);
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
        );
        stream
            .write_all(request.as_bytes())
            .map_err(|e| format!("发送 WebSocket 握手失败: {e}"))?;
        // 读取握手响应直到空行。
        let mut header = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            let n = stream
                .read(&mut byte)
                .map_err(|e| format!("读取 WebSocket 握手响应失败: {e}"))?;
            if n == 0 {
                return Err("WebSocket 握手连接被关闭".into());
            }
            header.push(byte[0]);
            if header.ends_with(b"\r\n\r\n") {
                break;
            }
            if header.len() > 8192 {
                return Err("WebSocket 握手响应过长".into());
            }
        }
        let header_text = String::from_utf8_lossy(&header);
        if !header_text.contains("101") {
            return Err(format!(
                "WebSocket 握手失败: {}",
                header_text.lines().next().unwrap_or("")
            ));
        }
        Ok(CdpClient { stream, next_id: 0 })
    }

    /// 发送一个带掩码的文本帧(客户端→服务端必须带掩码)。
    fn send_text(&mut self, text: &str) -> Result<(), String> {
        let data = text.as_bytes();
        let len = data.len();
        let mut frame = Vec::with_capacity(len + 14);
        frame.push(0x81); // FIN + 文本帧
        let mask_bit = 0x80u8;
        if len < 126 {
            frame.push(mask_bit | len as u8);
        } else if len < 65536 {
            frame.push(mask_bit | 126);
            frame.extend_from_slice(&(len as u16).to_be_bytes());
        } else {
            frame.push(mask_bit | 127);
            frame.extend_from_slice(&(len as u64).to_be_bytes());
        }
        let mask = uuid::Uuid::new_v4().into_bytes();
        frame.extend_from_slice(&mask[..4]);
        for (i, b) in data.iter().enumerate() {
            frame.push(b ^ mask[i % 4]);
        }
        self.stream
            .write_all(&frame)
            .map_err(|e| format!("发送 CDP 帧失败: {e}"))
    }

    /// 精确读取 n 字节。
    fn read_exact(&mut self, n: usize) -> Result<Vec<u8>, String> {
        let mut buf = vec![0u8; n];
        self.stream
            .read_exact(&mut buf)
            .map_err(|e| format!("读取 CDP 数据失败: {e}"))?;
        Ok(buf)
    }

    /// 发送一个带掩码的空 ping 帧（客户端→服务端必须带掩码）。
    fn send_ping(&mut self) -> Result<(), String> {
        let mask = uuid::Uuid::new_v4().into_bytes();
        let mut frame = Vec::with_capacity(6);
        frame.push(0x89); // FIN + ping
        frame.push(0x80); // masked, len=0
        frame.extend_from_slice(&mask[..4]);
        self.stream
            .write_all(&frame)
            .map_err(|e| format!("发送 CDP ping 失败: {e}"))
    }

    /// 读取 2 字节帧头。仅在「帧边界」（首字节尚未到达）容忍读超时：
    /// Codex 执行长任务时页面可能几分钟不发任何帧，读超时不代表连接断开，
    /// 发 ping 保活并继续等待，直到 deadline。帧中途超时视为真实错误。
    fn read_frame_head(&mut self, deadline: Instant) -> Result<[u8; 2], String> {
        let mut head = [0u8; 2];
        loop {
            match self.stream.read(&mut head[..1]) {
                Ok(0) => return Err("CDP 连接已关闭".into()),
                Ok(_) => break,
                Err(e)
                    if e.kind() == std::io::ErrorKind::TimedOut
                        || e.kind() == std::io::ErrorKind::WouldBlock =>
                {
                    if Instant::now() > deadline {
                        return Err("等待 CDP 响应超时（Codex 长时间未返回结果）".into());
                    }
                    self.send_ping()?;
                }
                Err(e) => return Err(format!("读取 CDP 数据失败: {e}")),
            }
        }
        self.stream
            .read_exact(&mut head[1..])
            .map_err(|e| format!("读取 CDP 数据失败: {e}"))?;
        Ok(head)
    }

    /// 读取一个完整帧的 payload(自动应答 ping，跳过非文本控制帧)。
    fn recv_text(&mut self) -> Result<String, String> {
        // 整体等待上限：略大于注入脚本自身的 300s 轮询上限。
        let deadline = Instant::now() + Duration::from_secs(330);
        loop {
            let head = self.read_frame_head(deadline)?;
            let opcode = head[0] & 0x0f;
            let masked = (head[1] & 0x80) != 0;
            let mut len = (head[1] & 0x7f) as usize;
            if len == 126 {
                let ext = self.read_exact(2)?;
                len = u16::from_be_bytes([ext[0], ext[1]]) as usize;
            } else if len == 127 {
                let ext = self.read_exact(8)?;
                len = u64::from_be_bytes([
                    ext[0], ext[1], ext[2], ext[3], ext[4], ext[5], ext[6], ext[7],
                ]) as usize;
            }
            // 安全：限制单帧大小，避免对端声明超大长度导致一次性分配巨量内存。
            if len > 64 * 1024 * 1024 {
                return Err("CDP WebSocket 帧过大".into());
            }
            let mask = if masked {
                Some(self.read_exact(4)?)
            } else {
                None
            };
            let mut payload = self.read_exact(len)?;
            if let Some(mask) = mask {
                for (i, b) in payload.iter_mut().enumerate() {
                    *b ^= mask[i % 4];
                }
            }
            match opcode {
                0x1 => return Ok(String::from_utf8_lossy(&payload).to_string()),
                0x8 => return Err("CDP WebSocket 已关闭".into()),
                0x9 => {
                    // ping → 回 pong(带掩码空帧即可，简化处理)
                    let _ = self.send_pong(&payload);
                    continue;
                }
                _ => continue, // pong / 续帧等，忽略
            }
        }
    }

    fn send_pong(&mut self, payload: &[u8]) -> Result<(), String> {
        let len = payload.len().min(125);
        let mut frame = Vec::with_capacity(len + 6);
        frame.push(0x8a); // FIN + pong
        frame.push(0x80 | len as u8);
        let mask = uuid::Uuid::new_v4().into_bytes();
        frame.extend_from_slice(&mask[..4]);
        for (i, b) in payload.iter().take(len).enumerate() {
            frame.push(b ^ mask[i % 4]);
        }
        self.stream
            .write_all(&frame)
            .map_err(|e| format!("发送 pong 失败: {e}"))
    }

    /// 发送一条 CDP 命令并等待匹配 id 的响应。
    fn command(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        self.next_id += 1;
        let id = self.next_id;
        let request = serde_json::json!({ "id": id, "method": method, "params": params });
        self.send_text(&request.to_string())?;
        // 循环读取直到拿到本次命令的响应(跳过事件通知)。
        // 墙钟总时限：页面持续发事件帧（如 console 日志）会让每次读取都成功，
        // 若响应因页面跳转等原因永远不来，仅靠读取次数上限可能等非常久。
        let deadline = Instant::now() + Duration::from_secs(360);
        for _ in 0..10000 {
            if Instant::now() > deadline {
                return Err(format!("CDP 命令 {method} 等待响应超过总时限"));
            }
            let text = self.recv_text()?;
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
                continue;
            };
            if value.get("id").and_then(|v| v.as_i64()) == Some(id) {
                if let Some(err) = value.get("error") {
                    return Err(format!("CDP 命令 {method} 失败: {err}"));
                }
                return Ok(value
                    .get("result")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null));
            }
            // 否则是事件(method 字段)，继续等。
        }
        Err(format!("CDP 命令 {method} 等待响应超时"))
    }
}

/// 在 Codex 页面执行一段 JS 表达式，返回其(已 returnByValue 的)结果值。
pub(crate) fn cdp_evaluate(client: &mut CdpClient, expression: &str) -> Result<serde_json::Value, String> {
    client.command("Runtime.enable", serde_json::json!({}))?;
    let result = client.command(
        "Runtime.evaluate",
        serde_json::json!({
            "expression": expression,
            "returnByValue": true,
            "awaitPromise": true,
            "allowUnsafeEvalBlockedByCSP": true,
            "userGesture": true,
        }),
    )?;
    if let Some(exception) = result.get("exceptionDetails") {
        return Err(format!("Codex 页面脚本异常: {exception}"));
    }
    Ok(result
        .get("result")
        .and_then(|r| r.get("value"))
        .cloned()
        .unwrap_or(serde_json::Value::Null))
}

/// 注入到 Codex 页面的“只发送”脚本。`{PROMPT_JSON}` 处会被替换为 JSON 安全转义后的消息。
/// 逻辑:找输入框 → 写入消息 → 点发送(兜底回车) → 确认输入框清空后立刻返回 {ok:true,sent:true}。
/// 不在页面里等待/抓取回复——回复由后端 `codex_capture_reply_from_session` 从共享会话文件读取，
/// 避免新版 ChatGPT 界面 DOM 抓取失败导致的 300 秒空等（会拖过平台被动回复窗口）。
pub(crate) fn codex_inject_send_script(prompt_json: &str) -> String {
    const TEMPLATE: &str = r#"
(async () => {
  const prompt = __PROMPT_JSON__;
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const textOf = (n) => (n?.innerText || n?.textContent || "").trim();
  function visible(n) {
    if (!n) return false;
    const s = getComputedStyle(n);
    const r = n.getBoundingClientRect();
    return s.visibility !== "hidden" && s.display !== "none" && r.width > 0 && r.height > 0;
  }
  function assistantTexts() {
    // Codex 桌面 App 的回复渲染在 markdown 内容块里，且不在用户气泡(items-end / bg-token-foreground)内。
    // 用户消息气泡含 items-end 或 bg-token-foreground 类，据此排除。
    function inUserBubble(node) {
      let c = node;
      for (let i = 0; c && i < 12; i++, c = c.parentElement) {
        const cl = (c.className || "").toString();
        if (cl.includes("items-end") || cl.includes("bg-token-foreground")) return true;
      }
      return false;
    }
    const values = [];
    // 优先用 markdown 内容块(每条回复一个 _markdownContent_ 容器；
    // 合并后的 ChatGPT 桌面应用也常用 prose 类渲染 markdown)。
    let blocks = Array.from(document.querySelectorAll("[class*='markdownContent'], [class*='markdown-content'], [class*='_markdown'], [class*='prose']"))
      .filter(visible)
      .filter(n => !inUserBubble(n));
    // 退路:旧版属性选择器。
    if (!blocks.length) {
      blocks = Array.from(document.querySelectorAll("[data-message-author-role='assistant'], [data-role='assistant'], [class*='assistant'], article"))
        .filter(visible);
    }
    for (const n of blocks) {
      const t = textOf(n);
      if (t && !values.includes(t)) values.push(t);
    }
    return values;
  }
  function findInput() {
    const selectors = ["textarea:not([disabled])", "[contenteditable='true']", "[role='textbox']"];
    for (const sel of selectors) {
      const ns = Array.from(document.querySelectorAll(sel)).filter(visible);
      const n = ns[ns.length - 1];
      if (n) return n;
    }
    return null;
  }
  function setInputValue(input, value) {
    input.focus();
    if ("value" in input) {
      // textarea/input: 逐字符模拟打字,确保 React 受控组件每次都触发 onChange。
      const setter = Object.getOwnPropertyDescriptor(input.constructor.prototype, "value")?.set
        || Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")?.set
        || Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
      if (setter) {
        setter.call(input, "");
        if (value.length > 200) {
          // 长文本一次性写入：逐字符模拟在重型 React 组件上可能耗时数分钟。
          setter.call(input, value);
          input.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertText", data: value }));
        } else {
          for (const char of value) {
            const current = input.value;
            setter.call(input, current + char);
            input.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertText", data: char }));
          }
        }
      } else {
        input.value = value;
        input.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertText", data: value }));
      }
      input.dispatchEvent(new Event("change", { bubbles: true }));
    } else {
      // contenteditable: execCommand 逐字符插入(长文本一次性写入)。
      input.textContent = "";
      if (value.length > 200 || !document.execCommand) {
        input.textContent = value;
      } else {
        for (const char of value) {
          document.execCommand("insertText", false, char);
        }
      }
      input.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertText", data: value }));
      input.dispatchEvent(new Event("change", { bubbles: true }));
    }
  }
  function buttonLabel(b) {
    return [b.getAttribute("aria-label"), b.getAttribute("title"), b.getAttribute("data-testid"), b.textContent]
      .filter(Boolean).join(" ").toLowerCase();
  }
  function centerDistance(b, rect) {
    const r = b.getBoundingClientRect();
    const dx = Math.max(0, rect.left - r.right, r.left - rect.right);
    const dy = Math.max(0, rect.top - r.bottom, r.top - rect.bottom);
    return dx + dy;
  }
  function findSendButton(input) {
    const buttons = Array.from(document.querySelectorAll("button")).filter(visible);
    const inputRect = input.getBoundingClientRect();
    const labeled = buttons.filter(b => /send|submit|arrow|up|发送|提交/.test(buttonLabel(b)) && !b.disabled);
    if (labeled.length) return labeled.sort((a, b) => centerDistance(a, inputRect) - centerDistance(b, inputRect))[0];
    const containers = [];
    let node = input;
    for (let i = 0; node && i < 6; i++, node = node.parentElement) containers.push(node);
    for (const container of containers) {
      const local = Array.from(container.querySelectorAll("button")).filter(b => visible(b) && !b.disabled);
      const rightSide = local.filter(b => {
        const r = b.getBoundingClientRect();
        return r.left >= inputRect.left && r.top < inputRect.bottom + 80 && r.bottom > inputRect.top - 80;
      });
      if (rightSide.length) return rightSide.sort((a, b) => b.getBoundingClientRect().right - a.getBoundingClientRect().right)[0];
    }
    return buttons.filter(b => !b.disabled).sort((a, b) => centerDistance(a, inputRect) - centerDistance(b, inputRect))[0] || null;
  }
  function activateButton(button) {
    if (!button) return false;
    // 不再强制滚动按钮到视口中央，避免兼容注入时改变用户当前 Codex 窗口滚动位置/视觉布局。
    // 直接调 click(),不触发冗余的鼠标事件链(可能导致重复提交)。
    button.click();
    return true;
  }
  function submitByKeyboard(input) {
    const opts = { key: "Enter", code: "Enter", which: 13, keyCode: 13, bubbles: true, cancelable: true };
    input.dispatchEvent(new KeyboardEvent("keydown", opts));
    input.dispatchEvent(new KeyboardEvent("keypress", opts));
    input.dispatchEvent(new KeyboardEvent("keyup", opts));
  }
  // 等待输入框出现（重启 Codex 后页面需要时间加载，最多等约 15 秒）。
  let input = null;
  for (let i = 0; i < 100; i++) {
    input = findInput();
    if (input) break;
    await sleep(150);
  }
  if (!input) return { ok: false, error: "未找到 Codex 输入框（页面可能还在加载）" };
  setInputValue(input, prompt);
  let sent = false;
  for (let i = 0; i < 20; i++) {
    await sleep(150);
    const button = findSendButton(input);
    if (button && !button.disabled) { sent = activateButton(button); break; }
  }
  if (!sent) submitByKeyboard(input);
  // 只负责把消息发出去，不在页面里等待/抓取回复：新版 ChatGPT 桌面应用界面
  // DOM 结构多变，抓取回复常常空等到 300 秒超时，拖过平台被动回复窗口（QQ 仅 5 分钟）。
  // 回复改由后端从共享会话文件读取（稳定且快）。这里确认输入框已清空作为“已发送”信号，
  // 最多等约 3 秒。
  let cleared = false;
  for (let i = 0; i < 20; i++) {
    await sleep(150);
    const v = ("value" in input) ? input.value : (input.textContent || "");
    if (!v || !v.trim()) { cleared = true; break; }
  }
  return { ok: true, sent: true, cleared };
})()
"#;
    TEMPLATE.replace("__PROMPT_JSON__", prompt_json)
}

/// 最近一次通过深链接激活的对话（thread_id + 时间），用于缩短重复激活的等待。
pub(crate) static CODEX_LAST_ACTIVATED_THREAD: OnceLock<Mutex<Option<(String, Instant)>>> = OnceLock::new();

/// 仅触发 codex://threads/<id> 深链接打开（不等待 App 完成切换），
/// 供「会话管理器恢复」与下方的消息注入激活流程共用。
pub(crate) fn open_codex_thread_deep_link(thread_id: &str) -> Result<(), String> {
    let id = thread_id.trim();
    if id.is_empty() {
        return Err("会话 ID 为空".into());
    }
    let deep_link = format!("codex://threads/{id}");
    #[cfg(windows)]
    {
        // 用 cmd 的 start 触发系统协议处理(start 的首个引号参数是窗口标题，故留空)。
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", "start", "", &deep_link])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(target_os = "windows")]
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd.spawn()
            .map_err(|e| format!("打开 Codex 对话失败: {e}"))?;
    }
    #[cfg(not(windows))]
    {
        let opener = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        Command::new(opener)
            .arg(&deep_link)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("打开 Codex 对话失败: {e}"))?;
    }
    Ok(())
}

/// 用 codex://threads/<id> deep link 让 Codex App 切到指定对话，并等待切换完成。
/// （合并后的 ChatGPT 桌面应用仍注册 codex:// 协议处理 Codex 线程链接。）
pub(crate) fn activate_codex_thread(thread_id: &str) -> Result<(), String> {
    let id = thread_id.trim();
    if id.is_empty() {
        return Ok(());
    }
    open_codex_thread_deep_link(id)?;
    // 等待 App 切换对话并渲染。若上一条消息刚激活过同一对话（App 已停在该对话上），
    // 深链接等同无操作，缩短等待以降低每条消息的固定延迟。
    let cache = CODEX_LAST_ACTIVATED_THREAD.get_or_init(|| Mutex::new(None));
    let recently_same = cache
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
        .map(|(last_id, at)| last_id == id && at.elapsed() < Duration::from_secs(600))
        .unwrap_or(false);
    let settle = if recently_same {
        Duration::from_millis(700)
    } else {
        Duration::from_millis(2000)
    };
    std::thread::sleep(settle);
    if let Ok(mut guard) = cache.lock() {
        *guard = Some((id.to_string(), Instant::now()));
    }
    Ok(())
}

/// Codex App 注入失败的两种性质，决定兜底策略：
pub(crate) enum CodexAppSendFailure {
    /// 消息确定没有进入 Codex（探测/重启/连接/找输入框失败），可安全改投后台 CLI 重发。
    NotSent(String),
    /// 消息可能已经发送成功（连接中断或回复未捕获），绝不能重发，只允许只读回捞回复。
    MaybeSent { port: u16, error: String },
}

/// 把消息注入 Codex 桌面 App 选中的对话，等待并返回助手回复。
/// 流程:切对话 → 探测调试端口 → 找页面 target → CDP 注入发送脚本 → 解析回复。
pub(crate) fn send_prompt_to_codex_app(thread_id: &str, prompt: &str) -> Result<String, CodexAppSendFailure> {
    use CodexAppSendFailure::{MaybeSent, NotSent};
    let text = prompt.trim();
    if text.is_empty() {
        return Err(NotSent("消息内容为空".into()));
    }
    // 1) 探测调试端口；没有则自动用调试端口重启 Codex App(放在切对话之前，
    //    因为重启会重置页面，切对话必须在重启之后做)。
    let port = codex_debug_port_or_relaunch().map_err(NotSent)?;
    // 2) 切到选中的对话(thread_id 为空则注入当前打开的对话)。
    if let Err(error) = activate_codex_thread(thread_id) {
        log_warn!("[mobile-control][codex-app] 切换对话失败(继续尝试注入当前对话): {error}");
    }
    // 3) 找页面 target。
    let ws_url = codex_find_page_target(port).map_err(NotSent)?;
    // 4) 连接 CDP 并注入。消息在脚本点击发送后才算「可能已发出」；
    //    在那之前的所有失败都可以安全重投 CLI。
    let mut client = CdpClient::connect(&ws_url).map_err(NotSent)?;
    client
        .command("Runtime.enable", serde_json::json!({}))
        .map_err(NotSent)?;
    let prompt_json = serde_json::to_string(text)
        .map_err(|e| NotSent(format!("序列化消息失败: {e}")))?;
    let script = codex_inject_send_script(&prompt_json);
    let result = client
        .command(
            "Runtime.evaluate",
            serde_json::json!({
                "expression": script,
                "returnByValue": true,
                "awaitPromise": true,
                "allowUnsafeEvalBlockedByCSP": true,
                "userGesture": true,
            }),
        )
        .map_err(|e| MaybeSent {
            port,
            error: format!("CDP 连接中断（消息可能已发送）: {e}"),
        })?;
    if let Some(exception) = result.get("exceptionDetails") {
        return Err(MaybeSent {
            port,
            error: format!("Codex 页面脚本异常: {exception}"),
        });
    }
    let value = result
        .get("result")
        .and_then(|r| r.get("value"))
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    // 5) 解析脚本返回 {ok, sent, [text]}。当前脚本只负责发送，正常返回 {ok:true}
    //    且不含 text；回复交由调用方从共享会话文件读取。若某个旧路径仍返回了 text，
    //    也照常直接使用。
    let ok = value.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
    if ok {
        let reply = json_string(&value, &["text"]);
        if reply.trim().is_empty() {
            return Err(MaybeSent {
                port,
                error: "消息已发送到 Codex，正在从会话文件读取回复".into(),
            });
        }
        Ok(reply.trim().to_string())
    } else {
        let error = json_string(&value, &["error"]);
        let error = if error.is_empty() {
            "Codex App 注入失败".to_string()
        } else {
            error
        };
        // 「未找到输入框」发生在输入/点击之前，消息一定没有发出去。
        if error.contains("未找到 Codex 输入框") {
            Err(NotSent(error))
        } else {
            Err(MaybeSent { port, error })
        }
    }
}

/// 只读回捞：消息可能已送达 Codex 但回复没拿到时，重新连接页面轮询最新的
/// 助手回复，直到生成结束且文本稳定。绝不重新发送消息，避免重复执行。
pub(crate) fn codex_capture_reply_readonly(port: u16) -> Result<String, String> {
    const CAPTURE_SCRIPT: &str = r#"
(() => {
  const textOf = (n) => (n?.innerText || n?.textContent || "").trim();
  function visible(n) {
    if (!n) return false;
    const s = getComputedStyle(n);
    const r = n.getBoundingClientRect();
    return s.visibility !== "hidden" && s.display !== "none" && r.width > 0 && r.height > 0;
  }
  function inUserBubble(node) {
    let c = node;
    for (let i = 0; c && i < 12; i++, c = c.parentElement) {
      const cl = (c.className || "").toString();
      if (cl.includes("items-end") || cl.includes("bg-token-foreground")) return true;
    }
    return false;
  }
  let blocks = Array.from(document.querySelectorAll("[class*='markdownContent'], [class*='markdown-content'], [class*='_markdown'], [class*='prose']"))
    .filter(visible)
    .filter(n => !inUserBubble(n));
  if (!blocks.length) {
    blocks = Array.from(document.querySelectorAll("[data-message-author-role='assistant'], [data-role='assistant'], [class*='assistant'], article"))
      .filter(visible);
  }
  const values = [];
  for (const n of blocks) {
    const t = textOf(n);
    if (t && !values.includes(t)) values.push(t);
  }
  let busy = false;
  const stopBtn = document.querySelector(
    "[data-testid*='stop'], [data-testid*='Stop'], button[aria-label*='Stop'], button[aria-label*='停止'], " +
    "button[aria-label*='Cancel'], button[aria-label*='取消'], button[aria-label*='中断']"
  );
  if (stopBtn) {
    const s = getComputedStyle(stopBtn);
    const r = stopBtn.getBoundingClientRect();
    busy = s.visibility !== "hidden" && s.display !== "none" && r.width > 0 && r.height > 0;
  }
  if (!busy) {
    busy = !!document.querySelector(
      "[class*='thinking'], [class*='Thinking'], [class*='loading'], [class*='spinner'], [class*='Spinner'], " +
      "[role='progressbar'], [class*='generating'], [class*='Generating'], [aria-busy='true']"
    );
  }
  return { busy, last: values[values.length - 1] || "" };
})()
"#;
    let ws_url = codex_find_page_target(port)?;
    let mut client = CdpClient::connect(&ws_url)?;
    let deadline = Instant::now() + Duration::from_secs(240);
    let mut stable: Option<(String, Instant)> = None;
    loop {
        let value = cdp_evaluate(&mut client, CAPTURE_SCRIPT)?;
        let busy = value.get("busy").and_then(|v| v.as_bool()).unwrap_or(false);
        let last = json_string(&value, &["last"]);
        if !busy && !last.is_empty() {
            match &stable {
                Some((prev, since)) if *prev == last => {
                    // 连续两次读取一致且间隔 ≥3s，认为回复已完成。
                    if since.elapsed() >= Duration::from_secs(3) {
                        return Ok(last);
                    }
                }
                _ => stable = Some((last.clone(), Instant::now())),
            }
        } else {
            stable = None;
        }
        if Instant::now() > deadline {
            return Err("回捞超时：Codex 长时间未完成回复".into());
        }
        std::thread::sleep(Duration::from_secs(3));
    }
}

/// 解析 Codex 会话 jsonl，返回 (助手 final_answer 条数, 最后一条 final_answer 文本)。
/// 桌面 App 与 CLI 共享该文件，App 生成的每条最终回复都会实时追加进去，因此这是
/// 比抓取 GUI DOM 更可靠的回复来源。若该会话使用旧格式（消息不带 phase 字段），
/// 退化为统计全部助手消息。返回 None 表示定位不到会话文件。
pub(crate) fn codex_session_reply_state(thread_id: &str) -> Option<(usize, String)> {
    let relative = codex_session_relative_file(thread_id)?;
    let path = codex_session_path_from_relative(&relative);
    Some(codex_session_reply_state_at(&path))
}

/// 直接解析给定路径的会话 jsonl（避免轮询期间反复递归扫描 sessions 目录）。
pub(crate) fn codex_session_reply_state_at(path: &Path) -> (usize, String) {
    let content = fs::read_to_string(path).unwrap_or_default();
    let mut final_count = 0usize;
    let mut last_final = String::new();
    let mut assistant_count = 0usize;
    let mut last_assistant = String::new();
    let mut saw_phase = false;
    for line in content.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        // 只认 response_item 里的 assistant 消息（type=message, role=assistant）。
        if value.pointer("/payload/type").and_then(|v| v.as_str()) != Some("message") {
            continue;
        }
        if value.pointer("/payload/role").and_then(|v| v.as_str()) != Some("assistant") {
            continue;
        }
        let text = value
            .pointer("/payload/content")
            .map(extract_text_from_content_value)
            .unwrap_or_default();
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        assistant_count += 1;
        last_assistant = text.to_string();
        if let Some(phase) = value.pointer("/payload/phase").and_then(|v| v.as_str()) {
            saw_phase = true;
            if phase == "final_answer" {
                final_count += 1;
                last_final = text.to_string();
            }
        }
    }
    if saw_phase {
        (final_count, last_final)
    } else {
        (assistant_count, last_assistant)
    }
}

/// 从共享会话文件里捕获「本次发送后新增的」助手最终回复。
/// baseline 是发送前的 final_answer 计数；轮询直到计数增长且文本稳定。
pub(crate) fn codex_capture_reply_from_session(thread_id: &str, baseline: usize) -> Result<String, String> {
    let relative = codex_session_relative_file(thread_id)
        .ok_or("未能定位该会话的存档文件")?;
    let path = codex_session_path_from_relative(&relative);
    let deadline = Instant::now() + Duration::from_secs(300);
    let mut stable: Option<(String, Instant)> = None;
    loop {
        let (count, last) = codex_session_reply_state_at(&path);
        if count > baseline && !last.is_empty() {
            match &stable {
                Some((prev, since)) if *prev == last => {
                    // 文本连续 3 秒不变，认为该轮回复已写完。
                    if since.elapsed() >= Duration::from_secs(3) {
                        return Ok(last);
                    }
                }
                _ => stable = Some((last.clone(), Instant::now())),
            }
        }
        if Instant::now() > deadline {
            return Err("等待会话文件出现新回复超时".into());
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

/// 所有手机消息都必须进入用户选择的 Codex 对话（绝不改投其他会话）。
///
/// 首选把消息注入 Codex/ChatGPT 桌面 App（回复会实时显示在 App 界面里）。
/// 无论走哪条路，回复优先从「共享会话文件」读取——桌面 App 每条最终回复都会
/// 实时写进该 jsonl，比抓取 GUI DOM 可靠得多（DOM 抓取受调试连接超时影响）。
/// 失败时按性质兜底：
/// - 消息「确定没发出去」（探测/重启/连接/找输入框失败）→ 用后台 Codex CLI
///   `exec resume <thread_id>` 续接同一个会话重投（Happy/Paseo 式的 CLI 驱动）；
/// - 消息「可能已发出」（连接中断/回复未捕获）→ 绝不重发（防止 Codex 重复执行），
///   从会话文件捕获本轮新回复；文件不可用时再退回只读抓取 DOM。
pub(crate) fn dispatch_codex_reply(thread_id: &str, text: &str, cwd: &str) -> Result<String, String> {
    // 高级控制通道走的是官方协议链路，比向桌面 App 注入 GUI 可靠得多，因此优先尝试。
    // 远端没连上时它直接返回 None，协议出错时也只记警告，两种情况都继续走下面的兼容路径，
    // 所以未启用高级控制的用户行为完全不变。
    match try_smart_control_dispatch(thread_id, text) {
        Ok(Some(reply)) => {
            log_info!(
                "[mobile-control][smart-control] reply produced over protocol channel: thread_id={}",
                thread_id
            );
            return Ok(reply);
        }
        Ok(None) => {}
        Err(error) => log_warn!(
            "[mobile-control][smart-control] 协议通道转发失败，改用兼容路径: thread_id={}, error={}",
            thread_id,
            error
        ),
    }
    // 发送前记录会话文件里的 final_answer 基线，用于识别「本次新增」的回复。
    let baseline = codex_session_reply_state(thread_id)
        .map(|(count, _)| count)
        .unwrap_or(0);
    let failure = match send_prompt_to_codex_app(thread_id, text) {
        Ok(reply) => {
            log_info!(
                "[mobile-control][codex-app] message sent to selected Codex App thread: thread_id={}",
                thread_id
            );
            return Ok(reply);
        }
        Err(failure) => failure,
    };
    match failure {
        CodexAppSendFailure::NotSent(app_error) => {
            log_warn!(
                "[mobile-control][codex-app] 消息未能送入 App，改用后台 CLI 续接同一会话: thread_id={}, error={}",
                thread_id,
                app_error
            );
            match run_codex_cli_reply(text, cwd, thread_id) {
                Ok(reply) => {
                    log_info!(
                        "[mobile-control][codex-cli] reply produced via CLI resume: thread_id={}",
                        thread_id
                    );
                    Ok(reply)
                }
                Err(cli_error) => Err(format!(
                    "Codex App 注入失败（{app_error}）；后台 CLI 续接同一会话也失败（{cli_error}）"
                )),
            }
        }
        CodexAppSendFailure::MaybeSent { port, error } => {
            log_warn!(
                "[mobile-control][codex-app] 消息可能已送达但回复未捕获，改从会话文件捕获（不重发）: thread_id={}, error={}",
                thread_id,
                error
            );
            // 首选：从共享会话文件捕获本轮新回复（App 一定会把 final_answer 写进去）。
            match codex_capture_reply_from_session(thread_id, baseline) {
                Ok(reply) => {
                    log_info!(
                        "[mobile-control][codex-app] reply captured from session file: thread_id={}",
                        thread_id
                    );
                    return Ok(reply);
                }
                Err(session_error) => {
                    log_warn!(
                        "[mobile-control][codex-app] 会话文件捕获失败，改为只读抓取 DOM: thread_id={}, error={}",
                        thread_id,
                        session_error
                    );
                }
            }
            // 退路：直接从页面 DOM 只读抓取最新回复。
            match codex_capture_reply_readonly(port) {
                Ok(reply) => {
                    log_info!(
                        "[mobile-control][codex-app] reply captured via readonly poll: thread_id={}",
                        thread_id
                    );
                    Ok(reply)
                }
                Err(capture_error) => Err(format!(
                    "消息已发送到 Codex App，但未能带回回复（{error}；会话文件与页面抓取均失败：{capture_error}）；请在电脑上查看 Codex 的回答"
                )),
            }
        }
    }
}

pub(crate) fn lark_tenant_access_token(
    client: &reqwest::blocking::Client,
    binding: &MobileChannelBinding,
) -> Result<String, String> {
    // B16: 先查缓存，避免每条消息重新鉴权触发限频。飞书 token TTL=7200s，提前 60s 刷新
    let cache_key = format!("lark:{}", binding.app_id.trim());
    let now = (chrono_timestamp_millis() / 1000) as u64;
    let cache = LARK_TOKEN_CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    if let Ok(guard) = cache.lock() {
        if let Some((token, expiry)) = guard.get(&cache_key) {
            if now < *expiry {
                return Ok(token.clone());
            }
        }
    }
    let base_url = normalize_mobile_base_url(&binding.base_url, "lark");
    let result = post_json_value(
        client,
        format!("{base_url}/open-apis/auth/v3/tenant_access_token/internal"),
        serde_json::json!({
            "app_id": binding.app_id.trim(),
            "app_secret": binding.app_secret.trim(),
        }),
    )?;
    let token = json_string(&result, &["tenant_access_token"]);
    if token.is_empty() {
        Err(format!(
            "获取飞书 tenant_access_token 失败: {}",
            platform_error_message(&result, "未返回 token")
        ))
    } else {
        // token 默认有效期 7200s，提前 60s 刷新
        let ttl = result
            .get("expire")
            .and_then(|v| v.as_u64())
            .unwrap_or(7200);
        let expiry = now + ttl.saturating_sub(60);
        if let Ok(mut guard) = cache.lock() {
            guard.insert(cache_key, (token.clone(), expiry));
        }
        Ok(token)
    }
}

pub(crate) fn send_lark_text_reply(
    binding: &MobileChannelBinding,
    message_id: &str,
    chat_id: &str,
    content: &str,
) -> Result<(), String> {
    let client = build_http_client(20)?;
    let token = lark_tenant_access_token(&client, binding)?;
    let base_url = normalize_mobile_base_url(&binding.base_url, "lark");
    for chunk in split_platform_reply_text(content, 3500, 5) {
        let reply_error: String;
        let payload = serde_json::json!({
            "content": serde_json::json!({ "text": chunk }).to_string(),
            "msg_type": "text"
        });
        let reply_result = client
            .post(format!(
                "{base_url}/open-apis/im/v1/messages/{}/reply",
                percent_encode_query_value(message_id)
            ))
            .bearer_auth(&token)
            .json(&payload)
            .send();
        match reply_result {
            Ok(response) => {
                let status = response.status();
                let body = response
                    .text()
                    .map_err(|e| format!("飞书回复响应读取失败: {e}"))?;
                let result: serde_json::Value =
                    serde_json::from_str(&body).unwrap_or_else(|_| serde_json::json!({}));
                if status.is_success() && is_platform_code_ok(&result) {
                    continue;
                }
                reply_error = if !status.is_success() {
                    format!("飞书 reply 返回 HTTP {}：{}", status.as_u16(), body)
                } else {
                    format!(
                        "飞书 reply 返回错误：{}",
                        platform_error_message(&result, "接口返回异常")
                    )
                };
            }
            Err(error) => {
                reply_error = format!("飞书 reply 请求失败: {error}");
            }
        }
        if chat_id.trim().is_empty() {
            return Err(if reply_error.is_empty() {
                "飞书 reply 失败，且缺少 chat_id，无法改用主动发送".into()
            } else {
                reply_error
            });
        }
        let response = client
            .post(format!(
                "{base_url}/open-apis/im/v1/messages?receive_id_type=chat_id"
            ))
            .bearer_auth(&token)
            .json(&serde_json::json!({
                "receive_id": chat_id.trim(),
                "content": serde_json::json!({ "text": chunk }).to_string(),
                "msg_type": "text"
            }))
            .send()
            .map_err(|e| format!("飞书主动发送失败: {e}; reply_error={reply_error}"))?;
        let status = response.status();
        let body = response
            .text()
            .map_err(|e| format!("飞书主动发送响应读取失败: {e}"))?;
        if !status.is_success() {
            return Err(format!(
                "飞书主动发送返回 HTTP {}：{}；reply_error={}",
                status.as_u16(),
                body,
                reply_error
            ));
        }
        let result: serde_json::Value =
            serde_json::from_str(&body).unwrap_or_else(|_| serde_json::json!({}));
        if !is_platform_code_ok(&result) {
            return Err(format!(
                "飞书主动发送失败: {}；reply_error={}",
                platform_error_message(&result, "接口返回异常"),
                reply_error
            ));
        }
    }
    Ok(())
}

pub(crate) fn qq_access_token(
    client: &reqwest::blocking::Client,
    binding: &MobileChannelBinding,
) -> Result<String, String> {
    // B16: 先查缓存，QQ token 有 expires_in 字段，提前 60s 刷新
    let cache_key = format!("qq:{}", binding.app_id.trim());
    let now = (chrono_timestamp_millis() / 1000) as u64;
    let cache = QQ_TOKEN_CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    if let Ok(guard) = cache.lock() {
        if let Some((token, expiry)) = guard.get(&cache_key) {
            if now < *expiry {
                return Ok(token.clone());
            }
        }
    }
    let result = post_json_value(
        client,
        "https://bots.qq.com/app/getAppAccessToken".into(),
        serde_json::json!({
            "appId": binding.app_id.trim(),
            "clientSecret": binding.app_secret.trim(),
        }),
    )?;
    let token = json_string(&result, &["access_token"]);
    if token.is_empty() {
        Err(format!(
            "QQ Bot 鉴权失败：{}",
            platform_error_message(&result, "未返回 access_token")
        ))
    } else {
        let ttl = result
            .get("expires_in")
            .and_then(|v| v.as_u64())
            .unwrap_or(1800);
        let expiry = now + ttl.saturating_sub(60);
        if let Ok(mut guard) = cache.lock() {
            guard.insert(cache_key, (token.clone(), expiry));
        }
        Ok(token)
    }
}

/// 在字符序列里从 `from` 起找到第一个目标字符的下标。
pub(crate) fn md_find_char(chars: &[char], from: usize, target: char) -> Option<usize> {
    (from..chars.len()).find(|&j| chars[j] == target)
}

/// 把一行里的行内 Markdown 语法清理成纯文本：
/// `[文字](链接)`/`![alt](链接)` → `文字 (链接)`；去掉 `**`、`__` 加粗标记与行内反引号。
pub(crate) fn strip_inline_markdown(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let is_img = c == '!' && i + 1 < chars.len() && chars[i + 1] == '[';
        if c == '[' || is_img {
            let bracket = if is_img { i + 1 } else { i };
            if let Some(close) = md_find_char(&chars, bracket + 1, ']') {
                if close + 1 < chars.len() && chars[close + 1] == '(' {
                    if let Some(rparen) = md_find_char(&chars, close + 2, ')') {
                        let text: String = chars[bracket + 1..close].iter().collect();
                        let url: String = chars[close + 2..rparen].iter().collect();
                        let text = text.trim();
                        let url = url.trim();
                        if url.is_empty() {
                            out.push_str(text);
                        } else if text.is_empty() {
                            out.push_str(url);
                        } else {
                            out.push_str(text);
                            out.push_str(" (");
                            out.push_str(url);
                            out.push(')');
                        }
                        i = rparen + 1;
                        continue;
                    }
                }
            }
        }
        out.push(c);
        i += 1;
    }
    out.replace("**", "").replace("__", "").replace('`', "")
}

/// 把 Codex 回复里的 Markdown 粗略转成整洁纯文本，供「不渲染 Markdown」的平台
/// （QQ 官方机器人只能发纯文本）使用，避免 `#`、`**`、代码围栏等符号原样显示。
/// 保守处理：只去装饰性符号，不改动正文与换行结构；代码围栏内的内容原样保留。
pub(crate) fn markdown_to_plaintext(md: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut in_fence = false;
    for raw in md.lines() {
        let trimmed_start = raw.trim_start();
        if trimmed_start.starts_with("```") || trimmed_start.starts_with("~~~") {
            // 代码围栏标记行本身丢弃，围栏内容原样保留。
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            lines.push(raw.to_string());
            continue;
        }
        let indent_len = raw.len() - raw.trim_start().len();
        let indent = &raw[..indent_len];
        let mut body = raw[indent_len..].to_string();
        // 标题：去掉行首连续的 # 号及其后空格。
        if body.starts_with('#') {
            body = body.trim_start_matches('#').trim_start().to_string();
        }
        // 引用块：去掉行首 > 。
        if body.starts_with('>') {
            body = body[1..].trim_start().to_string();
        }
        // 无序列表符号统一成 • （有序列表的数字保留）。
        let body = if body.starts_with("- ") || body.starts_with("* ") || body.starts_with("+ ") {
            format!("• {}", &body[2..])
        } else {
            body
        };
        lines.push(format!("{indent}{}", strip_inline_markdown(&body)));
    }
    // 折叠连续空行（>=2 压成 1），并去掉首尾空白。
    let mut result: Vec<String> = Vec::new();
    let mut prev_blank = false;
    for line in lines {
        let is_blank = line.trim().is_empty();
        if is_blank && prev_blank {
            continue;
        }
        prev_blank = is_blank;
        result.push(line);
    }
    result.join("\n").trim().to_string()
}

pub(crate) fn send_qq_text_reply(
    binding: &MobileChannelBinding,
    message: &serde_json::Value,
    content: &str,
) -> Result<(), String> {
    let client = build_http_client(20)?;
    let token = qq_access_token(&client, binding)?;
    // QQ 官方机器人只支持纯文本渲染，把 Markdown 清理成整洁纯文本再发，
    // 否则 #、**、代码围栏等符号会原样显示（用户反馈“没格式”）。
    let content = markdown_to_plaintext(content);
    let content = content.as_str();
    let scene = json_string(message, &["scene"]);
    let group_openid = json_string(message, &["groupOpenid", "group_openid"]);
    let openid = json_string(message, &["openid", "openId", "userOpenid"]);
    let message_id = json_string(message, &["messageId", "message_id"]);
    let event_id = json_string(message, &["eventId", "event_id"]);
    let path = if scene == "group" || !group_openid.is_empty() {
        if group_openid.is_empty() {
            return Err("缺少 QQ 群聊 group_openid，无法回复".into());
        }
        format!(
            "https://api.sgroup.qq.com/v2/groups/{}/messages",
            percent_encode_query_value(&group_openid)
        )
    } else {
        if openid.is_empty() {
            return Err("缺少 QQ 用户 openid，无法回复".into());
        }
        format!(
            "https://api.sgroup.qq.com/v2/users/{}/messages",
            percent_encode_query_value(&openid)
        )
    };
    for (_index, chunk) in split_platform_reply_text(content, 1800, 5)
        .into_iter()
        .enumerate()
    {
        let mut payload = serde_json::json!({
            "content": chunk,
            "msg_type": 0,
            "msg_seq": QQ_MSG_SEQ.fetch_add(1, Ordering::SeqCst),
        });
        if let Some(object) = payload.as_object_mut() {
            if !message_id.is_empty() {
                object.insert("msg_id".into(), serde_json::json!(message_id.clone()));
            }
            if !event_id.is_empty() {
                object.insert("event_id".into(), serde_json::json!(event_id.clone()));
            }
        }
        let response = client
            .post(&path)
            .header("Authorization", format!("QQBot {token}"))
            .json(&payload)
            .send()
            .map_err(|e| format!("QQ 回复发送失败: {e}"))?;
        let status = response.status();
        let body = response
            .text()
            .map_err(|e| format!("QQ 回复响应读取失败: {e}"))?;
        if !status.is_success() {
            return Err(format!("QQ 回复返回 HTTP {}：{}", status.as_u16(), body));
        }
    }
    Ok(())
}

pub(crate) fn wechat_bot_base_info() -> serde_json::Value {
    serde_json::json!({
        "channel_version": "2.4.4",
        "bot_agent": "VarSwitch/1.0"
    })
}

pub(crate) fn wechat_request_json(
    client: &reqwest::blocking::Client,
    method: &str,
    base_url: &str,
    endpoint: &str,
    token: &str,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let url = format!(
        "{}/{}",
        normalize_mobile_base_url(base_url, "wechat"),
        endpoint.trim_start_matches('/')
    );
    let request = match method {
        "GET" => client.get(&url),
        _ => client.post(&url).json(&payload),
    }
    .header("AuthorizationType", "ilink_bot_token")
    .header("Authorization", format!("Bearer {}", token.trim()))
    .header("X-WECHAT-UIN", uuid::Uuid::new_v4().to_string())
    .header("iLink-App-Id", "bot")
    .header("iLink-App-ClientVersion", "132100");
    let response = request
        .send()
        .map_err(|e| format!("微信 iLink 请求失败: {e}"))?;
    let status = response.status();
    let body = response
        .text()
        .map_err(|e| format!("微信 iLink 响应读取失败: {e}"))?;
    if !status.is_success() {
        return Err(format!(
            "微信 iLink 返回 HTTP {}：{}",
            status.as_u16(),
            body
        ));
    }
    if body.trim().is_empty() {
        Ok(serde_json::json!({}))
    } else {
        serde_json::from_str(&body).map_err(|e| format!("微信 iLink 返回不是 JSON: {e}; {body}"))
    }
}

pub(crate) fn wechat_json_array<'a>(
    value: &'a serde_json::Value,
    keys: &[&str],
) -> Vec<&'a serde_json::Value> {
    for key in keys {
        if let Some(array) = value.get(*key).and_then(|item| item.as_array()) {
            return array.iter().collect();
        }
    }
    Vec::new()
}

pub(crate) fn normalize_wechat_message_content(data: &serde_json::Value) -> String {
    let source = data
        .get("msg")
        .or_else(|| data.get("message"))
        .filter(|item| item.get("item_list").is_some())
        .unwrap_or(data);
    let mut parts = Vec::new();
    if let Some(items) = source.get("item_list").and_then(|item| item.as_array()) {
        for item in items {
            let item_type = item
                .get("type")
                .and_then(|value| value.as_i64())
                .unwrap_or(0);
            match item_type {
                1 => {
                    let text = item
                        .get("text_item")
                        .and_then(|value| value.get("text"))
                        .and_then(|value| value.as_str())
                        .unwrap_or_default()
                        .trim();
                    if !text.is_empty() {
                        parts.push(text.to_string());
                    }
                }
                2 => {
                    let url = item
                        .get("image_item")
                        .and_then(|value| value.get("media"))
                        .and_then(|value| value.get("full_url"))
                        .and_then(|value| value.as_str())
                        .unwrap_or_default();
                    parts.push(if url.is_empty() {
                        "[微信 图片]".into()
                    } else {
                        format!("[微信 图片: {url}]")
                    });
                }
                3 => parts.push("[微信 语音]".into()),
                4 => {
                    let filename = item
                        .get("file_item")
                        .and_then(|value| value.get("file_name"))
                        .and_then(|value| value.as_str())
                        .unwrap_or_default();
                    parts.push(if filename.is_empty() {
                        "[微信 文件]".into()
                    } else {
                        format!("[微信 文件: {filename}]")
                    });
                }
                5 => parts.push("[微信 视频]".into()),
                _ => {}
            }
        }
    }
    parts.join("\n").trim().to_string()
}

pub(crate) fn parse_wechat_incoming_message(
    data: &serde_json::Value,
    account_id: &str,
) -> Option<serde_json::Value> {
    let source = data
        .get("msg")
        .or_else(|| data.get("message"))
        .filter(|item| item.get("item_list").is_some())
        .unwrap_or(data);
    let content = normalize_wechat_message_content(source);
    let sender_id = json_string(source, &["from_user_id"]);
    if content.is_empty() || sender_id.is_empty() {
        return None;
    }
    Some(serde_json::json!({
        "eventId": json_string(source, &["client_id", "message_id", "seq"]),
        "messageId": json_string(source, &["message_id", "client_id", "seq"]),
        "content": content,
        "senderId": sender_id,
        "contextToken": json_string(source, &["context_token"]),
        "accountId": account_id,
    }))
}

pub(crate) fn send_wechat_text_reply(
    binding: &MobileChannelBinding,
    message: &serde_json::Value,
    content: &str,
) -> Result<(), String> {
    let client = build_http_client(20)?;
    let sender_id = json_string(message, &["senderId", "sender_id"]);
    if sender_id.is_empty() {
        return Err("缺少微信发送者 ID，无法回复".into());
    }
    let context_token = json_string(message, &["contextToken", "context_token"]);
    for chunk in split_platform_reply_text(content, 1800, 5) {
        let result = wechat_request_json(
            &client,
            "POST",
            &binding.base_url,
            "ilink/bot/sendmessage",
            &binding.bot_token,
            serde_json::json!({
                    "msg": {
                        "from_user_id": "",
                    "to_user_id": sender_id.clone(),
                    "client_id": format!("varswitch-wechat:{}-{}", chrono_timestamp_millis(), uuid::Uuid::new_v4()),
                    "message_type": 2,
                    "message_state": 2,
                    "item_list": [{
                        "type": 1,
                        "text_item": { "text": chunk }
                    }],
                    "context_token": if context_token.is_empty() { serde_json::Value::Null } else { serde_json::json!(context_token.clone()) }
                },
                "base_info": wechat_bot_base_info(),
            }),
        )?;
        if !is_platform_code_ok(&result) {
            return Err(format!(
                "微信回复发送失败：{}",
                platform_error_message(&result, "接口返回异常")
            ));
        }
    }
    Ok(())
}

pub(crate) fn update_channel_status(app: &tauri::AppHandle, channel: &str, status: &str, error: &str) {
    if error.trim().is_empty() {
        log_info!("[mobile-control][status][{}] {}", channel, status);
    } else {
        log_info!(
            "[mobile-control][status][{}] {} | error={}",
            channel,
            status,
            error
        );
    }
    let normalized = normalize_mobile_channel(channel);
    let mut state = read_toolbox_state(app);
    if let Some(binding) = state
        .mobile_channels
        .iter_mut()
        .find(|binding| binding.channel == normalized)
    {
        binding.status = status.to_string();
        binding.last_error = error.to_string();
        binding.updated_at = chrono_now();
    }
    let _ = write_toolbox_state(app, &state);
    // B15: 主动推送最新快照给前端，弹窗关闭后的断线/重连也能被看到，
    //      前端不必依赖短时轮询（listen("mobile-channel-status")）
    let _ = app.emit("mobile-channel-status", build_toolbox_snapshot(app));
}

pub(crate) fn handle_lark_bridge_message(
    app: &tauri::AppHandle,
    payload: &serde_json::Value,
) -> Result<(), String> {
    // B9: 串行锁，确保飞书消息按接收顺序处理完一条再处理下一条，防止 Codex 乱序响应
    let _guard = LARK_MSG_LOCK
        .lock()
        .map_err(|e| format!("获取飞书消息处理锁失败: {e}"))?;
    let message_id = json_string(payload, &["messageId", "message_id"]);
    let chat_id = json_string(payload, &["chatId", "chat_id"]);
    let text = json_string(payload, &["text", "content"]);
    if message_id.is_empty() || text.is_empty() {
        log_info!(
            "[mobile-control][lark] ignored empty message: message_id_empty={}, text_empty={}",
            message_id.is_empty(),
            text.is_empty()
        );
        return Ok(());
    }
    // B9: message_id 去重，防止飞书重投相同消息重复触发 Codex
    if check_and_record_msg_id(&LARK_SEEN_MSG_IDS, &message_id) {
        log_info!(
            "[mobile-control][lark] duplicate message_id={}, skipped",
            message_id
        );
        return Ok(());
    }
    log_info!(
        "[mobile-control][lark] received message: message_id={}, text_len={}",
        message_id,
        text.chars().count()
    );
    let state = read_toolbox_state(app);
    let binding = state
        .mobile_channels
        .iter()
        .find(|binding| binding.channel == "lark")
        .cloned()
        .ok_or("飞书通道不存在")?;
    // B7: 解绑后通道进程虽已被停止，但若 handler 仍被调用要在此拒绝，
    // 避免以 --ephemeral 在本机启动新 Codex 会话执行任意命令
    if !binding.enabled || binding.thread_id.trim().is_empty() {
        log_info!(
            "[mobile-control][lark] rejected: channel disabled or no thread bound (enabled={}, thread_id='{}')",
            binding.enabled, binding.thread_id
        );
        return Err("飞书通道未绑定会话，消息已忽略".into());
    }
    let selected_thread = state
        .synced_codex_threads
        .iter()
        .find(|thread| thread.id == binding.thread_id)
        .cloned();
    let cwd = selected_thread
        .as_ref()
        .map(|thread| thread.cwd.as_str())
        .unwrap_or("");
    let thread_id = selected_thread
        .as_ref()
        .map(|thread| thread.id.as_str())
        .unwrap_or("");
    // B7: 飞书同 QQ，防御绑定的会话不在 synced 列表时传空 thread_id 导致 --ephemeral 兜底
    if thread_id.is_empty() {
        log_info!(
            "[mobile-control][lark] rejected: bound thread not in synced list (binding.thread_id='{}')",
            binding.thread_id
        );
        return Err("飞书通道绑定的会话不存在，消息已忽略".into());
    }
    log_info!(
        "[mobile-control][lark] dispatching to codex: thread_id={}, thread_name={}, cwd={}",
        thread_id,
        selected_thread
            .as_ref()
            .map(|thread| thread.thread_name.as_str())
            .unwrap_or(""),
        cwd
    );
    update_channel_status(app, "lark", "收到飞书消息，正在发送给 Codex", "");
    let reply = match dispatch_codex_reply(thread_id, &text, cwd) {
        Ok(reply) => reply,
        Err(error) => {
            // B9: 执行失败也回传简短状态，手机端不会无响应且不泄露本机错误细节。
            let _ = send_lark_text_reply(
                &binding,
                &message_id,
                &chat_id,
                "Codex 当前未完成本次请求，请稍后重试。",
            );
            return Err(format!("Codex 执行失败：{error}"));
        }
    };
    log_info!(
        "[mobile-control][lark] codex replied: message_id={}, reply_len={}",
        message_id,
        reply.chars().count()
    );
    update_channel_status(app, "lark", "Codex 已回复，正在同步回飞书", "");
    send_lark_text_reply(&binding, &message_id, &chat_id, &reply)
        .map_err(|error| format!("飞书回发失败：{error}"))?;
    update_channel_status(app, "lark", "Codex 回复已同步回飞书", "");
    Ok(())
}

pub(crate) fn start_lark_bridge(app: tauri::AppHandle, binding: MobileChannelBinding) -> Result<(), String> {
    if LARK_BRIDGE_ACTIVE.swap(true, Ordering::SeqCst) {
        log_info!("[mobile-control][lark] bridge already active, skip start");
        return Ok(());
    }
    log_info!(
        "[mobile-control][lark] starting bridge: thread_id={}, thread_name={}, app_id={}",
        binding.thread_id,
        binding.thread_name,
        binding.app_id
    );
    let runner = ensure_lark_bridge_connector(&app)?;
    let node = node_command_name()?;
    let connector_dir = runner
        .parent()
        .ok_or("飞书 connector 运行目录不存在")?
        .to_path_buf();
    let mut cmd = Command::new(node);
    cmd.arg(&runner)
        .current_dir(connector_dir)
        .env("LARK_APP_ID", binding.app_id.trim())
        .env("LARK_APP_SECRET", binding.app_secret.trim())
        .env(
            "LARK_BASE_URL",
            normalize_mobile_base_url(&binding.base_url, "lark"),
        )
        .env("LARK_BOT_OPEN_ID", binding.bot_open_id.trim())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let mut child = cmd.spawn().map_err(|e| {
        LARK_BRIDGE_ACTIVE.store(false, Ordering::SeqCst);
        format!("启动飞书消息桥失败: {e}")
    })?;
    let stdout = child.stdout.take().ok_or("飞书消息桥没有输出通道")?;
    let stderr = child.stderr.take();
    if let Ok(mut guard) = LARK_BRIDGE_CHILD.lock() {
        *guard = Some(child);
    }
    if let Some(stderr) = stderr {
        std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().flatten() {
                log_info!("[mobile-control][lark][stderr] {}", line);
            }
        });
    }
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().flatten() {
            if !LARK_BRIDGE_ACTIVE.load(Ordering::SeqCst) {
                break;
            }
            let Ok(payload) = serde_json::from_str::<serde_json::Value>(&line) else {
                log_info!("[mobile-control][lark][stdout] {}", line);
                continue;
            };
            log_info!(
                "[mobile-control][lark][event] type={}",
                json_string(&payload, &["type"])
            ); // B5: 不记录完整 payload，避免用户聊天原文写进日志
            match json_string(&payload, &["type"]).as_str() {
                "ready" => {
                    update_channel_status(&app, "lark", "飞书智能体已在线，等待手机消息", "")
                }
                "message" => {
                    let message_app = app.clone();
                    std::thread::spawn(move || {
                        if let Err(error) = handle_lark_bridge_message(&message_app, &payload) {
                            update_channel_status(&message_app, "lark", "飞书消息处理失败", &error);
                        }
                    });
                }
                "status" => {
                    let message = json_string(&payload, &["message"]);
                    update_channel_status(&app, "lark", &message, "");
                }
                "failure" => {
                    let error = json_string(&payload, &["message"]);
                    update_channel_status(&app, "lark", "飞书消息桥启动失败", &error);
                }
                _ => {}
            }
        }
        // B10: 飞书连接断开时主动上报状态，UI 显示"已断开"而非继续显示"在线"
        update_channel_status(&app, "lark", "飞书连接已断开，看门狗将尝试重连", "");
        LARK_BRIDGE_ACTIVE.store(false, Ordering::SeqCst);
        if let Ok(mut guard) = LARK_BRIDGE_CHILD.lock() {
            let _ = guard.take().map(|mut child| child.wait());
        }
        log_info!("[mobile-control][lark] bridge worker exited");
    });
    Ok(())
}

pub(crate) fn stop_lark_bridge() {
    log_info!("[mobile-control][lark] stopping bridge");
    LARK_BRIDGE_ACTIVE.store(false, Ordering::SeqCst);
    if let Ok(mut guard) = LARK_BRIDGE_CHILD.lock() {
        if let Some(mut child) = guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

// B7: selected_thread_for_message 已不再使用，因为它有危险的 selected_mobile_thread_id 兜底。
// 所有 handler (lark/qq/wechat) 现在都直接按 binding.thread_id 查找，找不到则拒绝。

pub(crate) fn handle_qq_gateway_message(
    app: &tauri::AppHandle,
    payload: &serde_json::Value,
) -> Result<(), String> {
    // B9: 串行锁，确保 QQ 消息按顺序处理
    let _guard = QQ_MSG_LOCK
        .lock()
        .map_err(|e| format!("获取 QQ 消息处理锁失败: {e}"))?;
    let text = json_string(payload, &["content", "text"]);
    if text.is_empty() {
        return Ok(());
    }
    // B9: QQ message_id/eventId 去重，防止网关重投相同消息重复触发 Codex
    let qq_msg_key = json_string(payload, &["messageId", "eventId", "id"]);
    if check_and_record_msg_id(&QQ_SEEN_MSG_IDS, &qq_msg_key) {
        return Ok(());
    }
    let state = read_toolbox_state(app);
    let binding = state
        .mobile_channels
        .iter()
        .find(|binding| binding.channel == "qq")
        .cloned()
        .ok_or("QQ 通道不存在")?;
    // B7: 未绑定会话时拒绝执行，避免 --ephemeral 任意起新会话
    if !binding.enabled || binding.thread_id.trim().is_empty() {
        return Err("QQ 通道未绑定会话，消息已忽略".into());
    }
    let selected_thread = state
        .synced_codex_threads
        .iter()
        .find(|thread| thread.id == binding.thread_id)
        .cloned();
    let cwd = selected_thread
        .as_ref()
        .map(|thread| thread.cwd.clone())
        .unwrap_or_default();
    let thread_id = selected_thread
        .as_ref()
        .map(|thread| thread.id.as_str())
        .unwrap_or("");
    if thread_id.is_empty() {
        return Err("QQ 通道绑定的会话不存在，消息已忽略".into());
    }
    update_channel_status(app, "qq", "收到 QQ 消息，正在发送给 Codex", "");
    let reply = match dispatch_codex_reply(thread_id, &text, &cwd) {
        Ok(reply) => reply,
        Err(error) => {
            // B9: 失败时给 QQ 发送可见回执，避免用户只看到静默超时。
            let _ = send_qq_text_reply(&binding, payload, "Codex 当前未完成本次请求，请稍后重试。");
            return Err(format!("Codex 执行失败：{error}"));
        }
    };
    send_qq_text_reply(&binding, payload, &reply)?;
    update_channel_status(app, "qq", "Codex 回复已同步回 QQ", "");
    Ok(())
}

pub(crate) fn start_qq_gateway(app: tauri::AppHandle, binding: MobileChannelBinding) -> Result<(), String> {
    if QQ_GATEWAY_ACTIVE.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let runner = ensure_qq_gateway_connector(&app).map_err(|error| {
        QQ_GATEWAY_ACTIVE.store(false, Ordering::SeqCst);
        error
    })?;
    let node = node_command_name().map_err(|error| {
        QQ_GATEWAY_ACTIVE.store(false, Ordering::SeqCst);
        error
    })?;
    let connector_dir = runner
        .parent()
        .ok_or("QQ 网关运行目录不存在")?
        .to_path_buf();
    let mut cmd = Command::new(node);
    cmd.arg(&runner)
        .current_dir(connector_dir)
        .env("QQ_APP_ID", binding.app_id.trim())
        .env("QQ_APP_SECRET", binding.app_secret.trim())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let mut child = cmd.spawn().map_err(|e| {
        QQ_GATEWAY_ACTIVE.store(false, Ordering::SeqCst);
        format!("启动 QQ 网关失败: {e}")
    })?;
    let stdout = child.stdout.take().ok_or("QQ 网关没有输出通道")?;
    // B1: 消费 stderr，避免 Node 进程 stderr 缓冲区满后 write 阻塞（静默假死）
    let stderr_opt = child.stderr.take();
    if let Ok(mut guard) = QQ_GATEWAY_CHILD.lock() {
        *guard = Some(child);
    }
    if let Some(stderr) = stderr_opt {
        std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().flatten() {
                log_info!("[mobile-control][qq][stderr] {}", line);
            }
        });
    }
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().flatten() {
            if !QQ_GATEWAY_ACTIVE.load(Ordering::SeqCst) {
                break;
            }
            let Ok(payload) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            match json_string(&payload, &["type"]).as_str() {
                "ready" => update_channel_status(&app, "qq", "QQ 机器人已在线，等待手机消息", ""),
                "status" => {
                    let message = json_string(&payload, &["message"]);
                    update_channel_status(&app, "qq", &message, "");
                }
                "message" => {
                    let message_app = app.clone();
                    std::thread::spawn(move || {
                        if let Err(error) = handle_qq_gateway_message(&message_app, &payload) {
                            update_channel_status(&message_app, "qq", "QQ 消息处理失败", &error);
                        }
                    });
                }
                "failure" => {
                    let error = json_string(&payload, &["message"]);
                    update_channel_status(&app, "qq", "QQ 网关启动失败", &error);
                }
                _ => {}
            }
        }
        // B10: QQ 网关断开时主动上报，UI 停留在"在线"而非正确显示"已断开"
        update_channel_status(&app, "qq", "QQ 连接已断开，看门狗将尝试重连", "");
        QQ_GATEWAY_ACTIVE.store(false, Ordering::SeqCst);
        if let Ok(mut guard) = QQ_GATEWAY_CHILD.lock() {
            let _ = guard.take().map(|mut child| child.wait());
        }
    });
    Ok(())
}

pub(crate) fn stop_qq_gateway() {
    QQ_GATEWAY_ACTIVE.store(false, Ordering::SeqCst);
    if let Ok(mut guard) = QQ_GATEWAY_CHILD.lock() {
        if let Some(mut child) = guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub(crate) fn wechat_next_update_cursor(result: &serde_json::Value, previous: &str) -> String {
    for source in [
        Some(result),
        result.get("data"),
        result.get("result"),
        result.get("body"),
    ]
    .into_iter()
    .flatten()
    {
        let value = json_string(
            source,
            &[
                "get_updates_buf",
                "getUpdatesBuf",
                "next_get_updates_buf",
                "nextGetUpdatesBuf",
                "next_buf",
                "nextBuf",
                "buf",
            ],
        );
        if !value.is_empty() {
            return value;
        }
    }
    previous.to_string()
}

pub(crate) fn wechat_update_items(result: &serde_json::Value) -> Vec<serde_json::Value> {
    let mut items = Vec::new();
    for source in [
        Some(result),
        result.get("data"),
        result.get("result"),
        result.get("body"),
    ]
    .into_iter()
    .flatten()
    {
        for item in wechat_json_array(
            source,
            &[
                "msgs",
                "msg_list",
                "msgList",
                "message_list",
                "messageList",
                "messages",
                "updates",
                "update_list",
                "updateList",
            ],
        ) {
            items.push(item.clone());
        }
        if source.get("item_list").is_some() || source.get("msg").is_some() {
            items.push(source.clone());
        }
    }
    items
}

pub(crate) fn handle_wechat_bot_message(
    app: &tauri::AppHandle,
    payload: &serde_json::Value,
) -> Result<(), String> {
    // B9: 串行锁，确保微信消息按顺序处理
    let _guard = WECHAT_MSG_LOCK
        .lock()
        .map_err(|e| format!("获取微信消息处理锁失败: {e}"))?;
    let text = json_string(payload, &["content", "text"]);
    if text.is_empty() {
        return Ok(());
    }
    let state = read_toolbox_state(app);
    let binding = state
        .mobile_channels
        .iter()
        .find(|binding| binding.channel == "wechat")
        .cloned()
        .ok_or("微信通道不存在")?;
    // B7: 未绑定会话时拒绝执行，避免以 --ephemeral 任意起新 Codex 会话
    if !binding.enabled || binding.thread_id.trim().is_empty() {
        return Err("微信通道未绑定会话，消息已忽略".into());
    }
    let selected_thread = state
        .synced_codex_threads
        .iter()
        .find(|thread| thread.id == binding.thread_id)
        .cloned();
    let cwd = selected_thread
        .as_ref()
        .map(|thread| thread.cwd.clone())
        .unwrap_or_default();
    let thread_id = selected_thread
        .as_ref()
        .map(|thread| thread.id.as_str())
        .unwrap_or("");
    // B7: 微信同 QQ，防御绑定的会话不在 synced 列表时传空 thread_id 导致 --ephemeral 兜底
    if thread_id.is_empty() {
        return Err("微信通道绑定的会话不存在，消息已忽略".into());
    }
    update_channel_status(app, "wechat", "收到微信消息，正在发送给 Codex", "");
    let reply = match dispatch_codex_reply(thread_id, &text, &cwd) {
        Ok(reply) => reply,
        Err(error) => {
            // B9: 失败时给微信发送可见回执，避免用户只看到静默超时。
            let _ =
                send_wechat_text_reply(&binding, payload, "Codex 当前未完成本次请求，请稍后重试。");
            return Err(format!("Codex 执行失败：{error}"));
        }
    };
    send_wechat_text_reply(&binding, payload, &reply)?;
    update_channel_status(app, "wechat", "Codex 回复已同步回微信", "");
    Ok(())
}

pub(crate) fn start_wechat_listener(
    app: tauri::AppHandle,
    binding: MobileChannelBinding,
) -> Result<(), String> {
    if WECHAT_LISTENER_ACTIVE.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    std::thread::spawn(move || {
        let client = match build_http_client(45) {
            Ok(client) => client,
            Err(error) => {
                update_channel_status(&app, "wechat", "微信监听启动失败", &error);
                WECHAT_LISTENER_ACTIVE.store(false, Ordering::SeqCst);
                return;
            }
        };
        match wechat_request_json(
            &client,
            "POST",
            &binding.base_url,
            "ilink/bot/msg/notifystart",
            &binding.bot_token,
            serde_json::json!({ "base_info": wechat_bot_base_info() }),
        ) {
            Ok(result) if is_platform_code_ok(&result) => {
                update_channel_status(&app, "wechat", "微信 iLink 已在线，等待手机消息", "")
            }
            Ok(result) => {
                let err_msg = platform_error_message(&result, "接口返回异常");
                // 检测 token 过期
                if err_msg.to_lowercase().contains("session timeout")
                    || err_msg.to_lowercase().contains("expired")
                    || err_msg.to_lowercase().contains("token")
                {
                    update_channel_status(
                        &app,
                        "wechat",
                        "微信 token 已过期，请清除绑定后重新扫码",
                        &format!("错误详情：{}", err_msg),
                    );
                } else {
                    update_channel_status(&app, "wechat", "微信监听启动失败", &err_msg);
                }
                WECHAT_LISTENER_ACTIVE.store(false, Ordering::SeqCst);
                return;
            }
            Err(error) => {
                update_channel_status(&app, "wechat", "微信监听启动失败", &error);
                WECHAT_LISTENER_ACTIVE.store(false, Ordering::SeqCst);
                return;
            }
        }
        let mut cursor = String::new();
        let mut seen = HashSet::new();
        while WECHAT_LISTENER_ACTIVE.load(Ordering::SeqCst) {
            let result = match wechat_request_json(
                &client,
                "POST",
                &binding.base_url,
                "ilink/bot/getupdates",
                &binding.bot_token,
                serde_json::json!({
                    "get_updates_buf": cursor,
                    "base_info": wechat_bot_base_info(),
                }),
            ) {
                Ok(result) => result,
                Err(error) => {
                    let lower = error.to_lowercase();
                    // B12: token 过期不再无限重试，立即停止并通知用户重新绑定
                    if lower.contains("session timeout")
                        || lower.contains("expired")
                        || lower.contains("token")
                    {
                        update_channel_status(
                            &app,
                            "wechat",
                            "微信 token 已过期，请清除绑定后重新扫码",
                            &error,
                        );
                        break;
                    }
                    update_channel_status(&app, "wechat", "微信消息拉取失败", &error);
                    std::thread::sleep(Duration::from_secs(3));
                    continue;
                }
            };
            cursor = wechat_next_update_cursor(&result, &cursor);
            let items = wechat_update_items(&result);
            // B12: 空结果时不热循环，休眠 1 秒降低 CPU 和 API 调用频率
            if items.is_empty() {
                std::thread::sleep(Duration::from_secs(1));
            }
            for item in items {
                let Some(message) = parse_wechat_incoming_message(&item, &binding.account_id)
                else {
                    continue;
                };
                let key = json_string(&message, &["messageId", "eventId"]);
                if !key.is_empty() && !seen.insert(key.clone()) {
                    continue;
                }
                if seen.len() > 300 {
                    seen.clear();
                }
                let message_app = app.clone();
                std::thread::spawn(move || {
                    if let Err(error) = handle_wechat_bot_message(&message_app, &message) {
                        update_channel_status(&message_app, "wechat", "微信消息处理失败", &error);
                    }
                });
            }
        }
        // B10: 连接断开时上报状态，让 UI 显示"已断开"而非继续显示旧状态
        // B17: 不再在此调 notifystop，stop_wechat_listener 是唯一的 notifystop 调用点，
        //      避免 stop 后 start 线程再次 notifystop 产生双重调用
        update_channel_status(&app, "wechat", "微信连接已断开，看门狗将尝试重连", "");
        WECHAT_LISTENER_ACTIVE.store(false, Ordering::SeqCst);
    });
    Ok(())
}

pub(crate) fn stop_wechat_listener(app: &tauri::AppHandle) {
    WECHAT_LISTENER_ACTIVE.store(false, Ordering::SeqCst);
    let state = read_toolbox_state(app);
    if let Some(binding) = state
        .mobile_channels
        .iter()
        .find(|binding| binding.channel == "wechat" && !binding.bot_token.trim().is_empty())
    {
        if let Ok(client) = build_http_client(10) {
            let _ = wechat_request_json(
                &client,
                "POST",
                &binding.base_url,
                "ilink/bot/msg/notifystop",
                &binding.bot_token,
                serde_json::json!({ "base_info": wechat_bot_base_info() }),
            );
        }
    }
}

pub(crate) fn build_toolbox_snapshot(app: &tauri::AppHandle) -> ToolboxSnapshot {
    let mut state = read_toolbox_state(app);
    let _ = refresh_smart_control_status_for_state(&mut state);
    let config_path = codex_config_path();
    let config_text = fs::read_to_string(&config_path).unwrap_or_default();
    let current_source = state.plugin_marketplace_input.clone();
    let visible_threads = visible_codex_threads(&state);
    // B5: 对发给前端的凭据字段掩码，避免 secret 通过 snapshot 暴露到 devtools/DOM
    let masked_channels: Vec<MobileChannelBinding> = state
        .mobile_channels
        .iter()
        .cloned()
        .map(|mut ch| {
            // 用布尔语义标记（非空 → "*"），前端判断 hasAppSecret 即可，无需明文
            if !ch.app_secret.is_empty() {
                ch.app_secret = "*".to_string();
            }
            if !ch.bot_token.is_empty() {
                ch.bot_token = "*".to_string();
            }
            ch
        })
        .collect();
    ToolboxSnapshot {
        plugin_marketplace_input: current_source.clone(),
        plugin_marketplaces: list_plugin_marketplaces(&config_text, &current_source),
        builtin_plugins: list_codex_builtin_plugins(&config_text),
        session_bindings: state.session_bindings.clone(),
        codex_threads: Vec::new(),
        synced_codex_threads: visible_threads.clone(),
        trashed_codex_threads: state.trashed_codex_threads.clone(),
        session_sync: CodexSessionSyncState {
            last_synced_at: state.session_sync.last_synced_at.clone(),
            total: visible_threads.len(),
        },
        mobile_channels: masked_channels,
        selected_mobile_thread_id: state.selected_mobile_thread_id.clone(),
        mobile_remote: state.mobile_remote.clone(),
        codex_home: codex_config_dir().to_string_lossy().to_string(),
        codex_config_path: config_path.to_string_lossy().to_string(),
    }
}

/// 快照构建包含 HTTP 探测、PowerShell 调用与目录扫描，最坏要数秒。
/// Tauri 的同步命令跑在主线程上，而本命令在启动时自动调用、Toolbox 页还会每秒轮询，
/// 同步执行会持续冻结窗口，因此交给阻塞线程池。
#[tauri::command]
pub(crate) async fn get_codex_toolbox(app: tauri::AppHandle) -> Result<ToolboxSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || build_toolbox_snapshot(&app))
        .await
        .map_err(|e| format!("读取 Codex 工具箱状态失败: {e}"))
}

#[tauri::command]
pub(crate) async fn start_smart_control(app: tauri::AppHandle) -> Result<ToolboxSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = read_toolbox_state(&app);
        start_smart_control_server(
            app.clone(),
            state.mobile_remote.remote_control_backend_url.clone(),
        );
        std::thread::sleep(Duration::from_millis(120));
        build_toolbox_snapshot(&app)
    })
    .await
    .map_err(|e| format!("启动高级控制通道失败: {e}"))
}


#[tauri::command]
pub(crate) fn get_smart_control_debug() -> SmartControlDebugSnapshot {
    smart_control_debug_snapshot()
}

#[tauri::command]
pub(crate) fn submit_smart_control_approval(request_id: String, decision: String) -> Result<(), String> {
    let request_id = request_id.trim().to_string();
    if request_id.is_empty() {
        return Err("审批请求 ID 为空".into());
    }
    let id_value = request_id
        .parse::<u64>()
        .map(serde_json::Value::from)
        .unwrap_or_else(|_| serde_json::Value::String(request_id.clone()));
    let message = serde_json::json!({
        "id": id_value,
        "result": {
            "decision": decision.trim(),
            "approved": matches!(decision.trim().to_ascii_lowercase().as_str(), "approve" | "approved" | "allow" | "yes" | "accept"),
        }
    });
    let envelope = smart_control_wrap_client_message(message);
    smart_control_send_json(&envelope)?;
    if let Ok(mut approvals) = smart_control_approval_store().lock() {
        approvals.retain(|item| item.request_id != request_id);
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn repair_openai_bundled_plugins(
    app: tauri::AppHandle,
) -> Result<ToolboxSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || repair_openai_bundled_plugins_blocking(app))
        .await
        .map_err(|e| format!("修复内置插件市场失败: {e}"))?
}

fn repair_openai_bundled_plugins_blocking(
    app: tauri::AppHandle,
) -> Result<ToolboxSnapshot, String> {
    auto_backup_configs(&app);
    let config_path = codex_config_path();
    let existing = fs::read_to_string(&config_path).unwrap_or_default();
    let next = remove_invalid_local_plugin_marketplace_sections(&existing);
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    write_file_atomic(&config_path, &next)
        .map_err(|e| format!("写入 openai-bundled 插件市场失败: {e}"))?;
    invalidate_plugin_marketplace_cache();
    Ok(build_toolbox_snapshot(&app))
}

#[tauri::command]
pub(crate) async fn enable_codex_builtin_plugin(
    app: tauri::AppHandle,
    plugin_id: String,
) -> Result<ToolboxSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || enable_codex_builtin_plugin_blocking(app, plugin_id))
        .await
        .map_err(|e| format!("启用内置插件失败: {e}"))?
}

fn enable_codex_builtin_plugin_blocking(
    app: tauri::AppHandle,
    plugin_id: String,
) -> Result<ToolboxSnapshot, String> {
    let normalized = plugin_id.trim().to_string();
    if normalized.is_empty() || normalized.contains('\n') || normalized.contains('\r') {
        return Err("非法插件 ID".into());
    }
    auto_backup_configs(&app);
    let config_path = codex_config_path();
    let existing = fs::read_to_string(&config_path).unwrap_or_default();
    let with_marketplaces = ensure_discovered_plugin_marketplaces_config(&existing)?;
    let next = write_enabled_codex_plugin_config(&with_marketplaces, &normalized);
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    write_file_atomic(&config_path, &next).map_err(|e| format!("启用 Codex 内置插件失败: {e}"))?;
    Ok(build_toolbox_snapshot(&app))
}

#[tauri::command]
pub(crate) async fn enable_important_codex_builtin_plugins(
    app: tauri::AppHandle,
) -> Result<ToolboxSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        enable_important_codex_builtin_plugins_blocking(app)
    })
    .await
    .map_err(|e| format!("批量启用内置插件失败: {e}"))?
}

fn enable_important_codex_builtin_plugins_blocking(
    app: tauri::AppHandle,
) -> Result<ToolboxSnapshot, String> {
    auto_backup_configs(&app);
    let config_path = codex_config_path();
    let existing = fs::read_to_string(&config_path).unwrap_or_default();
    let mut next = ensure_discovered_plugin_marketplaces_config(&existing)?;
    let status = list_codex_builtin_plugins(&next);
    if !status.available {
        return Err(status.last_error);
    }
    let important_ids = status
        .plugins
        .iter()
        .filter(|plugin| plugin.important)
        .map(|plugin| plugin.id.clone())
        .collect::<Vec<_>>();
    if important_ids.is_empty() {
        return Err("未发现 Computer Use / Chrome / Browser / Fast Speed 等关键内置插件".into());
    }
    for id in important_ids {
        next = write_enabled_codex_plugin_config(&next, &id);
    }
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    write_file_atomic(&config_path, &next)
        .map_err(|e| format!("启用关键 Codex 内置插件失败: {e}"))?;
    Ok(build_toolbox_snapshot(&app))
}

#[tauri::command]
pub(crate) async fn apply_plugin_marketplace(
    app: tauri::AppHandle,
    source: String,
) -> Result<ToolboxSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || apply_plugin_marketplace_blocking(app, source))
        .await
        .map_err(|e| format!("安装插件市场后台任务失败: {e}"))?
}

pub(crate) fn apply_plugin_marketplace_blocking(
    app: tauri::AppHandle,
    source: String,
) -> Result<ToolboxSnapshot, String> {
    let trimmed = supported_plugin_marketplace_source(&source);
    emit_plugin_marketplace_progress(&app, 1, "prepare");
    let config_path = codex_config_path();
    let existing = fs::read_to_string(&config_path).unwrap_or_default();

    if config_has_plugin_marketplace_source(&existing, trimmed)
        && plugin_marketplace_snapshot_exists(default_plugin_marketplace_name())
    {
        let mut state = read_toolbox_state(&app);
        state.plugin_marketplace_input = trimmed.to_string();
        write_toolbox_state(&app, &state)?;
        emit_plugin_marketplace_progress(&app, 6, "done");
        return Ok(build_toolbox_snapshot(&app));
    }

    let cleaned = remove_all_plugin_marketplace_sections(&existing);
    let parent = config_path
        .parent()
        .ok_or("Codex 配置目录不存在")?
        .to_path_buf();
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    write_file_atomic(&config_path, &cleaned)?;

    emit_plugin_marketplace_progress(&app, 2, "install");
    if let Err(error) = repair_and_add_codex_plugin_marketplace(trimmed) {
        let fallback_existing = fs::read_to_string(&config_path).unwrap_or_default();
        let fallback_next = ensure_plugin_marketplace_section(
            &fallback_existing,
            default_plugin_marketplace_name(),
            trimmed,
            plugin_marketplace_source_type(trimmed),
        );
        let _ = write_file_atomic(&config_path, &fallback_next);
        return Err(format!(
            "{error}\n已尝试自动修复 varswitch-plugins 来源冲突并写入 config.toml 作为兜底；请确认 Codex CLI 和 Git 可用后重试。"
        ));
    }

    emit_plugin_marketplace_progress(&app, 3, "install");
    emit_plugin_marketplace_progress(&app, 4, "verify");

    let updated = fs::read_to_string(&config_path).unwrap_or_default();
    if !config_has_plugin_marketplace_source(&updated, trimmed) {
        let fallback_next = ensure_plugin_marketplace_section(
            &updated,
            default_plugin_marketplace_name(),
            trimmed,
            plugin_marketplace_source_type(trimmed),
        );
        write_file_atomic(&config_path, &fallback_next)?;
    }

    let mut state = read_toolbox_state(&app);
    state.plugin_marketplace_input = trimmed.to_string();
    write_toolbox_state(&app, &state)?;
    emit_plugin_marketplace_progress(&app, 6, "done");
    invalidate_plugin_marketplace_cache();
    Ok(build_toolbox_snapshot(&app))
}

/// 要读最多 200 个完整会话文件，长会话下可达十数秒，必须离开主线程
#[tauri::command]
pub(crate) async fn sync_codex_sessions(app: tauri::AppHandle) -> Result<ToolboxSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let threads = read_codex_threads(200);
        let mut state = read_toolbox_state(&app);
        state.synced_codex_threads = threads;
        refresh_selected_mobile_thread_after_session_change(&mut state);
        state.session_sync = CodexSessionSyncState {
            last_synced_at: chrono_now(),
            total: visible_codex_threads(&state).len(),
        };
        write_toolbox_state(&app, &state)?;
        Ok(build_toolbox_snapshot(&app))
    })
    .await
    .map_err(|e| format!("同步 Codex 会话失败: {e}"))?
}

/// 回退到磁盘查找时要读最多 500 个会话文件，同样离开主线程
#[tauri::command]
pub(crate) async fn trash_codex_sessions(
    app: tauri::AppHandle,
    thread_ids: Vec<String>,
) -> Result<ToolboxSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || trash_codex_sessions_blocking(app, thread_ids))
        .await
        .map_err(|e| format!("移入回收站失败: {e}"))?
}

fn trash_codex_sessions_blocking(
    app: tauri::AppHandle,
    thread_ids: Vec<String>,
) -> Result<ToolboxSnapshot, String> {
    let requested: HashSet<String> = thread_ids
        .into_iter()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect();
    if requested.is_empty() {
        return Err("请至少选择一条会话".into());
    }

    let mut state = read_toolbox_state(&app);
    let mut available = HashMap::new();
    for thread in state
        .synced_codex_threads
        .iter()
        .cloned()
        .chain(read_codex_threads(500).into_iter())
    {
        available.entry(thread.id.clone()).or_insert(thread);
    }

    let deleted_at = chrono_now();
    let mut added = 0usize;
    let mut existing_trash = trashed_codex_thread_ids(&state);
    for id in requested {
        if existing_trash.contains(&id) {
            continue;
        }
        let Some(thread) = available.get(&id).cloned() else {
            continue;
        };
        state
            .trashed_codex_threads
            .push(codex_thread_to_trash_record(thread, &deleted_at));
        existing_trash.insert(id);
        added += 1;
    }

    if added == 0 {
        return Err("没有找到可移入回收站的会话".into());
    }

    refresh_selected_mobile_thread_after_session_change(&mut state);
    state.session_sync.total = visible_codex_threads(&state).len();
    write_toolbox_state(&app, &state)?;
    Ok(build_toolbox_snapshot(&app))
}

#[tauri::command]
pub(crate) async fn restore_codex_sessions(
    app: tauri::AppHandle,
    thread_ids: Vec<String>,
) -> Result<ToolboxSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || restore_codex_sessions_blocking(app, thread_ids))
        .await
        .map_err(|e| format!("恢复会话失败: {e}"))?
}

fn restore_codex_sessions_blocking(
    app: tauri::AppHandle,
    thread_ids: Vec<String>,
) -> Result<ToolboxSnapshot, String> {
    let requested: HashSet<String> = thread_ids
        .into_iter()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect();
    if requested.is_empty() {
        return Err("请至少选择一条待恢复会话".into());
    }

    let mut state = read_toolbox_state(&app);
    let mut restored = Vec::new();
    state.trashed_codex_threads.retain(|thread| {
        if requested.contains(&thread.id) {
            restored.push(trash_record_to_codex_thread(thread));
            false
        } else {
            true
        }
    });

    if restored.is_empty() {
        return Err("没有找到可恢复的会话".into());
    }

    let mut known_ids: HashSet<String> = state
        .synced_codex_threads
        .iter()
        .map(|thread| thread.id.clone())
        .collect();
    for thread in restored {
        if known_ids.insert(thread.id.clone()) {
            state.synced_codex_threads.push(thread);
        }
    }

    refresh_selected_mobile_thread_after_session_change(&mut state);
    state.session_sync.total = visible_codex_threads(&state).len();
    write_toolbox_state(&app, &state)?;
    Ok(build_toolbox_snapshot(&app))
}

#[tauri::command]
pub(crate) async fn select_mobile_thread(
    app: tauri::AppHandle,
    thread_id: String,
) -> Result<ToolboxSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || select_mobile_thread_blocking(app, thread_id))
        .await
        .map_err(|e| format!("选择会话失败: {e}"))?
}

fn select_mobile_thread_blocking(
    app: tauri::AppHandle,
    thread_id: String,
) -> Result<ToolboxSnapshot, String> {
    let normalized = thread_id.trim().to_string();
    if normalized.is_empty() {
        return Err("请选择要控制的会话".into());
    }
    let mut state = read_toolbox_state(&app);
    let threads = if state.synced_codex_threads.is_empty() {
        read_codex_threads(200)
    } else {
        state.synced_codex_threads.clone()
    };
    let thread = threads
        .into_iter()
        .find(|item| item.id == normalized)
        .ok_or("会话未找到，请先同步本地历史")?;
    state.selected_mobile_thread_id = thread.id.clone();
    state.mobile_remote.active_thread_id = thread.id;
    state.mobile_remote.active_thread_name = thread.thread_name;
    write_toolbox_state(&app, &state)?;
    Ok(build_toolbox_snapshot(&app))
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn configure_mobile_channel(
    app: tauri::AppHandle,
    channel: String,
    app_id: String,
    app_secret: String,
    bot_token: String,
    account_id: String,
    base_url: String,
    user_id: String,
    bot_open_id: String,
) -> Result<ToolboxSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        configure_mobile_channel_blocking(
            app,
            channel,
            app_id,
            app_secret,
            bot_token,
            account_id,
            base_url,
            user_id,
            bot_open_id,
        )
    })
    .await
    .map_err(|e| format!("保存手机通道配置失败: {e}"))?
}

#[allow(clippy::too_many_arguments)]
fn configure_mobile_channel_blocking(
    app: tauri::AppHandle,
    channel: String,
    app_id: String,
    app_secret: String,
    bot_token: String,
    account_id: String,
    base_url: String,
    user_id: String,
    bot_open_id: String,
) -> Result<ToolboxSnapshot, String> {
    let normalized_channel = normalize_mobile_channel(&channel);
    if !["lark", "wechat", "qq"].contains(&normalized_channel.as_str()) {
        return Err("不支持的手机通道".into());
    }
    let mut state = read_toolbox_state(&app);
    if !state
        .mobile_channels
        .iter()
        .any(|binding| binding.channel == normalized_channel)
    {
        state
            .mobile_channels
            .push(default_mobile_channel(&normalized_channel));
    }
    // B11: 保存旧凭据，用于判断是否需要重启运行中的通道
    let old_creds: (String, String, String) = state
        .mobile_channels
        .iter()
        .find(|b| b.channel == normalized_channel)
        .map(|b| (b.app_id.clone(), b.app_secret.clone(), b.bot_token.clone()))
        .unwrap_or_default();
    if let Some(binding) = state
        .mobile_channels
        .iter_mut()
        .find(|binding| binding.channel == normalized_channel)
    {
        binding.app_id = app_id.trim().to_string();
        binding.app_secret = app_secret.trim().to_string();
        binding.bot_token = bot_token.trim().to_string();
        binding.account_id = account_id.trim().to_string();
        binding.base_url = normalize_mobile_base_url(&base_url, &normalized_channel);
        binding.user_id = user_id.trim().to_string();
        binding.bot_open_id = bot_open_id.trim().to_string();
        binding.credential_status = if mobile_channel_has_credentials(binding) {
            "已保存平台凭据，等待开启连接".into()
        } else {
            mobile_channel_credential_hint(&normalized_channel).into()
        };
        binding.last_error.clear();
        binding.updated_at = chrono_now();
    }
    if state
        .mobile_channels
        .iter()
        .find(|binding| binding.channel == normalized_channel)
        .map(mobile_channel_has_credentials)
        .unwrap_or(false)
    {
        attach_selected_thread_to_mobile_channel(&mut state, &normalized_channel);
    }
    write_toolbox_state(&app, &state)?;
    // B11: 如果凭据变化且通道正在运行，停止旧连接（看门狗会用新凭据重启）
    let new_creds: (String, String, String) = state
        .mobile_channels
        .iter()
        .find(|b| b.channel == normalized_channel)
        .map(|b| (b.app_id.clone(), b.app_secret.clone(), b.bot_token.clone()))
        .unwrap_or_default();
    if old_creds != new_creds {
        match normalized_channel.as_str() {
            "lark" if LARK_BRIDGE_ACTIVE.load(Ordering::SeqCst) => {
                log_info!("[mobile-control][configure] 飞书凭据变更，停止旧连接");
                stop_lark_bridge();
            }
            "qq" if QQ_GATEWAY_ACTIVE.load(Ordering::SeqCst) => {
                log_info!("[mobile-control][configure] QQ 凭据变更，停止旧连接");
                stop_qq_gateway();
            }
            "wechat" if WECHAT_LISTENER_ACTIVE.load(Ordering::SeqCst) => {
                log_info!("[mobile-control][configure] 微信凭据变更，停止旧连接");
                stop_wechat_listener(&app);
            }
            _ => {}
        }
    }
    Ok(build_toolbox_snapshot(&app))
}

// B4: 改 async + spawn_blocking，注册请求含网络往返，同步执行会卡 UI
#[tauri::command]
pub(crate) async fn start_lark_bot_registration(
    app: tauri::AppHandle,
    create_only: Option<bool>,
) -> Result<ToolboxSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        start_lark_bot_registration_blocking(app, create_only)
    })
    .await
    .map_err(|e| format!("飞书注册后台任务失败: {e}"))?
}

pub(crate) fn start_lark_bot_registration_blocking(
    app: tauri::AppHandle,
    create_only: Option<bool>,
) -> Result<ToolboxSnapshot, String> {
    // 新点击直接取代旧流程：先递增代际，旧轮询 worker 在下一轮检查时自行退出。
    // 此前这里用 ACTIVE 标志静默早退——旧 worker 可存活十来分钟，期间“创建/换绑”
    // 按钮点了没任何反应（前端还提示成功），表现为按钮大面积“失败”。
    let my_generation = LARK_REGISTRATION_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    LARK_REGISTRATION_ACTIVE.store(true, Ordering::SeqCst);
    let create_only = create_only.unwrap_or(true);
    let existing_app_id = read_toolbox_state(&app)
        .mobile_channels
        .iter()
        .find(|binding| binding.channel == "lark")
        .map(|binding| binding.app_id.clone())
        .unwrap_or_default();

    let client = match build_http_client(15) {
        Ok(client) => client,
        Err(error) => {
            LARK_REGISTRATION_ACTIVE.store(false, Ordering::SeqCst);
            return Err(error);
        }
    };
    let (device_code, launcher_url, interval) =
        match request_lark_registration(&client, create_only, &existing_app_id) {
            Ok(result) => result,
            Err(error) => {
                LARK_REGISTRATION_ACTIVE.store(false, Ordering::SeqCst);
                return Err(error);
            }
        };
    if let Err(error) = open_with_system(&launcher_url) {
        LARK_REGISTRATION_ACTIVE.store(false, Ordering::SeqCst);
        return Err(error);
    }
    let mut state = read_toolbox_state(&app);
    if let Some(binding) = state
        .mobile_channels
        .iter_mut()
        .find(|binding| binding.channel == "lark")
    {
        binding.base_url = default_mobile_channel_base_url("lark").into();
        binding.launcher_url = launcher_url;
        binding.qr_url = binding.launcher_url.clone();
        binding.qr_device_code = device_code.clone();
        binding.qr_status = if create_only {
            "已打开飞书创建页面，建议命名为 VarSwitch 智能体，创建完成后会自动填充 AppID/AppSecret"
                .into()
        } else {
            "已打开飞书换绑页面，完成授权后会自动读取 AppID/AppSecret".into()
        };
        binding.credential_status = "等待飞书创建/授权确认".into();
        binding.status = if create_only {
            "等待飞书机器人创建完成".into()
        } else {
            "等待飞书已有机器人授权完成".into()
        };
        binding.last_error.clear();
        binding.qr_started_at = chrono_timestamp_millis().to_string();
        binding.updated_at = chrono_now();
    }
    write_toolbox_state(&app, &state)?;
    start_lark_registration_poll_worker(app.clone(), device_code, interval, my_generation);
    Ok(build_toolbox_snapshot(&app))
}

// B4: 命令改 async + spawn_blocking，避免 15 秒阻塞 POST 卡死主线程（前端每秒调一次）
#[tauri::command]
pub(crate) async fn poll_lark_bot_registration(app: tauri::AppHandle) -> Result<ToolboxSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || poll_lark_bot_registration_blocking(app))
        .await
        .map_err(|e| format!("飞书注册轮询后台任务失败: {e}"))?
}

pub(crate) fn poll_lark_bot_registration_blocking(app: tauri::AppHandle) -> Result<ToolboxSnapshot, String> {
    let state = read_toolbox_state(&app);
    let binding = state
        .mobile_channels
        .iter()
        .find(|binding| binding.channel == "lark")
        .cloned()
        .ok_or("飞书通道不存在")?;
    if binding.qr_device_code.trim().is_empty() {
        return Ok(build_toolbox_snapshot(&app));
    }
    let client = build_http_client(15)?;
    let poll = match poll_lark_registration_device(&client, &binding.qr_device_code) {
        Ok(poll) => poll,
        Err(error) => {
            update_channel_status(&app, "lark", "飞书自动读取 AppID/AppSecret 失败", &error);
            return Ok(build_toolbox_snapshot(&app));
        }
    };
    apply_lark_registration_poll(&app, poll)
}

#[tauri::command]
pub(crate) async fn open_lark_bot_launcher(app: tauri::AppHandle) -> Result<ToolboxSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || open_lark_bot_launcher_blocking(app))
        .await
        .map_err(|e| format!("飞书启动器后台任务失败: {e}"))?
}

pub(crate) fn open_lark_bot_launcher_blocking(app: tauri::AppHandle) -> Result<ToolboxSnapshot, String> {
    // B4: start_lark_bot_registration 已改 async，这里调用同步的 blocking 版本
    match start_lark_bot_registration_blocking(app.clone(), Some(true)) {
        Ok(snapshot) => Ok(snapshot),
        Err(_) => {
            let url = lark_create_bot_launcher_url();
            open_with_system(url)?;
            let mut state = read_toolbox_state(&app);
            if let Some(binding) = state
                .mobile_channels
                .iter_mut()
                .find(|binding| binding.channel == "lark")
            {
                binding.launcher_url = url.into();
                binding.qr_url = url.into();
                binding.qr_status =
                    "已打开飞书创建页面；当前网络未能启动自动填充，请稍后重试自动创建".into();
                binding.updated_at = chrono_now();
            }
            write_toolbox_state(&app, &state)?;
            Ok(build_toolbox_snapshot(&app))
        }
    }
}

#[tauri::command]
pub(crate) async fn clear_mobile_channel_binding(
    app: tauri::AppHandle,
    channel: String,
) -> Result<ToolboxSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        clear_mobile_channel_binding_blocking(app, channel)
    })
    .await
    .map_err(|e| format!("清除手机通道绑定后台任务失败: {e}"))?
}

pub(crate) fn clear_mobile_channel_binding_blocking(
    app: tauri::AppHandle,
    channel: String,
) -> Result<ToolboxSnapshot, String> {
    let normalized_channel = normalize_mobile_channel(&channel);
    match normalized_channel.as_str() {
        "lark" => {
            // B13: 清除绑定后使正在运行的注册轮询立刻失效，避免旧 worker 回写凭据。
            LARK_REGISTRATION_GENERATION.fetch_add(1, Ordering::SeqCst);
            LARK_REGISTRATION_ACTIVE.store(false, Ordering::SeqCst);
            stop_lark_bridge();
        }
        "qq" => {
            stop_qq_gateway();
            cancel_qq_qr_binding_process();
        }
        "wechat" => stop_wechat_listener(&app),
        _ => {}
    }
    let mut state = read_toolbox_state(&app);
    state
        .session_bindings
        .retain(|binding| normalize_mobile_channel(&binding.channel) != normalized_channel);
    if let Some(binding) = state
        .mobile_channels
        .iter_mut()
        .find(|binding| binding.channel == normalized_channel)
    {
        let base_url = default_mobile_channel_base_url(&normalized_channel).to_string();
        *binding = MobileChannelBinding {
            channel: normalized_channel.clone(),
            status: "未绑定".into(),
            base_url,
            updated_at: chrono_now(),
            ..MobileChannelBinding::default()
        };
    }
    write_toolbox_state(&app, &state)?;
    Ok(build_toolbox_snapshot(&app))
}

pub(crate) fn cancel_qq_qr_binding_process() {
    let _state_guard = match QQ_QR_STATE_LOCK.lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    QQ_QR_GENERATION.fetch_add(1, Ordering::SeqCst);
    QQ_QR_ACTIVE.store(false, Ordering::SeqCst);
    if let Ok(mut guard) = QQ_QR_CHILD.lock() {
        if let Some(mut child) = guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub(crate) fn write_qq_qr_state_for_generation(
    app: &tauri::AppHandle,
    generation: u64,
    status: &str,
    qr_url: &str,
    qr_data_url: &str,
    error: &str,
) -> bool {
    let Ok(_state_guard) = QQ_QR_STATE_LOCK.lock() else {
        return false;
    };
    if QQ_QR_GENERATION.load(Ordering::SeqCst) != generation {
        return false;
    }
    let _ = write_mobile_channel_qr_state(app, "qq", status, qr_url, qr_data_url, error);
    true
}

pub(crate) fn save_qq_qr_credentials_for_generation(
    app: &tauri::AppHandle,
    generation: u64,
    app_id: &str,
    app_secret: &str,
) -> bool {
    let Ok(_state_guard) = QQ_QR_STATE_LOCK.lock() else {
        return false;
    };
    if QQ_QR_GENERATION.load(Ordering::SeqCst) != generation {
        return false;
    }
    let _ =
        update_mobile_channel_credentials_from_qr(app, "qq", app_id, app_secret, "", "", "", "");
    true
}

pub(crate) fn finish_qq_qr_generation(generation: u64) {
    let Ok(_state_guard) = QQ_QR_STATE_LOCK.lock() else {
        return;
    };
    if QQ_QR_GENERATION.load(Ordering::SeqCst) != generation {
        return;
    }
    if let Ok(mut guard) = QQ_QR_CHILD.lock() {
        if let Some(mut child) = guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    QQ_QR_ACTIVE.store(false, Ordering::SeqCst);
}

#[tauri::command]
pub(crate) async fn cancel_qq_qr_binding(app: tauri::AppHandle) -> Result<ToolboxSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        cancel_qq_qr_binding_process();
        let _ = write_mobile_channel_qr_state(&app, "qq", "已取消 QQ 扫码绑定", "", "", "");
        Ok(build_toolbox_snapshot(&app))
    })
    .await
    .map_err(|e| format!("取消 QQ 扫码后台任务失败: {e}"))?
}

#[tauri::command]
pub(crate) async fn start_qq_qr_binding(app: tauri::AppHandle) -> Result<ToolboxSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || start_qq_qr_binding_blocking(app))
        .await
        .map_err(|e| format!("启动 QQ 扫码后台任务失败: {e}"))?
}

pub(crate) fn start_qq_qr_binding_blocking(app: tauri::AppHandle) -> Result<ToolboxSnapshot, String> {
    if QQ_QR_ACTIVE.swap(true, Ordering::SeqCst) {
        // 保留当前二维码和状态；重复点击不能把已经生成的二维码清空。
        return Ok(build_toolbox_snapshot(&app));
    }

    let generation = QQ_QR_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let _ = write_mobile_channel_qr_state(&app, "qq", "正在准备 QQ 扫码绑定服务...", "", "", "");
    let app_for_thread = app.clone();
    std::thread::spawn(move || {
        let runner = match ensure_qq_qr_connector(&app_for_thread) {
            Ok(path) => path,
            Err(error) => {
                write_qq_qr_state_for_generation(
                    &app_for_thread,
                    generation,
                    "QQ 扫码绑定失败",
                    "",
                    "",
                    &error,
                );
                finish_qq_qr_generation(generation);
                return;
            }
        };
        let node = match node_command_name() {
            Ok(command) => command,
            Err(error) => {
                write_qq_qr_state_for_generation(
                    &app_for_thread,
                    generation,
                    "QQ 扫码绑定失败",
                    "",
                    "",
                    &error,
                );
                finish_qq_qr_generation(generation);
                return;
            }
        };
        let connector_dir = match runner.parent() {
            Some(parent) => parent.to_path_buf(),
            None => {
                write_qq_qr_state_for_generation(
                    &app_for_thread,
                    generation,
                    "QQ 扫码绑定失败",
                    "",
                    "",
                    "QQ 扫码运行目录不存在",
                );
                finish_qq_qr_generation(generation);
                return;
            }
        };
        let mut cmd = Command::new(node);
        cmd.arg(&runner)
            .current_dir(connector_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(target_os = "windows")]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(error) => {
                write_qq_qr_state_for_generation(
                    &app_for_thread,
                    generation,
                    "QQ 扫码绑定失败",
                    "",
                    "",
                    &format!("启动 QQ 扫码服务失败: {error}"),
                );
                finish_qq_qr_generation(generation);
                return;
            }
        };
        let stdout = match child.stdout.take() {
            Some(stdout) => stdout,
            None => {
                write_qq_qr_state_for_generation(
                    &app_for_thread,
                    generation,
                    "QQ 扫码绑定失败",
                    "",
                    "",
                    "QQ 扫码服务没有输出通道",
                );
                let _ = child.kill();
                let _ = child.wait();
                finish_qq_qr_generation(generation);
                return;
            }
        };
        // B1: 消费 stderr，防止缓冲区满后 write 阻塞造成进程假死
        if let Some(stderr) = child.stderr.take() {
            std::thread::spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines().flatten() {
                    log_info!("[mobile-control][qq-qr][stderr] {}", line);
                }
            });
        }
        // 取消可能发生在进程启动和保存句柄之间。过期轮次自行回收，不能覆盖新轮次句柄。
        // B2: 保存子进程句柄，方便外部取消；同时加 5 分钟总超时。
        // 与取消操作共用状态锁，防止取消刚发生又写回新的子进程句柄。
        let mut child = Some(child);
        let keep_child = if let Ok(_state_guard) = QQ_QR_STATE_LOCK.lock() {
            if QQ_QR_GENERATION.load(Ordering::SeqCst) == generation {
                if let Ok(mut guard) = QQ_QR_CHILD.lock() {
                    *guard = child.take();
                    true
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };
        if !keep_child {
            if let Some(mut child) = child {
                let _ = child.kill();
                let _ = child.wait();
            }
            return;
        }
        let (line_tx, line_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().flatten() {
                if line_tx.send(line).is_err() {
                    break;
                }
            }
        });
        let deadline = Instant::now() + Duration::from_secs(300);
        loop {
            if QQ_QR_GENERATION.load(Ordering::SeqCst) != generation {
                break;
            }
            if std::time::Instant::now() > deadline {
                write_qq_qr_state_for_generation(
                    &app_for_thread,
                    generation,
                    "QQ 扫码已超时（5 分钟），请重新尝试",
                    "",
                    "",
                    "扫码总超时",
                );
                break;
            }
            let line = match line_rx.recv_timeout(Duration::from_secs(1)) {
                Ok(line) => line,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            };
            let Ok(payload) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            let event_type = json_string(&payload, &["type"]);
            match event_type.as_str() {
                "qr" => {
                    let url = json_string(&payload, &["url"]);
                    let mut data_url = json_string(&payload, &["dataUrl", "data_url"]);
                    // QQ connector 应返回 data:image/ 格式二维码，但若失败或返回 URL，用本地生成兜底。
                    if !data_url.starts_with("data:image/") && !url.is_empty() {
                        data_url = generate_qr_code_data_url(&url).unwrap_or_default();
                    }
                    if data_url.is_empty() || !data_url.starts_with("data:image/") {
                        write_qq_qr_state_for_generation(
                            &app_for_thread,
                            generation,
                            "QQ 扫码绑定失败",
                            "",
                            "",
                            "QQ connector 未返回可用二维码且本地生成失败",
                        );
                        break;
                    }
                    write_qq_qr_state_for_generation(
                        &app_for_thread,
                        generation,
                        "请用 QQ 扫描二维码完成机器人绑定",
                        &url,
                        &data_url,
                        "",
                    );
                }
                "success" => {
                    let credentials_raw = payload
                        .get("credentials")
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!({}));

                    // QQ connector 返回的 credentials 是数组 [{"appId":"...","appSecret":"..."}]，取第一个元素
                    let credentials = if credentials_raw.is_array() {
                        credentials_raw
                            .get(0)
                            .cloned()
                            .unwrap_or_else(|| serde_json::json!({}))
                    } else {
                        credentials_raw
                    };

                    let app_id = json_string(&credentials, &["appId", "appid", "app_id", "id"]);
                    let app_secret = json_string(
                        &credentials,
                        &[
                            "appSecret",
                            "appsecret",
                            "app_secret",
                            "clientSecret",
                            "client_secret",
                        ],
                    );
                    save_qq_qr_credentials_for_generation(
                        &app_for_thread,
                        generation,
                        &app_id,
                        &app_secret,
                    );
                    break;
                }
                "expired" => {
                    write_qq_qr_state_for_generation(
                        &app_for_thread,
                        generation,
                        "二维码已过期，请重新绑定",
                        "",
                        "",
                        "QQ 扫码二维码已过期",
                    );
                    break;
                }
                "failure" => {
                    let message = json_string(&payload, &["message"]);
                    write_qq_qr_state_for_generation(
                        &app_for_thread,
                        generation,
                        "QQ 扫码绑定失败",
                        "",
                        "",
                        &message,
                    );
                    break;
                }
                _ => {}
            }
        }
        // B2: 无论正常结束/超时/expired，都先 kill 子进程，再清 ACTIVE
        // 这样 child.wait() 不会永久阻塞（runner 过期后本身不退出）
        finish_qq_qr_generation(generation);
    });

    Ok(build_toolbox_snapshot(&app))
}

pub(crate) fn wechat_qr_data_url_from_content(value: &str) -> String {
    let raw = value.trim();
    if raw.starts_with("data:image/") {
        raw.to_string()
    } else {
        let compact = raw.replace(['\r', '\n', ' '], "");
        let image_mime = if compact.starts_with("iVBORw0KGgo") {
            Some("image/png")
        } else if compact.starts_with("/9j/") {
            Some("image/jpeg")
        } else if compact.starts_with("R0lGOD") {
            Some("image/gif")
        } else if compact.starts_with("UklGR") {
            Some("image/webp")
        } else {
            None
        };
        image_mime
            .map(|mime| format!("data:{mime};base64,{compact}"))
            .unwrap_or_default()
    }
}

pub(crate) fn request_wechat_qr_binding(app: &tauri::AppHandle) -> Result<(), String> {
    let client = build_http_client(15)?;
    let base_url = default_mobile_channel_base_url("wechat");
    let url = format!("{base_url}/ilink/bot/get_bot_qrcode?bot_type=3");
    let response = client
        .post(&url)
        .json(&serde_json::json!({ "local_token_list": [] }))
        .send()
        .map_err(|e| format!("微信二维码请求失败: {e}"))?;
    let status = response.status();
    let body = response
        .text()
        .map_err(|e| format!("微信二维码响应读取失败: {e}"))?;
    if !status.is_success() {
        return Err(format!(
            "微信二维码接口返回 HTTP {}：{}",
            status.as_u16(),
            body
        ));
    }
    let result: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("微信二维码返回不是 JSON: {e}; {body}"))?;
    if !is_platform_code_ok(&result) {
        return Err(format!(
            "微信二维码生成失败：{}",
            platform_error_message(&result, "接口返回异常")
        ));
    }
    let qrcode = json_string(&result, &["qrcode"]);
    let image_content = json_string(&result, &["qrcode_img_content", "qrcodeImageContent"]);
    let qr_url = json_string(&result, &["qrcode_url", "qrcodeUrl", "url"]);
    if qrcode.is_empty() {
        return Err(format!("微信二维码接口未返回绑定码：{result}"));
    }

    let mut qr_data_url = wechat_qr_data_url_from_content(&image_content);
    if qr_data_url.is_empty()
        && (image_content.starts_with("http://") || image_content.starts_with("https://"))
    {
        // 微信 iLink 返回的 qrcode_img_content 是个 liteapp 短链(HTML页面),不是真图片。
        // 用本地 qrcode crate 把这个 URL 编码成二维码图片,用户扫短链即可绑定。
        qr_data_url = generate_qr_code_data_url(&image_content).unwrap_or_default();
    }
    if qr_data_url.is_empty() && (qr_url.starts_with("http://") || qr_url.starts_with("https://")) {
        qr_data_url = generate_qr_code_data_url(&qr_url).unwrap_or_default();
    }
    if qr_data_url
        .trim()
        .to_ascii_lowercase()
        .starts_with("data:image/svg+xml")
    {
        qr_data_url.clear();
    }
    if qr_data_url.is_empty() {
        return Err("微信接口没有返回真实可扫码二维码图片，已拒绝展示文本二维码；请重新生成或检查 iLink 接口返回".into());
    }

    let mut state = read_toolbox_state(app);
    if let Some(binding) = state
        .mobile_channels
        .iter_mut()
        .find(|binding| binding.channel == "wechat")
    {
        binding.qr_url.clear();
        binding.qr_data_url = qr_data_url.clone();
        binding.qr_device_code = qrcode;
        binding.qr_status = "请用手机微信扫描二维码，并在手机上确认绑定".into();
        binding.base_url = base_url.into();
        binding.qr_started_at = chrono_now();
        binding.updated_at = chrono_now();
        binding.last_error.clear();
    }
    write_toolbox_state(app, &state)
}

#[tauri::command]
pub(crate) fn start_wechat_qr_binding(app: tauri::AppHandle) -> Result<ToolboxSnapshot, String> {
    if WECHAT_QR_ACTIVE.swap(true, Ordering::SeqCst) {
        let _ = write_mobile_channel_qr_state(
            &app,
            "wechat",
            "微信二维码正在生成中，请稍等",
            "",
            "",
            "",
        );
        return Ok(build_toolbox_snapshot(&app));
    }

    let _ = write_mobile_channel_qr_state(&app, "wechat", "正在请求微信绑定二维码...", "", "", "");
    let app_for_thread = app.clone();
    std::thread::spawn(move || {
        if let Err(error) = request_wechat_qr_binding(&app_for_thread) {
            let _ = write_mobile_channel_qr_state(
                &app_for_thread,
                "wechat",
                "微信二维码生成失败",
                "",
                "",
                &error,
            );
        }
        WECHAT_QR_ACTIVE.store(false, Ordering::SeqCst);
    });
    Ok(build_toolbox_snapshot(&app))
}

// B4: 命令改 async + spawn_blocking，避免 35 秒阻塞 GET 卡死主线程（前端每秒调一次）
#[tauri::command]
pub(crate) async fn poll_wechat_qr_binding(
    app: tauri::AppHandle,
    verify_code: Option<String>,
) -> Result<ToolboxSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || poll_wechat_qr_binding_blocking(app, verify_code))
        .await
        .map_err(|e| format!("微信扫码轮询后台任务失败: {e}"))?
}

pub(crate) fn poll_wechat_qr_binding_blocking(
    app: tauri::AppHandle,
    verify_code: Option<String>,
) -> Result<ToolboxSnapshot, String> {
    let state = read_toolbox_state(&app);
    let binding = state
        .mobile_channels
        .iter()
        .find(|binding| binding.channel == "wechat")
        .cloned()
        .ok_or("微信通道不存在")?;
    if binding.qr_device_code.trim().is_empty() {
        return Err("请先生成微信绑定二维码".into());
    }
    let client = build_http_client(35)?;
    let mut url = format!(
        "{}/ilink/bot/get_qrcode_status?qrcode={}",
        normalize_mobile_base_url(&binding.base_url, "wechat"),
        percent_encode_query_value(&binding.qr_device_code)
    );
    if let Some(code) = verify_code {
        if !code.trim().is_empty() {
            url.push_str("&verify_code=");
            url.push_str(&percent_encode_query_value(code.trim()));
        }
    }
    let response = client
        .get(&url)
        .send()
        .map_err(|e| format!("微信扫码状态查询失败: {e}"))?;
    let status = response.status();
    let body = response
        .text()
        .map_err(|e| format!("微信扫码状态响应读取失败: {e}"))?;
    if !status.is_success() {
        return Err(format!(
            "微信扫码状态返回 HTTP {}：{}",
            status.as_u16(),
            body
        ));
    }
    let result: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| format!("微信扫码状态返回不是 JSON: {e}; {body}"))?;
    let qr_status = json_string(&result, &["status"]);
    if qr_status == "confirmed" {
        let bot_token = json_string(&result, &["bot_token"]);
        let account_id = json_string(&result, &["ilink_bot_id", "account_id"]);
        let base_url = json_string(&result, &["baseurl", "base_url"]);
        let user_id = json_string(&result, &["ilink_user_id", "user_id"]);
        return update_mobile_channel_credentials_from_qr(
            &app,
            "wechat",
            "",
            "",
            &bot_token,
            &account_id,
            &base_url,
            &user_id,
        );
    }
    let message = match qr_status.as_str() {
        "scaned" => "微信已扫码，请在手机上确认",
        "need_verifycode" => "微信需要验证码，请按手机提示输入验证码",
        "expired" => "微信二维码已过期，请重新生成",
        "binded_redirect" => "该微信机器人已绑定过，请重新生成或换绑",
        _ => "等待微信扫码确认",
    };
    write_mobile_channel_qr_state(
        &app,
        "wechat",
        message,
        &binding.qr_url,
        &binding.qr_data_url,
        "",
    )
}

#[tauri::command]
pub(crate) async fn bind_codex_thread(
    app: tauri::AppHandle,
    channel: String,
    thread_id: String,
    sync_enabled: bool,
    note: Option<String>,
) -> Result<ToolboxSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        bind_codex_thread_blocking(app, channel, thread_id, sync_enabled, note)
    })
    .await
    .map_err(|e| format!("绑定会话失败: {e}"))?
}

fn bind_codex_thread_blocking(
    app: tauri::AppHandle,
    channel: String,
    thread_id: String,
    sync_enabled: bool,
    note: Option<String>,
) -> Result<ToolboxSnapshot, String> {
    let normalized_channel = normalize_mobile_channel(&channel);
    log_info!(
        "[mobile-control][bind] request: channel={}, normalized={}, thread_id={}, sync_enabled={}, note={}",
        channel,
        normalized_channel,
        thread_id,
        sync_enabled,
        note.as_deref().unwrap_or("")
    );
    if normalized_channel.is_empty() {
        log_error!("[mobile-control][bind] failed: empty channel");
        return Err("绑定通道不能为空".into());
    }
    let state = read_toolbox_state(&app);
    let thread_pool = if state.synced_codex_threads.is_empty() {
        read_codex_threads(200)
    } else {
        state.synced_codex_threads.clone()
    };
    let thread = thread_pool
        .into_iter()
        .find(|item| item.id == thread_id)
        .ok_or("会话未找到")?;
    log_info!(
        "[mobile-control][bind] found thread: channel={}, thread_id={}, thread_name={}, session_file={}, cwd={}",
        normalized_channel,
        thread.id,
        thread.thread_name,
        thread.session_file,
        thread.cwd
    );
    let mut state = state;
    let thread_id_value = thread.id.clone();
    let thread_name_value = thread.thread_name.clone();
    let session_file_value = thread.session_file.clone();
    let updated_at_value = thread.updated_at.clone();
    let cwd_value = thread.cwd.clone();
    state
        .session_bindings
        .retain(|binding| normalize_mobile_channel(&binding.channel) != normalized_channel);
    state.session_bindings.push(CodexSessionBinding {
        channel: normalized_channel.clone(),
        thread_id: thread_id_value.clone(),
        thread_name: thread_name_value.clone(),
        session_file: session_file_value.clone(),
        updated_at: updated_at_value,
        cwd: cwd_value,
        sync_enabled,
        last_synced_at: String::new(),
        note: note.unwrap_or_default().trim().to_string(),
    });
    if let Some(binding) = state
        .mobile_channels
        .iter_mut()
        .find(|binding| binding.channel == normalized_channel)
    {
        binding.thread_id = thread_id_value.clone();
        binding.thread_name = thread_name_value.clone();
        binding.session_file = session_file_value.clone();
        binding.enabled = true;
        binding.status = "已绑定，等待开启平台连接".into();
        binding.last_error.clear();
        binding.updated_at = chrono_now();
    } else {
        let mut mobile_binding = default_mobile_channel(&normalized_channel);
        mobile_binding.thread_id = thread_id_value.clone();
        mobile_binding.thread_name = thread_name_value.clone();
        mobile_binding.session_file = session_file_value;
        mobile_binding.enabled = true;
        mobile_binding.status = "已绑定，等待开启平台连接".into();
        mobile_binding.updated_at = chrono_now();
        state.mobile_channels.push(mobile_binding);
    }
    state.selected_mobile_thread_id = thread_id_value.clone();
    state.mobile_remote.active_thread_id = thread_id_value;
    state.mobile_remote.active_thread_name = thread_name_value;
    write_toolbox_state(&app, &state)?;
    log_info!(
        "[mobile-control][bind] saved: channel={}, thread_id={}",
        normalized_channel,
        state.mobile_remote.active_thread_id
    );
    Ok(build_toolbox_snapshot(&app))
}

#[tauri::command]
pub(crate) async fn unbind_codex_thread(
    app: tauri::AppHandle,
    channel: String,
) -> Result<ToolboxSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || unbind_codex_thread_blocking(app, channel))
        .await
        .map_err(|e| format!("解绑手机会话后台任务失败: {e}"))?
}

pub(crate) fn unbind_codex_thread_blocking(
    app: tauri::AppHandle,
    channel: String,
) -> Result<ToolboxSnapshot, String> {
    let normalized_channel = normalize_mobile_channel(&channel);
    log_info!(
        "[mobile-control][unbind] request: channel={}, normalized={}",
        channel,
        normalized_channel
    );
    let mut state = read_toolbox_state(&app);
    state
        .session_bindings
        .retain(|binding| normalize_mobile_channel(&binding.channel) != normalized_channel);
    if let Some(binding) = state
        .mobile_channels
        .iter_mut()
        .find(|binding| binding.channel == normalized_channel)
    {
        binding.thread_id.clear();
        binding.thread_name.clear();
        binding.session_file.clear();
        binding.enabled = false;
        binding.listening = false;
        binding.status = "未绑定".into();
        binding.last_error.clear();
        binding.updated_at = chrono_now();
    }
    write_toolbox_state(&app, &state)?;
    // B7: 解绑时停止对应平台通道，确保手机侧消息不再驱动本机 Codex
    match normalized_channel.as_str() {
        "lark" => stop_lark_bridge(),
        "qq" => stop_qq_gateway(),
        "wechat" => stop_wechat_listener(&app),
        _ => {}
    }
    log_info!(
        "[mobile-control][unbind] stopped channel and saved: channel={}",
        normalized_channel
    );
    Ok(build_toolbox_snapshot(&app))
}

#[tauri::command]
pub(crate) async fn start_mobile_remote(
    app: tauri::AppHandle,
    _state: State<'_, AppState>,
    port: Option<u16>,
) -> Result<ToolboxSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || start_mobile_remote_blocking(app, port))
        .await
        .map_err(|e| format!("启动手机连接后台任务失败: {e}"))?
}

pub(crate) fn start_mobile_remote_blocking(
    app: tauri::AppHandle,
    port: Option<u16>,
) -> Result<ToolboxSnapshot, String> {
    let _ = port;
    // B4: 通道 probe 等重活已在 run_mobile_remote_start 的后台线程里执行，
    //     这里只做 state 写入和 120ms 探测等待，不阻塞长时间网络往返
    log_info!("[mobile-control][start] start_mobile_remote requested");
    MOBILE_REMOTE_CANCEL_REQUESTED.store(false, Ordering::SeqCst);
    let already_starting = MOBILE_REMOTE_START_ACTIVE.swap(true, Ordering::SeqCst);
    let mut toolbox_state = read_toolbox_state(&app);
    toolbox_state.mobile_remote.enabled = true;
    toolbox_state.mobile_remote.mode = "platform_bot".into();
    toolbox_state.mobile_remote.listen_addr = "platform".into();
    toolbox_state.mobile_remote.last_started_at = chrono_now();
    toolbox_state.mobile_remote.last_error.clear();
    toolbox_state.mobile_remote.device_name = local_device_name();
    toolbox_state.mobile_remote.local_ip = detect_local_ip();
    start_smart_control_server(
        app.clone(),
        toolbox_state
            .mobile_remote
            .remote_control_backend_url
            .clone(),
    );
    std::thread::sleep(Duration::from_millis(120));
    let smart_status = refresh_smart_control_status_for_state(&mut toolbox_state);
    log_info!(
        "[mobile-control][smart-control] start probe: connected={}, status={}, detail={}",
        smart_status.connected,
        smart_status.status,
        smart_status.detail
    );
    for channel in toolbox_state.mobile_channels.iter_mut() {
        if channel.enabled && !channel.thread_id.trim().is_empty() {
            channel.listening = false;
            channel.status = "正在连接平台...".into();
            channel.last_error.clear();
            channel.updated_at = chrono_now();
        }
    }
    write_toolbox_state(&app, &toolbox_state)?;
    if !already_starting {
        log_info!("[mobile-control][start] spawning platform connection worker");
        let app_for_thread = app.clone();
        std::thread::spawn(move || run_mobile_remote_start(app_for_thread));
    } else {
        log_info!("[mobile-control][start] worker already starting, skip spawn");
    }
    Ok(build_toolbox_snapshot(&app))
}

pub(crate) fn run_mobile_remote_start(app: tauri::AppHandle) {
    log_info!("[mobile-control][start-worker] begin");
    let mut toolbox_state = read_toolbox_state(&app);
    let mut connected_count = 0usize;
    let mut last_error = String::new();
    let mut lark_bridge_binding: Option<MobileChannelBinding> = None;
    let mut qq_gateway_binding: Option<MobileChannelBinding> = None;
    let mut wechat_listener_binding: Option<MobileChannelBinding> = None;
    for channel in toolbox_state.mobile_channels.iter_mut() {
        if MOBILE_REMOTE_CANCEL_REQUESTED.load(Ordering::SeqCst) {
            log_info!("[mobile-control][start-worker] cancelled before channel probe");
            MOBILE_REMOTE_START_ACTIVE.store(false, Ordering::SeqCst);
            return;
        }
        if channel.enabled && !channel.thread_id.trim().is_empty() {
            log_info!(
                "[mobile-control][start-worker] probing channel: channel={}, thread_id={}, thread_name={}",
                channel.channel, channel.thread_id, channel.thread_name
            );
            if !mobile_channel_has_credentials(channel) {
                channel.listening = false;
                channel.credential_status = mobile_channel_credential_hint(&channel.channel).into();
                channel.last_error = channel.credential_status.clone();
                channel.status = "缺少平台凭据，无法开启连接".into();
                channel.updated_at = chrono_now();
                last_error = channel.last_error.clone();
                log_info!(
                    "[mobile-control][start-worker] missing credentials: channel={}, error={}",
                    channel.channel,
                    channel.last_error
                );
                continue;
            }
            match probe_mobile_channel(channel) {
                Ok((gateway_url, status)) => {
                    log_info!(
                        "[mobile-control][start-worker] probe ok: channel={}, gateway={}, status={}",
                        channel.channel, gateway_url, status
                    );
                    connected_count += 1;
                    channel.gateway_url = gateway_url;
                    channel.listening = true;
                    channel.status = status;
                    channel.credential_status = "平台凭据已验证".into();
                    channel.last_error.clear();
                    channel.last_checked_at = chrono_now();
                    channel.updated_at = chrono_now();
                    match channel.channel.as_str() {
                        "lark" => lark_bridge_binding = Some(channel.clone()),
                        "qq" => qq_gateway_binding = Some(channel.clone()),
                        "wechat" => wechat_listener_binding = Some(channel.clone()),
                        _ => {}
                    }
                }
                Err(error) => {
                    log_info!(
                        "[mobile-control][start-worker] probe failed: channel={}, error={}",
                        channel.channel,
                        error
                    );
                    channel.listening = false;
                    channel.status = "平台连接失败".into();
                    channel.last_error = error.clone();
                    channel.last_checked_at = chrono_now();
                    channel.updated_at = chrono_now();
                    last_error = error;
                }
            }
        }
    }
    if MOBILE_REMOTE_CANCEL_REQUESTED.load(Ordering::SeqCst) {
        log_info!("[mobile-control][start-worker] cancelled after channel probe");
        MOBILE_REMOTE_START_ACTIVE.store(false, Ordering::SeqCst);
        return;
    }
    if connected_count == 0 {
        log_info!(
            "[mobile-control][start-worker] no channels connected: last_error={}",
            last_error
        );
        toolbox_state.mobile_remote.enabled = false;
        toolbox_state.mobile_remote.last_error = if last_error.is_empty() {
            "没有可连接的手机通道，请先绑定对话并填写平台凭据".into()
        } else {
            last_error
        };
    }
    let _ = write_toolbox_state(&app, &toolbox_state);
    log_info!(
        "[mobile-control][start-worker] state saved: enabled={}, connected_count={}",
        toolbox_state.mobile_remote.enabled,
        connected_count
    );
    if toolbox_state.mobile_remote.enabled {
        if let Some(binding) = lark_bridge_binding {
            log_info!("[mobile-control][start-worker] starting lark bridge");
            if let Err(error) = start_lark_bridge(app.clone(), binding) {
                log_info!(
                    "[mobile-control][start-worker] lark bridge failed: {}",
                    error
                );
                update_channel_status(&app, "lark", "飞书消息桥启动失败", &error);
            }
        }
        if let Some(binding) = qq_gateway_binding {
            log_info!("[mobile-control][start-worker] starting qq gateway");
            if let Err(error) = start_qq_gateway(app.clone(), binding) {
                log_info!(
                    "[mobile-control][start-worker] qq gateway failed: {}",
                    error
                );
                update_channel_status(&app, "qq", "QQ 网关启动失败", &error);
            }
        }
        if let Some(binding) = wechat_listener_binding {
            log_info!("[mobile-control][start-worker] starting wechat listener");
            if let Err(error) = start_wechat_listener(app.clone(), binding) {
                log_info!(
                    "[mobile-control][start-worker] wechat listener failed: {}",
                    error
                );
                update_channel_status(&app, "wechat", "微信监听启动失败", &error);
            }
        }
    }
    MOBILE_REMOTE_START_ACTIVE.store(false, Ordering::SeqCst);
    log_info!("[mobile-control][start-worker] finished");

    // 看门狗：后台持续监控各平台连接，断线后自动重启（最多等 5 分钟才重试）
    if toolbox_state.mobile_remote.enabled {
        // B3: 保证看门狗单例——start→stop→start 快速切换时旧看门狗已退出才能起新的
        if WATCHDOG_ACTIVE.swap(true, Ordering::SeqCst) {
            log_info!("[mobile-control][watchdog] 已有看门狗在运行，跳过重复启动");
            return;
        }
        let watchdog_app = app.clone();
        std::thread::spawn(move || {
            // B3: 辅助函数：分片睡眠，每 1 秒检查一次取消标志，避免停止后长时间不响应
            fn interruptible_sleep(secs: u64) {
                for _ in 0..secs {
                    if MOBILE_REMOTE_CANCEL_REQUESTED.load(Ordering::SeqCst) {
                        return;
                    }
                    std::thread::sleep(Duration::from_secs(1));
                }
            }

            let mut fail_counts: std::collections::HashMap<&str, u32> =
                std::collections::HashMap::new();
            loop {
                // 每 30 秒轮询一次（分片睡眠，可随时响应取消）
                interruptible_sleep(30);
                if MOBILE_REMOTE_CANCEL_REQUESTED.load(Ordering::SeqCst) {
                    log_info!("[mobile-control][watchdog] 已停止（用户取消）");
                    break;
                }
                let state = read_toolbox_state(&watchdog_app);
                // B3: 重读 state 后再次复查 enabled + cancel 标志
                if !state.mobile_remote.enabled
                    || MOBILE_REMOTE_CANCEL_REQUESTED.load(Ordering::SeqCst)
                {
                    log_info!("[mobile-control][watchdog] remote 已禁用或已取消，看门狗退出");
                    break;
                }
                // 检查飞书
                if !LARK_BRIDGE_ACTIVE.load(Ordering::SeqCst) {
                    if let Some(binding) = state
                        .mobile_channels
                        .iter()
                        .find(|b| b.channel == "lark" && mobile_channel_has_credentials(b))
                        .cloned()
                    {
                        let cnt = fail_counts.entry("lark").or_insert(0);
                        let delay = (30u64 * (1 << (*cnt).min(4))).min(300);
                        log_info!(
                            "[mobile-control][watchdog] 飞书断线，{}s 后重启（第{}次）",
                            delay,
                            *cnt + 1
                        );
                        interruptible_sleep(delay);
                        // B3: 退避后重读 state + cancel 标志，避免停止后继续拉起桥接
                        if MOBILE_REMOTE_CANCEL_REQUESTED.load(Ordering::SeqCst) {
                            break;
                        }
                        let fresh = read_toolbox_state(&watchdog_app);
                        if !fresh.mobile_remote.enabled {
                            break;
                        }
                        match start_lark_bridge(watchdog_app.clone(), binding) {
                            Ok(_) => {
                                *cnt = 0;
                                update_channel_status(&watchdog_app, "lark", "飞书已自动重连", "");
                            }
                            Err(e) => {
                                *cnt += 1;
                                update_channel_status(
                                    &watchdog_app,
                                    "lark",
                                    "飞书自动重连失败",
                                    &e,
                                );
                            }
                        }
                    }
                } else {
                    fail_counts.remove("lark");
                }
                // 检查 QQ
                if !QQ_GATEWAY_ACTIVE.load(Ordering::SeqCst) {
                    if let Some(binding) = state
                        .mobile_channels
                        .iter()
                        .find(|b| b.channel == "qq" && mobile_channel_has_credentials(b))
                        .cloned()
                    {
                        let cnt = fail_counts.entry("qq").or_insert(0);
                        let delay = (30u64 * (1 << (*cnt).min(4))).min(300);
                        log_info!(
                            "[mobile-control][watchdog] QQ 断线，{}s 后重启（第{}次）",
                            delay,
                            *cnt + 1
                        );
                        interruptible_sleep(delay);
                        if MOBILE_REMOTE_CANCEL_REQUESTED.load(Ordering::SeqCst) {
                            break;
                        }
                        let fresh = read_toolbox_state(&watchdog_app);
                        if !fresh.mobile_remote.enabled {
                            break;
                        }
                        match start_qq_gateway(watchdog_app.clone(), binding) {
                            Ok(_) => {
                                *cnt = 0;
                                update_channel_status(&watchdog_app, "qq", "QQ 已自动重连", "");
                            }
                            Err(e) => {
                                *cnt += 1;
                                update_channel_status(&watchdog_app, "qq", "QQ 自动重连失败", &e);
                            }
                        }
                    }
                } else {
                    fail_counts.remove("qq");
                }
                // 检查微信
                if !WECHAT_LISTENER_ACTIVE.load(Ordering::SeqCst) {
                    if let Some(binding) = state
                        .mobile_channels
                        .iter()
                        .find(|b| b.channel == "wechat" && mobile_channel_has_credentials(b))
                        .cloned()
                    {
                        let cnt = fail_counts.entry("wechat").or_insert(0);
                        let delay = (30u64 * (1 << (*cnt).min(4))).min(300);
                        log_info!(
                            "[mobile-control][watchdog] 微信断线，{}s 后重启（第{}次）",
                            delay,
                            *cnt + 1
                        );
                        interruptible_sleep(delay);
                        if MOBILE_REMOTE_CANCEL_REQUESTED.load(Ordering::SeqCst) {
                            break;
                        }
                        let fresh = read_toolbox_state(&watchdog_app);
                        if !fresh.mobile_remote.enabled {
                            break;
                        }
                        match start_wechat_listener(watchdog_app.clone(), binding) {
                            Ok(_) => {
                                *cnt = 0;
                                update_channel_status(
                                    &watchdog_app,
                                    "wechat",
                                    "微信已自动重连",
                                    "",
                                );
                            }
                            Err(e) => {
                                *cnt += 1;
                                update_channel_status(
                                    &watchdog_app,
                                    "wechat",
                                    "微信自动重连失败",
                                    &e,
                                );
                            }
                        }
                    }
                } else {
                    fail_counts.remove("wechat");
                }
            }
            WATCHDOG_ACTIVE.store(false, Ordering::SeqCst);
            log_info!("[mobile-control][watchdog] 看门狗线程退出");
        });
    }
}

#[tauri::command]
pub(crate) async fn stop_mobile_remote(
    app: tauri::AppHandle,
    _state: State<'_, AppState>,
) -> Result<ToolboxSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || stop_mobile_remote_blocking(app))
        .await
        .map_err(|e| format!("停止手机连接后台任务失败: {e}"))?
}

pub(crate) fn stop_mobile_remote_blocking(app: tauri::AppHandle) -> Result<ToolboxSnapshot, String> {
    log_info!("[mobile-control][stop] stop_mobile_remote requested");
    MOBILE_REMOTE_CANCEL_REQUESTED.store(true, Ordering::SeqCst);
    MOBILE_REMOTE_START_ACTIVE.store(false, Ordering::SeqCst);
    stop_smart_control_server();
    stop_lark_bridge();
    stop_qq_gateway();
    stop_wechat_listener(&app);
    let mut toolbox_state = read_toolbox_state(&app);
    toolbox_state.mobile_remote.enabled = false;
    toolbox_state.mobile_remote.last_error.clear();
    toolbox_state.mobile_remote.local_ip = detect_local_ip();
    for channel in toolbox_state.mobile_channels.iter_mut() {
        channel.listening = false;
        if channel.enabled {
            channel.status = "已绑定，平台连接已停止".into();
            channel.updated_at = chrono_now();
        }
    }
    write_toolbox_state(&app, &toolbox_state)?;
    log_info!("[mobile-control][stop] stopped and state saved");
    Ok(build_toolbox_snapshot(&app))
}
