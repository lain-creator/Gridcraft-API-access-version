use super::*;
use std::io::Write;
type TestResult = Result<(), Box<dyn std::error::Error>>;
fn message() -> Vec<Message> {
    vec![Message { role: "user".into(), content: "synthetic test only".into() }]
}
fn local_profile(url: String) -> Profile {
    Profile { base_url: url, model: "test-model".into(), api_key: "not-a-real-key".into(), ..Default::default() }
}
fn mock(status: &str, response: &str) -> Result<(String, std::thread::JoinHandle<Result<String, String>>), Box<dyn std::error::Error>> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    listener.set_nonblocking(true)?;
    let status = status.to_string();
    let response = response.to_string();
    let worker = std::thread::spawn(move || -> Result<String, String> {
        #[allow(clippy::disallowed_methods)]
        let start = Instant::now();
        let mut stream = loop {
            match listener.accept() {
                Ok((s, _)) => break s,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if start.elapsed() > Duration::from_secs(8) {
                        return Err("test server timed out".into());
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(e) => return Err(e.to_string()),
            }
        };
        stream.set_read_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let n = stream.read(&mut chunk).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..n]);
            let text = String::from_utf8_lossy(&bytes);
            if let Some((head, body)) = text.split_once("\r\n\r\n") {
                let len = head
                    .lines()
                    .find_map(|l| {
                        l.split_once(':').filter(|(k, _)| k.eq_ignore_ascii_case("content-length")).and_then(|(_, v)| v.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                if body.len() >= len {
                    break;
                }
            }
            if bytes.len() > 1024 * 1024 {
                return Err("test request too big".into());
            }
        }
        write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nLocation: http://127.0.0.1:1/never-follow\r\nConnection: close\r\n\r\n{response}",
            response.len()
        )
        .map_err(|e| e.to_string())?;
        String::from_utf8(bytes).map_err(|e| e.to_string())
    });
    Ok((format!("http://{address}/v1"), worker))
}
#[test]
fn all_presets_have_editable_protocol_and_endpoint() -> TestResult {
    let profiles = Profile::presets();
    assert_eq!(profiles.len(), 6);
    for p in profiles.iter().filter(|p| p.id != "custom") {
        let url = endpoint(p)?;
        assert_eq!(url.scheme(), "https");
        assert!(url.path().ends_with(if p.id == "openai" { "responses" } else { "chat/completions" }));
    }
    let mut p = local_profile("https://example.invalid/v2/chat/completions/".into());
    assert_eq!(endpoint(&p)?.path(), "/v2/chat/completions/");
    p.base_url = "http://example.invalid/v1".into();
    assert!(endpoint(&p).is_err());
    p.allow_http = true;
    assert!(endpoint(&p).is_ok());
    p.base_url = "https://user:password@example.invalid/v1".into();
    assert!(endpoint(&p).is_err());
    p.base_url = "https://example.invalid/v1?api_key=secret".into();
    assert!(endpoint(&p).is_err());
    Ok(())
}
#[test]
fn payload_uses_correct_protocol_and_blocks_overrides() -> TestResult {
    let mut p = local_profile("http://127.0.0.1".into());
    let chat = body(&p, &message())?;
    assert!(chat.get("messages").is_some() && chat.get("input").is_none());
    assert_eq!(chat["stream"], false);
    p.protocol = Protocol::Responses;
    let responses = body(&p, &message())?;
    assert!(responses.get("input").is_some());
    assert_eq!(responses["store"], false);
    p.extra_body = "{\"tools\":[{}]}".into();
    assert!(body(&p, &message()).is_err());
    p.extra_body = "[]".into();
    assert!(body(&p, &message()).is_err());
    p.extra_body = "{\"temperature\":0.2}".into();
    assert_eq!(body(&p, &message())?["temperature"], 0.2);
    Ok(())
}
#[test]
fn response_formats_do_not_confuse_thinking_with_answer() -> TestResult {
    let b =
        json!({"choices":[{"finish_reason":"stop","message":{"content":"<think>private reasoning</think>正文","reasoning_content":"not answer"}}]});
    assert_eq!(parse(&serde_json::to_vec(&b)?, Protocol::ChatCompletions)?, "正文");
    let r = json!({"status":"completed","output":[{"type":"reasoning","summary":[]},{"type":"message","content":[{"type":"output_text","text":"answer"}]}]});
    assert_eq!(parse(&serde_json::to_vec(&r)?, Protocol::Responses)?, "answer");
    let b = json!({"choices":[{"finish_reason":"length","message":{"content":"partial"}}]});
    assert!(parse(&serde_json::to_vec(&b)?, Protocol::ChatCompletions).is_err());
    assert!(parse(b"<html>login</html>", Protocol::ChatCompletions).is_err());
    Ok(())
}
#[test]
fn real_transport_sends_bearer_json_and_preserves_unicode() -> TestResult {
    let (url, worker) = mock("200 OK", r#"{"choices":[{"finish_reason":"stop","message":{"content":"回答成功"}}]}"#)?;
    assert_eq!(complete(local_profile(url), message(), &AtomicBool::new(false))?, "回答成功");
    let request = worker.join().map_err(|_| "server")??;
    assert!(request.starts_with("POST /v1/chat/completions HTTP/1.1"));
    assert!(request.contains("Authorization: Bearer not-a-real-key"));
    assert!(request.contains("application/json"));
    let payload: Json = serde_json::from_str(request.split_once("\r\n\r\n").ok_or("body")?.1)?;
    assert_eq!(payload["messages"][0]["content"], "synthetic test only");
    Ok(())
}
#[test]
fn xiaomi_header_and_responses_protocol_reach_server() -> TestResult {
    let (url, worker) = mock("200 OK", r#"{"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"ok"}]}]}"#)?;
    let mut p = local_profile(url);
    p.protocol = Protocol::Responses;
    p.authentication = Authentication::ApiKey;
    assert_eq!(complete(p, message(), &AtomicBool::new(false))?, "ok");
    let request = worker.join().map_err(|_| "server")??;
    assert!(request.starts_with("POST /v1/responses HTTP/1.1"));
    assert!(request.contains("api-key: not-a-real-key"));
    assert!(!request.contains("Authorization:"));
    Ok(())
}
#[test]
fn redirects_and_http_errors_are_not_retried_and_do_not_leak_secrets() -> TestResult {
    for code in ["302 Found", "401 Unauthorized", "429 Too Many Requests"] {
        let (url, worker) = mock(code, "not-a-real-key echoed by server")?;
        let error = complete(local_profile(url), message(), &AtomicBool::new(false)).err().ok_or("expected error")?;
        assert!(!error.contains("not-a-real-key"));
        assert!(error.contains(&code[..3]));
        worker.join().map_err(|_| "server")??;
    }
    Ok(())
}
#[test]
fn cancelled_and_invalid_credentials_never_connect() {
    let mut p = local_profile("http://127.0.0.1:1".into());
    assert!(complete(p.clone(), message(), &AtomicBool::new(true)).is_err());
    p.api_key = "bad\nInjected: header".into();
    assert!(validate(&p).is_err());
}
