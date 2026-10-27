//! User-configured API profiles; secrets are excluded from JSON and document data.
use serde::{Deserialize, Serialize};
use std::sync::{Arc, atomic::AtomicBool};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Protocol {
    #[default]
    ChatCompletions,
    Responses,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Authentication {
    #[default]
    Bearer,
    ApiKey,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub model: String,
    pub protocol: Protocol,
    pub authentication: Authentication,
    pub extra_body: String,
    pub timeout_seconds: u32,
    pub allow_http: bool,
    pub remember_key: bool,
    /// Never part of JSON preferences, workbook data, logs or control responses.
    #[serde(skip)]
    pub api_key: String,
}
impl Default for Profile {
    fn default() -> Self {
        Self {
            id: "custom".into(),
            name: "自定义兼容 API".into(),
            base_url: String::new(),
            model: String::new(),
            protocol: Protocol::ChatCompletions,
            authentication: Authentication::Bearer,
            extra_body: "{}".into(),
            timeout_seconds: 180,
            allow_http: false,
            remember_key: false,
            api_key: String::new(),
        }
    }
}
impl Profile {
    pub fn presets() -> Vec<Self> {
        [
            ("openai", "GPT / OpenAI", "https://api.openai.com/v1", "gpt-4.1-mini"),
            ("minimax", "MiniMax", "https://api.minimax.io/v1", "MiniMax-M2.7"),
            ("deepseek", "DeepSeek", "https://api.deepseek.com/v1", "deepseek-chat"),
            ("kimi", "Kimi / 月之暗面", "https://api.moonshot.cn/v1", "kimi-k2.5"),
            ("xiaomi", "小米 MiMo", "https://api.xiaomimimo.com/v1", "mimo-v2.6-flash"),
            ("custom", "自定义兼容 API", "", ""),
        ]
        .into_iter()
        .map(|(id, name, base_url, model)| Self {
            id: id.into(),
            name: name.into(),
            base_url: base_url.into(),
            model: model.into(),
            protocol: if id == "openai" { Protocol::Responses } else { Protocol::ChatCompletions },
            authentication: if id == "xiaomi" { Authentication::ApiKey } else { Authentication::Bearer },
            extra_body: if id == "minimax" { "{\"reasoning_split\":true}".into() } else { "{}".into() },
            ..Self::default()
        })
        .collect()
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub active: usize,
    pub profiles: Vec<Profile>,
}
impl Default for Settings {
    fn default() -> Self {
        Self { active: 0, profiles: Profile::presets() }
    }
}
impl Settings {
    pub fn selected(&self) -> Result<&Profile, String> {
        self.profiles.get(self.active).ok_or_else(|| "请选择 API 服务商.".into())
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}
pub type Completion = Arc<dyn Fn(Profile, Vec<Message>, &AtomicBool) -> Result<String, String> + Send + Sync>;
pub type SaveSettings = Box<dyn Fn(&Settings) -> Result<(), String>>;

#[derive(Default)]
pub struct State {
    pub settings: Settings,
    pub open: bool,
    pub feedback: String,
    job: Option<TestJob>,
}
struct TestJob {
    cancel: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<Result<String, String>>>,
}
impl Drop for TestJob {
    fn drop(&mut self) {
        self.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}
impl crate::WordApp {
    pub(crate) fn api_command(&mut self, id: &str) -> Option<Result<serde_json::Value, String>> {
        match id {
            "ui.api.settings" => {
                self.api.open = true;
                Some(Ok(serde_json::json!({"open":true})))
            }
            "ui.api.status" => Some(serde_json::to_value(&self.api.settings).map_err(|_| "无法读取 API 设置.".into())),
            _ => None,
        }
    }
    pub fn api_window(&mut self, ctx: &egui::Context) {
        if self.api.job.as_ref().is_some_and(|j| j.worker.as_ref().is_some_and(|w| w.is_finished()))
            && let Some(job) = self.api.job.take()
        {
            self.api.feedback =
                if job.cancel.load(std::sync::atomic::Ordering::Relaxed) { "连接测试已取消.".into() } else { finish_test(job) };
        }
        if self.api.job.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if !self.api.open {
            return;
        }
        let mut open = true;
        egui::Window::new("第三方 API 设置").open(&mut open).default_width(560.0).resizable(true).show(ctx, |ui| {
            let settings = &mut self.api.settings;
            egui::ComboBox::from_label("服务商").selected_text(settings.selected().map(|p| p.name.as_str()).unwrap_or("选择服务商")).show_ui(
                ui,
                |ui| {
                    for (i, p) in settings.profiles.iter().enumerate() {
                        ui.selectable_value(&mut settings.active, i, &p.name);
                    }
                },
            );
            let Some(p) = settings.profiles.get_mut(settings.active) else { return };
            ui.label("API 地址（Base URL 或完整端点）");
            ui.text_edit_singleline(&mut p.base_url);
            ui.label("模型 ID（以服务商控制台为准,可更换）");
            ui.text_edit_singleline(&mut p.model);
            ui.label("API Key");
            ui.add(egui::TextEdit::singleline(&mut p.api_key).password(true).hint_text("仅发送到上面的 API 地址"));
            ui.checkbox(&mut p.remember_key, "使用系统钥匙串 / Windows 凭据管理器记住密钥");
            ui.small("未勾选时密钥仅用于本次运行,配置文件不保存明文密钥.");
            egui::ComboBox::from_label("接口协议")
                .selected_text(match p.protocol {
                    Protocol::ChatCompletions => "Chat Completions",
                    Protocol::Responses => "OpenAI Responses",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut p.protocol, Protocol::ChatCompletions, "Chat Completions");
                    ui.selectable_value(&mut p.protocol, Protocol::Responses, "OpenAI Responses");
                });
            ui.collapsing("高级设置", |ui| {
                egui::ComboBox::from_label("密钥发送方式")
                    .selected_text(match p.authentication {
                        Authentication::Bearer => "Authorization: Bearer",
                        Authentication::ApiKey => "api-key",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut p.authentication, Authentication::Bearer, "Authorization: Bearer");
                        ui.selectable_value(&mut p.authentication, Authentication::ApiKey, "api-key");
                    });
                ui.horizontal(|ui| {
                    ui.label("超时（秒）");
                    ui.add(egui::DragValue::new(&mut p.timeout_seconds).range(10..=600));
                });
                ui.checkbox(&mut p.allow_http, "允许此接口使用未加密的 HTTP");
                ui.label("附加请求参数（JSON 对象）");
                ui.add(egui::TextEdit::multiline(&mut p.extra_body).desired_rows(3).desired_width(f32::INFINITY));
            });
            ui.small("翻译使用当前服务商.MiniMax 国内账号可改为 https://api.minimaxi.com/v1.预设模型不保证账户权限.");
            ui.small("仅点击翻译才发送候选文字;保留原文,生成独立译文副本.连接测试可能产生少量 API 费用,不发送文档.");
            ui.horizontal(|ui| {
                if ui.button("保存设置").clicked() {
                    self.api.feedback = match &self.services.api_save_settings {
                        Some(save) => save(&self.api.settings).map(|()| "设置已保存.".into()).unwrap_or_else(|e| e),
                        None => "本环境仅用于本次运行.".into(),
                    };
                }
                if ui.add_enabled(self.api.job.is_none(), egui::Button::new("测试连接")).clicked()
                    && let (Some(service), Ok(profile)) = (self.services.api_complete.clone(), self.api.settings.selected().cloned())
                {
                    let cancel = Arc::new(AtomicBool::new(false));
                    let flag = cancel.clone();
                    match std::thread::Builder::new()
                        .name("wordcraft-api-test".into())
                        .spawn(move || service(profile, vec![Message { role: "user".into(), content: "请只回复:连接成功".into() }], &flag))
                    {
                        Ok(worker) => {
                            self.api.job = Some(TestJob { cancel, worker: Some(worker) });
                            self.api.feedback = "正在测试连接（未发送文档）…".into();
                        }
                        Err(_) => self.api.feedback = "无法启动连接测试.".into(),
                    }
                }
                if let Some(job) = &self.api.job
                    && ui.button("取消测试").clicked()
                {
                    job.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            });
            ui.label(&self.api.feedback);
        });
        self.api.open = open;
    }
}
fn finish_test(mut job: TestJob) -> String {
    match job.worker.take().map(|w| w.join()) {
        Some(Ok(Ok(answer))) => format!("API 已响应:{}", answer.chars().take(240).collect::<String>()),
        Some(Ok(Err(error))) => error,
        _ => "连接测试线程异常.".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keys_are_never_serialized_and_settings_open_does_not_send() {
        let mut app = crate::WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::new()), crate::Services::default());
        app.api.settings.profiles[0].api_key = "never-in-json-test-secret".into();
        let status = app.execute("ui.api.status", serde_json::json!({})).unwrap().to_string();
        assert!(!status.contains("never-in-json-test-secret"));
        assert!(!status.contains("apiKey"));
        assert!(app.execute("ui.api.settings", serde_json::json!({})).is_ok());
        assert!(app.api.open);
        assert!(app.api.job.is_none());
    }
    #[test]
    fn missing_configuration_opens_settings_without_sending() {
        let called = Arc::new(AtomicBool::new(false));
        let check = called.clone();
        let mut app = crate::WordApp::new(
            wordcraft_engine::Session::new(wordcraft_doc::Document::new()),
            crate::Services {
                translate_document: Some(Arc::new(move |_, _, _, _| {
                    check.store(true, std::sync::atomic::Ordering::Relaxed);
                    Ok(Default::default())
                })),
                publish_translation: Some(Box::new(|_, _| Ok("unused".into()))),
                ..Default::default()
            },
        );
        app.execute("text.insert", serde_json::json!({"text":"板材","raw":true})).unwrap();
        assert!(app.execute("ui.translateDocument", serde_json::json!({})).is_err());
        assert!(app.api.open);
        assert!(!called.load(std::sync::atomic::Ordering::Relaxed));
    }
}
