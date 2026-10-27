//! Native HTTPS transport. Credentials never appear in command arguments or preference JSON.
use serde_json::{Value as Json, json};
use wordcraft_ui_egui::api::{Authentication, Message, Profile, Protocol, Settings};
#[cfg(test)]
#[path = "ai_api_tests.rs"]
mod tests;
use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

const RESPONSE_LIMIT: u64 = 4 * 1024 * 1024;

fn endpoint(profile: &Profile) -> Result<url::Url, String> {
    let mut url = url::Url::parse(profile.base_url.trim()).map_err(|_| "API 地址无效，请填写完整的 https:// 地址。")?;
    if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() || url.query().is_some() {
        return Err("API 地址不能含账号、密码、查询参数或片段；请在 API Key 栏填写密钥。".into());
    }
    let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]" | "::1"));
    if url.scheme() != "https" && !(url.scheme() == "http" && (local || profile.allow_http)) {
        return Err("请使用 HTTPS；本地接口或已在高级设置中明确允许的 HTTP 地址也可使用。".into());
    }
    let path = url.path().trim_end_matches('/');
    let suffix = match profile.protocol {
        Protocol::ChatCompletions => "chat/completions",
        Protocol::Responses => "responses",
    };
    if !path.ends_with(&format!("/{suffix}")) {
        let path = format!("{path}/{suffix}");
        url.set_path(&path);
    }
    Ok(url)
}
fn validate(profile: &Profile) -> Result<url::Url, String> {
    let endpoint = endpoint(profile)?;
    if profile.model.trim().is_empty() || profile.model.len() > 256 {
        return Err("请填写有效模型 ID。".into());
    }
    if profile.api_key.len() > 8192 || profile.api_key.chars().any(char::is_control) {
        return Err("API Key 不能包含换行或控制字符。".into());
    }
    let local = matches!(endpoint.host_str(), Some("localhost" | "127.0.0.1" | "[::1]" | "::1"));
    if profile.api_key.trim().is_empty() && !local {
        return Err("请在“API 设置”中填写该服务商的 API Key。".into());
    }
    if !(10..=600).contains(&profile.timeout_seconds) {
        return Err("API 超时时间须在 10–600 秒之间。".into());
    }
    Ok(endpoint)
}
fn body(profile: &Profile, messages: &[Message]) -> Result<Json, String> {
    if messages.is_empty() || messages.len() > 64 || messages.iter().any(|m| !matches!(m.role.as_str(), "system" | "user" | "assistant")) {
        return Err("请求的消息角色或数量无效。".into());
    }
    let mut body = match profile.protocol {
        Protocol::ChatCompletions => json!({"model":profile.model.trim(),"messages":messages,"stream":false}),
        Protocol::Responses => json!({"model":profile.model.trim(),"input":messages,"stream":false,"store":false}),
    };
    let extra: Json = serde_json::from_str(if profile.extra_body.trim().is_empty() { "{}" } else { &profile.extra_body })
        .map_err(|_| "附加请求参数必须是有效 JSON 对象。")?;
    let extra = extra.as_object().ok_or("附加请求参数必须是 JSON 对象。")?;
    let map = body.as_object_mut().ok_or("请求构造失败。")?;
    for (key, value) in extra {
        if matches!(
            key.as_str(),
            "model" | "messages" | "input" | "stream" | "store" | "tools" | "tool_choice" | "previous_response_id" | "conversation" | "background"
        ) {
            return Err("附加参数不能覆盖模型、消息、流式、存储或工具设置。".into());
        }
        map.insert(key.clone(), value.clone());
    }
    Ok(body)
}
fn content_text(content: &Json) -> String {
    if let Some(text) = content.as_str() {
        return text.to_string();
    }
    content
        .as_array()
        .map(|parts| {
            parts
                .iter()
                .filter(|p| matches!(p.get("type").and_then(Json::as_str), Some("text" | "output_text")))
                .filter_map(|p| p.get("text").and_then(Json::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}
fn parse(bytes: &[u8], protocol: Protocol) -> Result<String, String> {
    let value: Json = serde_json::from_slice(bytes).map_err(|_| "API 返回的内容不是有效 JSON，请核对接口协议和地址。")?;
    if value.get("error").is_some_and(|v| !v.is_null()) {
        return Err("服务商返回 API 错误，请检查密钥、模型权限及附加参数。".into());
    }
    let text = match protocol {
        Protocol::ChatCompletions => {
            let choice = value.get("choices").and_then(Json::as_array).and_then(|c| c.first()).ok_or("API 响应缺少 choices。")?;
            if choice.get("finish_reason").and_then(Json::as_str).is_some_and(|r| r != "stop") {
                return Err("回答未完整结束或被服务商拦截，未使用不完整内容；可调整输出长度后重试。".into());
            }
            let message = choice.get("message").ok_or("API 响应缺少 message。")?;
            if message.get("refusal").and_then(Json::as_str).is_some_and(|r| !r.is_empty()) {
                return Err("服务商拒绝了本次请求。".into());
            }
            content_text(&message["content"])
        }
        Protocol::Responses => {
            if value.get("status").and_then(Json::as_str).is_some_and(|s| s != "completed") {
                return Err("Responses 回答尚未完整完成，请检查模型输出限制后重试。".into());
            }
            let output = value.get("output").and_then(Json::as_array).ok_or("API 响应缺少 output。")?;
            output
                .iter()
                .filter(|o| o.get("type").and_then(Json::as_str) == Some("message"))
                .map(|o| content_text(&o["content"]))
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        }
    };
    // MiniMax's compatible endpoint can put a leading reasoning block in content.
    // Only remove complete leading blocks; never treat reasoning_content as the answer.
    let mut text = text.trim();
    while let Some(rest) = text.strip_prefix("<think>") {
        let (_, after) = rest.split_once("</think>").ok_or("API 只返回了未完成的思考内容。")?;
        text = after.trim();
    }
    if text.is_empty() {
        return Err("API 没有返回正文；请检查模型或增加输出长度。".into());
    }
    Ok(text.to_string())
}

fn curl_executable() -> Result<std::path::PathBuf, String> {
    #[cfg(target_os = "windows")]
    {
        let path = std::env::var_os("SystemRoot").map(std::path::PathBuf::from).ok_or("Windows 系统路径不可用。")?.join("System32/curl.exe");
        if !path.is_file() {
            return Err("未找到系统 curl.exe，请使用已更新的 Windows 10/11。".into());
        }
        Ok(path)
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(std::path::PathBuf::from("/usr/bin/curl"))
    }
}
fn private_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).and_then(|mut f| f.write_all(bytes)).map_err(|_| "无法创建私有临时请求文件。".into())
}

pub fn complete(profile: Profile, messages: Vec<Message>, cancel: &AtomicBool) -> Result<String, String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("请求已取消。".into());
    }
    let endpoint = validate(&profile)?;
    let bytes = serde_json::to_vec(&body(&profile, &messages)?).map_err(|_| "无法编码请求。")?;
    if bytes.len() > 1024 * 1024 {
        return Err("API 请求超过 1 MB，请缩小选区或新建对话。".into());
    }
    let dir = tempfile::Builder::new().prefix("wordcraft-api-").tempdir().map_err(|_| "无法创建临时请求目录。")?;
    private_write(&dir.path().join("request.json"), &bytes)?;
    let authorization = match profile.authentication {
        Authentication::Bearer => format!("Authorization: Bearer {}", profile.api_key.trim()),
        Authentication::ApiKey => format!("api-key: {}", profile.api_key.trim()),
    };
    let headers = if profile.api_key.trim().is_empty() {
        "Content-Type: application/json\n".to_string()
    } else {
        format!("Content-Type: application/json\n{authorization}\n")
    };
    private_write(&dir.path().join("headers.txt"), headers.as_bytes())?;
    let output = dir.path().join("response.json");
    let status = dir.path().join("http-code");
    let mut command = Command::new(curl_executable()?);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
        .current_dir(dir.path())
        .args([
            "--disable",
            "--silent",
            "--show-error",
            "--noproxy",
            "localhost,127.0.0.1,::1",
            "--proto",
            "=http,https",
            "--connect-timeout",
            "15",
            "--max-time",
        ])
        .arg(profile.timeout_seconds.to_string())
        .args([
            "--max-filesize",
            &RESPONSE_LIMIT.to_string(),
            "--header",
            "@headers.txt",
            "--data-binary",
            "@request.json",
            "--write-out",
            "%{http_code}",
            "--output",
        ])
        .arg(&output)
        .arg(endpoint.as_str())
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(&status).map_err(|_| "无法创建响应状态文件。")?)
        .stderr(Stdio::null());
    let mut child = command.spawn().map_err(|_| "无法启动系统网络客户端。")?;
    #[allow(clippy::disallowed_methods)]
    let start = Instant::now();
    loop {
        if cancel.load(Ordering::Relaxed) || start.elapsed() > Duration::from_secs(u64::from(profile.timeout_seconds) + 5) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(if cancel.load(Ordering::Relaxed) {
                "已停止本机等待；服务端可能已处理请求。"
            } else {
                "API 请求超时，未自动重试。"
            }
            .into());
        }
        match child.try_wait() {
            Ok(Some(s)) if s.success() => break,
            Ok(Some(_)) => return Err("API 连接失败、证书无效、响应超限或请求超时；未自动重试。".into()),
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("无法读取网络客户端状态。".into());
            }
        }
    }
    let status = std::fs::read_to_string(status).map_err(|_| "无法读取 HTTP 状态。")?;
    let code = status.trim().parse::<u16>().map_err(|_| "API 返回了无效 HTTP 状态。")?;
    if !(200..300).contains(&code) {
        let hint = match code {
            401 | 403 => "请检查 API Key 和模型权限",
            404 => "请检查 API 地址、协议和模型 ID",
            429 => "额度或速率受限，请稍后重试",
            300..=399 => "为保护密钥，不跟随重定向，请填写最终 API 地址",
            _ => "请检查服务商状态及请求参数",
        };
        return Err(format!("API 返回 HTTP {code}：{hint}；未自动重试。"));
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("请求已取消。".into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(output).and_then(|f| f.take(RESPONSE_LIMIT + 1).read_to_end(&mut bytes)).map_err(|_| "无法读取 API 响应。")?;
    if bytes.len() as u64 > RESPONSE_LIMIT {
        return Err("API 响应超过 4 MB。".into());
    }
    parse(&bytes, profile.protocol)
}

pub fn load_settings() -> Result<Settings, String> {
    if !super::prefs_enabled() {
        return Ok(Settings::default());
    }
    let Some(dir) = wordcraft_control_key::settings_dir() else {
        return Ok(Settings::default());
    };
    let path = dir.join("ai-api.json");
    if !path.exists() {
        return Ok(Settings::default());
    }
    let bytes = std::fs::read(path).map_err(|_| "无法读取 API 设置。")?;
    if bytes.len() > 128 * 1024 {
        return Err("API 设置文件过大。".into());
    }
    let mut settings: Settings = serde_json::from_slice(&bytes).map_err(|_| "API 设置无效，请重新配置。")?;
    if settings.profiles.is_empty() || settings.profiles.len() > 32 || settings.active >= settings.profiles.len() {
        return Err("API 服务商设置无效。".into());
    }
    for p in &mut settings.profiles {
        if p.remember_key {
            p.api_key = read_key(&p.id)?;
        }
    }
    Ok(settings)
}
pub fn save_settings(settings: &Settings) -> Result<(), String> {
    if !super::prefs_enabled() {
        return Err("本次测试运行不保存设置。".into());
    }
    let profile = settings.selected()?;
    // Validate address and JSON without requiring an entered key, allowing setup before login.
    endpoint(profile)?;
    body(profile, &[Message { role: "user".into(), content: "test".into() }])?;
    let dir = wordcraft_control_key::settings_dir().ok_or("无法确定设置目录。")?;
    std::fs::create_dir_all(&dir).map_err(|_| "无法创建设置目录。")?;
    for p in &settings.profiles {
        if p.remember_key && !p.api_key.is_empty() {
            write_key(&p.id, &p.api_key)?;
        } else if !p.remember_key {
            forget_key(&p.id)?;
        }
    }
    let bytes = serde_json::to_vec_pretty(settings).map_err(|_| "无法编码设置。")?;
    let mut file = tempfile::NamedTempFile::new_in(&dir).map_err(|_| "无法创建设置文件。")?;
    std::io::Write::write_all(&mut file, &bytes).map_err(|_| "无法写入设置。")?;
    file.persist(dir.join("ai-api.json")).map_err(|_| "无法替换设置文件。")?;
    Ok(())
}
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn entry(id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new("WordCraft Third Party API", id).map_err(|_| "无法访问系统密钥存储。".into())
}
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn read_key(id: &str) -> Result<String, String> {
    match entry(id)?.get_password() {
        Ok(v) => Ok(v),
        Err(keyring::Error::NoEntry) => Ok(String::new()),
        Err(_) => Err("无法读取已保存的 API Key，请在设置中重新填写。".into()),
    }
}
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn write_key(id: &str, key: &str) -> Result<(), String> {
    entry(id)?.set_password(key).map_err(|_| "系统未能保存 API Key；请取消记住密钥后重试。".into())
}
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn forget_key(id: &str) -> Result<(), String> {
    match entry(id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err("无法移除已保存的 API Key。".into()),
    }
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn read_key(_: &str) -> Result<String, String> {
    Ok(String::new())
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn write_key(_: &str, _: &str) -> Result<(), String> {
    Err("本平台暂不支持记住密钥，请仅在本次运行使用。".into())
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn forget_key(_: &str) -> Result<(), String> {
    Ok(())
}
