use crate::{
    compatibility_check::Incompatibility,
    conductor::{RuntimeProcess, State},
    messages::{LockoutMessage, Message, OwnerMessage, RuntimeFault},
    runtime_builds::{BuildAcceptance, LaunchFailure},
};
use axum::http::{HeaderMap, header};
use serde::Deserialize;

const COOKIE: &str = "acp2api_language";
const COOKIE_MAX_AGE_SECONDS: u32 = 365 * 24 * 60 * 60;

#[derive(Clone, Copy, Deserialize, PartialEq)]
pub(super) enum Language {
    #[serde(rename = "en")]
    English,
    #[serde(rename = "zh")]
    Chinese,
}

pub(super) struct Labels {
    pub(super) official_runtime: &'static str,
    pub(super) quota: &'static str,
    pub(super) places_and_queue: &'static str,
    pub(super) setup: &'static str,
    pub(super) logs: &'static str,
    pub(super) settings: &'static str,
    pub(super) notices: &'static str,
    pub(super) places_in_use: &'static str,
    pub(super) requests_in_the_queue: &'static str,
    pub(super) build_in_use: &'static str,
    pub(super) concurrency_limit: &'static str,
    pub(super) management_password: &'static str,
}
const ENGLISH_LABELS: Labels = Labels {
    official_runtime: "Official runtime",
    quota: "Quota",
    places_and_queue: "Places and queue",
    setup: "Setup",
    logs: "Logs",
    settings: "Settings",
    notices: "Notices",
    places_in_use: "Places in use",
    requests_in_the_queue: "Requests in the queue",
    build_in_use: "Build in use",
    concurrency_limit: "Concurrency limit",
    management_password: "Management password",
};
const CHINESE_LABELS: Labels = Labels {
    official_runtime: "官方运行时",
    quota: "配额",
    places_and_queue: "名额与队列",
    setup: "接入",
    logs: "日志",
    settings: "设置",
    notices: "通知",
    places_in_use: "在用名额",
    requests_in_the_queue: "队列中的请求",
    build_in_use: "在用的构建",
    concurrency_limit: "并发上限",
    management_password: "管理密码",
};

impl Language {
    pub(super) fn of(headers: &HeaderMap) -> Self {
        super::cookie(headers, COOKIE)
            .and_then(Self::from_code)
            .unwrap_or_else(|| Self::of_browser(headers))
    }
    fn from_code(code: &str) -> Option<Self> {
        match code {
            "en" => Some(Self::English),
            "zh" => Some(Self::Chinese),
            _ => None,
        }
    }
    fn of_browser(headers: &HeaderMap) -> Self {
        let chinese = headers
            .get(header::ACCEPT_LANGUAGE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.trim_start().to_ascii_lowercase().starts_with("zh"));
        if chinese {
            Self::Chinese
        } else {
            Self::English
        }
    }
    pub(super) fn cookie(self) -> String {
        let code = match self {
            Self::English => "en",
            Self::Chinese => "zh",
        };
        format!(
            "{COOKIE}={code}; Path=/; HttpOnly; SameSite=Strict; Max-Age={COOKIE_MAX_AGE_SECONDS}"
        )
    }
    pub(super) fn html_tag(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Chinese => "zh-CN",
        }
    }
    pub(super) fn pick<'a>(self, english: &'a str, chinese: &'a str) -> &'a str {
        match self {
            Self::English => english,
            Self::Chinese => chinese,
        }
    }
    pub(super) fn labels(self) -> &'static Labels {
        match self {
            Self::English => &ENGLISH_LABELS,
            Self::Chinese => &CHINESE_LABELS,
        }
    }
    pub(super) fn state(self, state: State) -> &'static str {
        match state {
            State::Running => self.pick(state.name(), "运行中"),
            State::QuotaWait => self.pick(state.name(), "配额等待"),
            State::SignInRequired => self.pick(state.name(), "需要登录"),
            State::RuntimeUnavailable => self.pick(state.name(), "运行时不可用"),
            State::Lockout => self.pick(state.name(), "已锁死"),
        }
    }
    pub(super) fn runtime_process(self, runtime_process: RuntimeProcess) -> &'static str {
        match runtime_process {
            RuntimeProcess::Starting => self.pick("starting", "正在启动"),
            RuntimeProcess::Running => self.pick("running", "运行中"),
            RuntimeProcess::Stopping => self.pick("stopping", "正在停止"),
            RuntimeProcess::Stopped => self.pick("stopped", "已停止"),
        }
    }
    pub(super) fn build_acceptance(self, acceptance: BuildAcceptance) -> &'static str {
        match acceptance {
            BuildAcceptance::OnTheReleaseList => self.pick("On the release list", "发布列表"),
            BuildAcceptance::CheckedOnThisComputer => {
                self.pick("Checked on this computer", "本机已检查")
            }
        }
    }
    pub(super) fn built_in_tool_notice(self, name: &str) -> String {
        match self {
            Self::English => format!(
                "The built-in tool {name} ran with no permission request. acp2api cancelled the prompt turn and sent an error to the API client."
            ),
            Self::Chinese => format!(
                "内置工具 {name} 在没有权限请求的情况下运行。acp2api 取消了这个 prompt 轮次，并给 API 客户端返回了错误。"
            ),
        }
    }
    pub(super) fn api_message(self, message: &Message) -> String {
        match self {
            Self::English => message.to_string(),
            Self::Chinese => chinese_api_message(message),
        }
    }
    pub(super) fn owner_message(self, message: &OwnerMessage) -> String {
        match self {
            Self::English => message.to_string(),
            Self::Chinese => chinese_owner_message(message),
        }
    }
    pub(super) fn launch_failure(self, failure: &LaunchFailure) -> String {
        match self {
            Self::English => failure.to_string(),
            Self::Chinese => chinese_launch_failure(failure),
        }
    }
}

fn chinese_fault(fault: &RuntimeFault) -> String {
    match fault {
        RuntimeFault::NoExecutable => {
            "没有找到官方运行时，所以 acp2api 没有启动运行时进程。".into()
        }
        RuntimeFault::NotAccepted => {
            "找到的构建不是已接受版本，所以 acp2api 没有启动运行时进程。".into()
        }
        RuntimeFault::RepeatedFailures => {
            "运行时进程在 10 分钟内失败了 3 次，所以 acp2api 不再自动启动它。".into()
        }
        RuntimeFault::CopyFailed { version } => {
            format!("已接受构建 {version} 的复制失败了，所以 acp2api 没有启动运行时进程。")
        }
        RuntimeFault::StateFileNotWritable => "acp2api 无法写入状态文件 state.json".into(),
    }
}

fn chinese_lockout(message: &LockoutMessage) -> String {
    let LockoutMessage {
        since,
        signal,
        address,
        original,
    } = message;
    let original = original
        .as_ref()
        .map(|text| format!(" 原始错误：{text}"))
        .unwrap_or_default();
    format!(
        "{since}，官方运行时返回了一个可能危及 Google 账户的响应（{signal}）。从那时起，acp2api 不向官方运行时发送任何请求。所有者可以在 {address} 结束锁死。{original}"
    )
}

fn chinese_api_message(message: &Message) -> String {
    match message {
        Message::Lockout(lockout) => chinese_lockout(lockout),
        Message::RuntimeUnavailable(unavailable) => format!(
            "{} 已接受版本：{}。详见 http://127.0.0.1:{}/。",
            chinese_fault(&unavailable.fault),
            unavailable.versions,
            unavailable.port
        ),
        Message::SignInRequired(address) => {
            format!("官方运行时没有有效的登录。请在 {address} 登录。")
        }
        Message::QueueFull => format!(
            "队列已满（{} 个请求）。请稍后再发送请求。",
            crate::conductor::QUEUE_CAPACITY
        ),
        Message::QuotaWait { until, .. } => {
            format!("Google 配额已用完。{until} 之前，acp2api 不启动 prompt 轮次。")
        }
        Message::RuntimeFailed => "运行时进程失败了，所以 acp2api 结束了这个 prompt 轮次。".into(),
        Message::NoUpdate => format!(
            "官方运行时 {} 分钟没有更新，所以 acp2api 取消了这个 prompt 轮次。",
            crate::conductor::NO_UPDATE_LIMIT / 60_000
        ),
        Message::MaxTurnRequests => "这个 prompt 轮次达到了官方运行时的模型请求上限。".into(),
        Message::Stopped => "acp2api 在这个请求结束之前停止了。".into(),
        Message::BuiltInToolRan(name) => format!(
            "官方运行时的内置工具 {name} 在没有权限请求的情况下运行，所以 acp2api 取消了这个 prompt 轮次。"
        ),
        Message::RetryWait { seconds, error } => format!(
            "这个对话的一个请求在 {} 秒内失败过。请在 {seconds} 秒后再发送请求。错误：{}",
            crate::conductor::WAIT_SECONDS,
            chinese_api_message(error)
        ),
        Message::Upstream(text) => format!("上游错误：{text}"),
        Message::MissingSessionId => "官方运行时的回复里没有 sessionId。".into(),
        Message::RuntimeStdoutClosed => "运行时进程的标准输出关闭了。".into(),
        Message::RuntimeStdinWriteTimedOut => format!(
            "运行时进程的标准输入写入超过了 {} 秒。",
            crate::runtime_driver::WRITE_TIMEOUT / 1000
        ),
        Message::RuntimeStdinQueueFull => format!(
            "运行时进程的标准输入队列超过了 {} 字节。",
            crate::runtime_driver::MAX_QUEUED_BYTES
        ),
        Message::RuntimeStdinWriterStopped => "运行时进程的标准输入写入任务已停止。".into(),
        Message::RuntimeNotAvailable => "官方运行时不可用。".into(),
        Message::ConfigOptionNotApplied { option, value } => format!(
            "官方运行时没有应用 {option} 的值 {value}，所以 acp2api 结束了这个 prompt 轮次。"
        ),
        Message::UpstreamHttp(error) => {
            let code = error
                .code
                .map(|code| format!("{code} "))
                .unwrap_or_default();
            format!("上游错误 {code}{}：{}", error.status, error.message)
        }
        Message::UpstreamAcp(error) => {
            format!("上游 ACP 错误 {}：{}", error.code, error.message)
        }
        // These refuse the request of an API client, so no owner action gets them.
        Message::Field { .. }
        | Message::Model { .. }
        | Message::Content(_)
        | Message::Media(_)
        | Message::Url(_)
        | Message::Pdf(_)
        | Message::Prefill
        | Message::ToolSearch
        | Message::ToolName(_)
        | Message::AdvisorTool(_)
        | Message::ServerTool { .. }
        | Message::CodexWebSearch
        | Message::ToolType { .. }
        | Message::InvalidJson
        | Message::Invalid(_)
        | Message::Key
        | Message::TooLarge => message.to_string(),
    }
}

fn chinese_incompatibility(reason: &Incompatibility) -> String {
    match reason {
        Incompatibility::NoMcpOverHttp => "initialize 没有声明 HTTP 上的 MCP".into(),
        Incompatibility::NoSystemPromptAddition => "initialize 没有声明系统提示词追加".into(),
        Incompatibility::MissingProtocolSupport => {
            "运行时必须支持 ACP 1、session/load 和 oauth-personal".into()
        }
        Incompatibility::NoVersion => "initialize 没有版本".into(),
        Incompatibility::NoSystemCodeword => "回复里没有系统提示词中的暗号".into(),
        Incompatibility::NoPermissionRequest => "没有客户端工具的权限请求".into(),
        Incompatibility::ToolArgumentsDiffer => "客户端工具的参数和提示不一致".into(),
        Incompatibility::SessionLoadFailed => "兼容性检查期间 session/load 失败了".into(),
        Incompatibility::NoToolResultText => "回复里没有以文本发送的工具结果".into(),
        Incompatibility::NoSessionFile => "检查会话在运行时主目录里没有文件".into(),
        Incompatibility::SessionFilesInGeminiFolder => {
            "检查会话在所有者的 Gemini 文件夹里有文件".into()
        }
        Incompatibility::UnexpectedStopReason => "兼容性检查的 prompt 轮次以意外的原因结束".into(),
        Incompatibility::BuiltInToolRan(name) => {
            format!("兼容性检查期间内置工具 {name} 运行了")
        }
        Incompatibility::PermissionForAnotherTool => "一个权限请求没有指向提供的客户端工具".into(),
        Incompatibility::RuntimeFailed => "兼容性检查期间运行时进程失败了".into(),
        Incompatibility::RuntimeExitedDuringLoad => {
            "检查用的运行时进程在 session/load 期间退出了".into()
        }
        Incompatibility::RuntimeExited => "检查用的运行时进程意外退出了".into(),
        Incompatibility::Refused(message) => chinese_api_message(message),
    }
}

fn chinese_owner_message(message: &OwnerMessage) -> String {
    match message {
        OwnerMessage::Api(message) => chinese_api_message(message),
        OwnerMessage::SignInCannotStart => {
            "登录或运行时进程替换正在进行，现在不能开始登录。".into()
        }
        OwnerMessage::QuotaRefreshTooSoon => "两次刷新配额至少间隔 60 秒。".into(),
        OwnerMessage::PathNotAbsolute => "填写 agy_acp_server 的绝对路径。".into(),
        OwnerMessage::PathWithoutServer => "这个路径上没有 agy_acp_server。".into(),
        OwnerMessage::PathWithoutHarness => {
            "这个文件夹里没有 localharness_external，acp2api 没有使用这个路径。".into()
        }
        OwnerMessage::ChooseUnacceptedBuild => "从搜索结果里选择一个不是已接受版本的构建。".into(),
        OwnerMessage::ChooseAcceptedBuild => "从搜索结果里选择一个已接受版本的构建。".into(),
        OwnerMessage::BuildChangedBeforeCheck => "选中的构建在兼容性检查之前变了。".into(),
        OwnerMessage::CheckCopyChanged => "检查副本在兼容性检查期间变了。".into(),
        OwnerMessage::CheckPassed { version } => format!("构建 {version} 通过了兼容性检查。"),
        OwnerMessage::CheckFailed(reason) => {
            format!("兼容性检查没有通过：{}。", chinese_incompatibility(reason))
        }
        OwnerMessage::CheckRunFailed(failure) => {
            format!("兼容性检查没有通过：{}", chinese_launch_failure(failure))
        }
        OwnerMessage::NoAnswerBeforeDeadline => "官方运行时在截止时间之前没有回答。".into(),
        OwnerMessage::VersionDiffers => "initialize 的版本和已接受的构建不一致。".into(),
        OwnerMessage::GeminiFolderNotFound => "找不到所有者的 Gemini 文件夹。".into(),
        OwnerMessage::LockoutEndNotSaved => "acp2api 无法写入状态文件，所以锁死没有结束。".into(),
        OwnerMessage::Diagnostic(text) => text.clone(),
    }
}

fn chinese_launch_failure(failure: &LaunchFailure) -> String {
    match failure {
        LaunchFailure::NoExecutable => chinese_fault(&RuntimeFault::NoExecutable),
        LaunchFailure::NotAccepted => chinese_fault(&RuntimeFault::NotAccepted),
        LaunchFailure::Io(error) => format!("操作系统错误：{error}"),
        LaunchFailure::Runtime(text) => format!("运行时进程出错：{text}"),
        LaunchFailure::Lockout { error, .. } => chinese_api_message(error),
        LaunchFailure::Stopped => "acp2api 正在退出。".into(),
        LaunchFailure::Owner(message) => chinese_owner_message(message),
    }
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod tests {
    use super::*;

    fn language(cookie: Option<&str>, browser: Option<&str>) -> &'static str {
        let mut headers = HeaderMap::new();
        if let Some(cookie) = cookie {
            headers.insert(header::COOKIE, cookie.parse().unwrap());
        }
        if let Some(browser) = browser {
            headers.insert(header::ACCEPT_LANGUAGE, browser.parse().unwrap());
        }
        Language::of(&headers).html_tag()
    }

    #[test]
    fn the_choice_of_the_owner_wins_over_the_browser_language() {
        assert_eq!(
            [
                language(Some("acp2api_language=en"), Some("zh-CN")),
                language(Some("other=1; acp2api_language=zh"), Some("en-US")),
                language(Some("acp2api_language=fr"), Some("zh-TW,en;q=0.5")),
            ],
            ["en", "zh-CN", "zh-CN"]
        );
    }

    #[test]
    fn only_a_browser_language_that_starts_with_zh_selects_chinese() {
        assert_eq!(
            [
                language(None, None),
                language(None, Some("ZH")),
                language(None, Some("fr-FR,zh;q=0.5")),
            ],
            ["en", "zh-CN", "en"]
        );
    }

    #[tokio::test]
    async fn a_check_with_no_answer_before_the_deadline_fails_in_chinese() {
        let folder = std::env::temp_dir().join(format!(
            "acp2api-check-deadline-{}",
            crate::random_hex::<12>().unwrap()
        ));
        std::fs::create_dir_all(&folder).unwrap();
        let clock: std::sync::Arc<dyn crate::Clock> =
            std::sync::Arc::new(crate::os::SystemClock::default());
        let builds = crate::runtime_builds::Builds::fixture(&folder, clock);
        let failure = builds
            .start
            .before_deadline(0, std::future::pending::<Result<(), LaunchFailure>>())
            .await
            .unwrap_err();
        let identity = crate::runtime_builds::BuildIdentity::fixture();
        let notice = builds
            .finish_check(
                &identity,
                Err(crate::compatibility_check::Failure::Runtime(failure)),
            )
            .unwrap();
        std::fs::remove_dir_all(&folder).unwrap();
        assert_eq!(
            Language::Chinese.owner_message(&notice),
            "兼容性检查没有通过：官方运行时在截止时间之前没有回答。"
        );
    }

    #[test]
    fn a_failed_check_preserves_the_external_diagnostics_final_full_stops() {
        let notice = OwnerMessage::CheckRunFailed(std::sync::Arc::new(LaunchFailure::Runtime(
            "external diagnostic。。".into(),
        )));
        assert_eq!(
            Language::Chinese.owner_message(&notice),
            "兼容性检查没有通过：运行时进程出错：external diagnostic。。"
        );
    }

    #[tokio::test]
    async fn a_check_with_a_stalled_stdin_fails_in_chinese() {
        // One frame, five real seconds, at most 16 KiB of recorded bytes, no real process.
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let mut fixture = crate::runtime_driver::write_tests::fixture().await;
            let checking = fixture.runtime.initialize(&fixture.start);
            let advance = async {
                assert_eq!(fixture.waits.recv().await.unwrap(), 30_000);
                fixture.now.send_replace(30_000);
            };
            let (result, ()) = tokio::join!(checking, advance);
            fixture.runtime.stop().await;
            let notice = OwnerMessage::CheckRunFailed(std::sync::Arc::new(result.unwrap_err()));
            assert_eq!(
                Language::English.owner_message(&notice),
                "The compatibility check failed: runtime stdin write exceeded 30 s."
            );
            assert_eq!(
                Language::Chinese.owner_message(&notice),
                "兼容性检查没有通过：运行时进程的标准输入写入超过了 30 秒。"
            );
        })
        .await
        .expect("panel stdin timeout test cap: 5 s");
    }

    #[test]
    fn an_external_diagnostic_keeps_its_text_inside_a_translated_frame() {
        let diagnostic = "permission denied (os error 13)";
        assert_eq!(
            [
                Language::Chinese.owner_message(&OwnerMessage::CheckFailed(
                    Incompatibility::Refused(Message::Upstream(diagnostic.into()))
                )),
                Language::Chinese.launch_failure(&LaunchFailure::Runtime(diagnostic.into())),
                Language::English.owner_message(&OwnerMessage::Diagnostic(diagnostic.into())),
            ],
            [
                format!("兼容性检查没有通过：上游错误：{diagnostic}。"),
                format!("运行时进程出错：{diagnostic}"),
                diagnostic.to_owned(),
            ]
        );
    }
}
