use super::texts::{Labels, Language};
use crate::conductor::{RuntimeProcess, State};

pub(super) const COUNTDOWN: &str = r#"const timer = document.getElementById('sign-in-time');
const seconds = Number(timer.dataset.seconds);
const started = performance.now();
function update() {
    const left = Math.max(0, seconds - Math.floor((performance.now() - started) / 1000));
    timer.textContent = `${Math.floor(left / 60)}:${String(left % 60).padStart(2, '0')}`;
    if (left > 0) setTimeout(update, 1000);
}
update();"#;
// The Content-Security-Policy holds the hash of this script, so its texts stay in the page.
pub(super) const COPY_KEY: &str = r#"const copyKey = document.getElementById('copy-key');
copyKey.addEventListener('click', async () => {
    const report = (tone, text) => {
        const dot = document.createElement('span');
        dot.className = `dot ${tone}`;
        document.getElementById('copy-key-status').replaceChildren(dot, text);
    };
    try {
        await navigator.clipboard.writeText(document.getElementById('local-api-key').value);
        report('good', copyKey.dataset.copied);
    } catch {
        report('critical', copyKey.dataset.failed);
    }
});"#;
const STYLE: &str = r#"
:root {
    --gutter:max(22px,calc((100% - 980px)/2));
    --hairline:#d2d2d7;
    --action:#0066cc;
    color-scheme:light;
    font:17px/1.47 system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI","PingFang SC","Microsoft YaHei",sans-serif;
    color:#1d1d1f;
    background:#fff;
}
* {
    box-sizing:border-box;
}
body {
    margin:0;
}
a {
    color:var(--action);
}
h1 {
    font-size:40px;
    font-weight:600;
    line-height:1.1;
    margin:0 0 16px;
}
h2 {
    font-size:40px;
    font-weight:600;
    line-height:1.1;
    margin:0 0 24px;
}
h3 {
    font-size:21px;
    font-weight:600;
    line-height:1.19;
    margin:32px 0 12px;
}
:lang(en) h1,:lang(en) h2 {
    letter-spacing:-.28px;
}
p {
    max-width:65ch;
}
.global-bar {
    display:flex;
    justify-content:space-between;
    align-items:center;
    gap:16px;
    min-height:44px;
    padding:4px var(--gutter);
    background:#000;
    color:#fff;
    font-size:14px;
}
.global-bar button {
    margin:0;
    padding:4px 14px;
    font-size:12px;
}
.section-bar {
    position:sticky;
    top:0;
    z-index:1;
    display:flex;
    flex-wrap:wrap;
    align-items:center;
    gap:4px 16px;
    padding:10px var(--gutter);
    background:#f5f5f7cc;
    -webkit-backdrop-filter:saturate(180%) blur(20px);
    backdrop-filter:saturate(180%) blur(20px);
    border-bottom:1px solid var(--hairline);
}
.section-bar p {
    margin:0;
    font-size:14px;
}
.section-bar .panel-title {
    font-size:21px;
    font-weight:600;
    line-height:1.19;
}
.section-bar nav {
    display:flex;
    gap:20px;
    margin-left:auto;
    max-width:100%;
    overflow-x:auto;
    font-size:14px;
    white-space:nowrap;
}
section {
    padding:80px var(--gutter);
    scroll-margin-top:96px;
}
main>section:nth-of-type(even) {
    background:#f5f5f7;
}
.dark {
    background:#272729;
    color:#fff;
}
.dark button:focus-visible,.dark input:focus-visible,.global-bar button:focus-visible {
    outline-color:#fff;
}
#sign-in {
    text-align:center;
}
#sign-in p {
    margin-left:auto;
    margin-right:auto;
}
#sign-in h1 {
    font-size:56px;
    line-height:1.07;
}
#sign-in h1 .dot {
    margin-right:14px;
}
#sign-in .figures {
    display:grid;
    grid-template-columns:repeat(3,1fr);
    gap:24px;
    margin-top:56px;
}
.figures p {
    display:flex;
    flex-direction:column-reverse;
    margin:0;
    font-size:14px;
}
.figures span {
    font-size:40px;
    font-weight:600;
    line-height:1.1;
    overflow-wrap:anywhere;
}
.surface {
    background:#fff;
    color:#1d1d1f;
    border:1px solid var(--hairline);
    border-radius:18px;
    padding:16px 20px;
    overflow-wrap:anywhere;
}
.two {
    display:grid;
    grid-template-columns:1fr 1fr;
    gap:48px;
}
.two>div {
    min-width:0;
}
#notices section {
    padding-top:32px;
    padding-bottom:32px;
    background:#f5f5f7;
    border-bottom:1px solid var(--hairline);
}
#notices h2 {
    font-size:21px;
    line-height:1.19;
    margin-bottom:8px;
}
.notice-row {
    display:flex;
    flex-wrap:wrap;
    align-items:center;
    justify-content:space-between;
    gap:0 24px;
}
.notice-row p {
    margin:12px 0;
    font-size:14px;
}
.notice-row button {
    margin:0;
}
.line {
    margin:0;
    max-width:none;
    padding:12px var(--gutter);
    background:#f5f5f7;
    border-bottom:1px solid var(--hairline);
    font-size:14px;
}
.login {
    max-width:540px;
    margin:80px auto;
    padding:0 22px;
}
.login section {
    padding:28px;
    border:1px solid var(--hairline);
    border-radius:18px;
}
.login .line {
    padding:12px 0 0;
    background:none;
    border:0;
}
.intro {
    color:#616166;
    margin-bottom:36px;
}
.fine {
    font-size:12px;
    color:#616166;
}
.fine code {
    font-size:12px;
}
.dot {
    display:inline-block;
    width:9px;
    height:9px;
    margin-right:8px;
    border-radius:50%;
    vertical-align:middle;
}
.good {
    background:#0ca30c;
}
.warning {
    background:#fab219;
}
.serious {
    background:#ec835a;
}
.critical {
    background:#d03b3b;
}
form+form {
    margin-top:24px;
}
.actions {
    display:flex;
    flex-wrap:wrap;
    gap:12px;
}
.actions form {
    margin:0;
}
label {
    display:block;
    margin-bottom:8px;
}
input,select,button,summary {
    font:inherit;
}
input,select {
    border:1px solid #86868b;
    border-radius:999px;
    padding:12px 18px;
    max-width:100%;
    width:100%;
    background:#fff;
    color:#1d1d1f;
}
input:focus-visible,select:focus-visible,button:focus-visible,summary:focus-visible,a:focus-visible {
    outline:3px solid var(--action);
    outline-offset:3px;
}
button {
    background:var(--action);
    color:#fff;
    border:0;
    border-radius:999px;
    padding:12px 22px;
    cursor:pointer;
    margin-top:18px;
}
summary {
    color:var(--action);
    cursor:pointer;
    padding:12px 0;
}
code {
    overflow-wrap:anywhere;
    font:14px ui-monospace,Menlo,Consolas,monospace;
}
pre {
    overflow-x:auto;
    margin:0;
    padding:18px 20px;
    background:#fff;
    border:1px solid var(--hairline);
    border-radius:18px;
    font:14px/1.6 ui-monospace,Menlo,Consolas,monospace;
}
.quota-result {
    white-space:pre-wrap;
    overflow-wrap:anywhere;
    font:inherit;
}
.log-table {
    overflow-x:auto;
    margin-top:24px;
}
table {
    border-collapse:collapse;
    width:100%;
    font-size:14px;
    text-align:left;
}
th,td {
    border-bottom:1px solid var(--hairline);
    padding:8px 12px;
    vertical-align:top;
}
thead th {
    font-size:12px;
    white-space:nowrap;
}
.result {
    white-space:nowrap;
}
caption {
    text-align:left;
    font-weight:600;
    margin-bottom:12px;
}
@media(max-width:600px) {
    .global-bar,.section-bar {
        padding-left:16px;
        padding-right:16px;
    }
    .section-bar nav {
        margin-left:0;
    }
    section {
        padding:48px 16px;
    }
    h1 {
        font-size:34px;
    }
    h2 {
        font-size:34px;
    }
    #sign-in h1 {
        font-size:40px;
    }
    #sign-in .figures {
        gap:12px;
        margin-top:40px;
    }
    .figures span {
        font-size:28px;
    }
    .two {
        grid-template-columns:1fr;
    }
    .line {
        padding-left:16px;
        padding-right:16px;
    }
    .login {
        margin:40px auto;
        padding:0 16px;
    }
    .login section {
        padding:22px;
    }
}
@media(forced-colors:active) {
    section,input,select {
        border-color:CanvasText;
    }
    button {
        border:1px solid ButtonText;
    }
    .dot {
        background:CanvasText;
    }
}
"#;

#[derive(Clone, Copy)]
enum Tone {
    Good,
    Warning,
    Serious,
    Critical,
}
fn dot(tone: Tone) -> String {
    let class = match tone {
        Tone::Good => "good",
        Tone::Warning => "warning",
        Tone::Serious => "serious",
        Tone::Critical => "critical",
    };
    format!(r#"<span class="dot {class}"></span>"#)
}
fn state_tone(state: State) -> Tone {
    match state {
        State::Running => Tone::Good,
        State::QuotaWait | State::SignInRequired => Tone::Warning,
        State::RuntimeUnavailable => Tone::Serious,
        State::Lockout => Tone::Critical,
    }
}
fn state_shade(state: State) -> &'static str {
    match state {
        State::Running | State::QuotaWait => "light",
        State::SignInRequired | State::RuntimeUnavailable | State::Lockout => "dark",
    }
}
fn runtime_process_tone(runtime_process: RuntimeProcess) -> Tone {
    match runtime_process {
        RuntimeProcess::Running => Tone::Good,
        RuntimeProcess::Starting | RuntimeProcess::Stopping => Tone::Warning,
        RuntimeProcess::Stopped => Tone::Serious,
    }
}
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn figure(label: &str, value: impl std::fmt::Display) -> String {
    format!(
        "<p>{} <span>{}</span></p>",
        escape(label),
        escape(&value.to_string())
    )
}
fn post_form(action: &str, content: &str) -> String {
    format!(r#"<form method="post" action="{action}">{content}</form>"#)
}
fn submit(label: &str) -> String {
    format!(r#"<button type="submit">{label}</button>"#)
}
pub(super) struct Line<'a> {
    pub(super) text: &'a str,
    pub(super) error: bool,
}
fn line(line: &Line<'_>) -> String {
    let text = escape(line.text);
    let (tone, role) = if line.error {
        (Tone::Critical, "alert")
    } else {
        (Tone::Good, "status")
    };
    let dot = dot(tone);
    format!(r#"<p class="line" role="{role}">{dot}{text}</p>"#)
}
fn sign_in_wait(
    language: Language,
    wait: &crate::conductor::sign_in::Wait,
    now: crate::Moment,
) -> String {
    let seconds = wait.deadline.saturating_sub(now).div_ceil(1000);
    let time = format!(
        r#"<span id="sign-in-time" role="timer" data-seconds="{seconds}">{}:{:02}</span>"#,
        seconds / 60,
        seconds % 60
    );
    let waits = match language {
        Language::English => format!("acp2api waits for the sign-in in the browser: {time} left."),
        Language::Chinese => format!("正在等待浏览器完成登录，还剩 {time}。"),
    };
    let address = wait
        .address
        .as_ref()
        .map(|address| {
            format!(
                r#"{}<a href="{1}">{1}</a>"#,
                language.pick(
                    " If no browser opened, copy this address: ",
                    "如果浏览器没有打开，复制这个地址："
                ),
                escape(address)
            )
        })
        .unwrap_or_default();
    let cancel = post_form(
        "/manage/cancel-sign-in",
        &submit(language.pick("Cancel the sign-in", "取消登录")),
    );
    format!(r#"<p class="surface">{waits}{address}</p>{cancel}<script>{COUNTDOWN}</script>"#)
}
fn end_lockout(language: Language) -> String {
    post_form(
        "/manage/end-lockout",
        &format!(
            r#"<label for="end-lockout-password">{}</label><input type="password" id="end-lockout-password" name="password" autocomplete="current-password" required>{}"#,
            language.labels().management_password,
            submit(language.pick(
                "End the lockout with the management password",
                "输入管理密码，结束锁死",
            )),
        ),
    )
}
fn lockout_unsaved(language: Language) -> &'static str {
    language.pick(
        "acp2api: the lockout is not in the state file state.json, because the write failed. A restart of acp2api ends this lockout. acp2api tries the write again.",
        "acp2api：锁死没有写进状态文件 state.json，因为写入失败。重启 acp2api 会结束这次锁死。acp2api 会再次尝试写入。",
    )
}
fn first_tile(
    language: Language,
    view: &crate::conductor::View,
    now: crate::Moment,
    build: Option<&str>,
) -> String {
    let labels = language.labels();
    let shade = state_shade(view.state);
    let dot = dot(state_tone(view.state));
    let state = language.state(view.state);
    let mut paragraphs = view
        .lockout_evidence
        .as_ref()
        .map(|text| format!("<p>{}</p>", escape(text)))
        .unwrap_or_default();
    if view.lockout_unsaved {
        let text = lockout_unsaved(language);
        paragraphs.push_str(&format!(r#"<p role="status">{text}</p>"#));
    }
    if let Some(unavailable) = &view.runtime_unavailable {
        let text = escape(&language.runtime_fault(&unavailable.fault));
        paragraphs.push_str(&format!(r#"<p role="status">{text}</p>"#));
    }
    let action = if view.state == State::Lockout {
        end_lockout(language)
    } else {
        match &view.sign_in_wait {
            Some(wait) => sign_in_wait(language, wait, now),
            None => post_form(
                "/manage/sign-in",
                &submit(language.pick("Sign in with Google", "用 Google 账户登录")),
            ),
        }
    };
    let places = figure(labels.places_in_use, view.places);
    let queued = figure(labels.requests_in_the_queue, view.queued);
    let build = figure(labels.build_in_use, build.unwrap_or("—"));
    format!(
        r#"<section id="{entry_id}" class="{shade}"><h1>{dot}{state}</h1>{paragraphs}{action}<div class="figures">{places}{queued}{build}</div></section>"#,
        entry_id = crate::conductor::sign_in::ENTRY_ID
    )
}
fn document(language: Language, bars: &str, content: &str) -> String {
    format!(
        r#"<!doctype html><html lang="{}"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="color-scheme" content="light"><title>acp2api</title><style>{STYLE}</style></head><body>{bars}{content}</body></html>"#,
        language.html_tag()
    )
}
fn global_bar(switch: &str) -> String {
    format!(r#"<header class="global-bar"><strong>acp2api</strong>{switch}</header>"#)
}
fn language_switch(language: Language) -> String {
    let (value, tag, label) = match language {
        Language::English => ("zh", "zh-CN", "中文"),
        Language::Chinese => ("en", "en", "English"),
    };
    post_form(
        "/manage/language",
        &format!(
            r#"<button type="submit" name="language" value="{value}" lang="{tag}">{label}</button>"#
        ),
    )
}
fn section_bar(language: Language, state: State) -> String {
    let Labels {
        official_runtime,
        quota,
        places_and_queue,
        setup,
        logs,
        settings,
        notices,
        ..
    } = language.labels();
    let anchors = [
        ("runtime", official_runtime),
        ("quota", quota),
        ("places", places_and_queue),
        ("setup", setup),
        ("logs", logs),
        ("settings", settings),
        ("notices", notices),
    ]
    .map(|(id, title)| format!(r##"<a href="#{id}">{title}</a>"##))
    .concat();
    format!(
        r#"<div class="section-bar"><p class="panel-title">{}</p><p>{}{}</p><nav aria-label="{}">{anchors}</nav></div>"#,
        language.pick("Management panel", "管理面板"),
        dot(state_tone(state)),
        language.state(state),
        language.pick("Sections", "区块"),
    )
}
pub(super) fn login(language: Language, error: Option<&str>) -> String {
    let error = error
        .map(|text| line(&Line { text, error: true }))
        .unwrap_or_default();
    let title = language.pick("Welcome back.", "欢迎回来。");
    let intro = language.pick(
        "Enter the management password to open this panel.",
        "输入管理密码，打开这个面板。",
    );
    let password = language.labels().management_password;
    let open = submit(language.pick("Open the management panel", "打开管理面板"));
    let form = post_form(
        "/manage/login",
        &format!(
            r#"<label for="current-password">{password}</label><input type="password" id="current-password" name="password" autocomplete="current-password" required enterkeyhint="go">{open}"#
        ),
    );
    let first_start = language.pick(
        "Use the password shown in the terminal at the first start.",
        "使用首次启动时终端显示的密码。",
    );
    document(
        language,
        &global_bar(""),
        &format!(
            r#"<main class="login"><h1>{title}</h1><p class="intro">{intro}</p><section>{form}{error}</section><p class="fine">{first_start}</p></main>"#
        ),
    )
}
pub(super) struct Page<'a> {
    pub(super) language: Language,
    pub(super) settings: &'a crate::store::Settings,
    pub(super) highest: u8,
    pub(super) port: u16,
    pub(super) data_folder: &'a std::path::Path,
    pub(super) notice: Option<Line<'a>>,
    pub(super) notices: &'a [crate::conductor::Notice],
    pub(super) builds: Result<crate::runtime_builds::Summary, String>,
    pub(super) quota_result: &'a Option<crate::conductor::QuotaResult>,
    pub(super) history: &'a crate::journal::History,
    pub(super) view: &'a crate::conductor::View,
    pub(super) now: crate::Moment,
}
pub(super) fn settings(page: Page<'_>) -> String {
    let language = page.language;
    let bars = format!(
        "{}{}",
        global_bar(&language_switch(language)),
        section_bar(language, page.view.state)
    );
    let first_tile = first_tile(
        language,
        page.view,
        page.now,
        page.builds
            .as_ref()
            .ok()
            .and_then(|builds| builds.current.as_ref())
            .map(|build| build.version.as_str()),
    );
    let notices = notices_band(&page);
    let quota = quota_tile(&page);
    let setup = setup_tile(&page);
    let logs = logs_tile(language, page.history, page.data_folder);
    let settings = settings_tile(&page);
    let runtime = runtime(
        language,
        page.builds,
        page.settings,
        page.view.runtime_process,
    );
    document(
        language,
        &bars,
        &format!("<main>{first_tile}{notices}{runtime}{quota}{setup}{logs}{settings}</main>"),
    )
}

fn notices_band(page: &Page<'_>) -> String {
    let language = page.language;
    let dismiss = submit(language.pick("Dismiss", "知道了"));
    let rows = page
        .notices
        .iter()
        .map(|notice| {
            let text = escape(&language.built_in_tool_notice(&notice.name));
            let form = post_form(
                "/manage/notice",
                &format!(
                    r#"<input type="hidden" name="notice_id" value="{}">{dismiss}"#,
                    escape(&notice.id)
                ),
            );
            let dot = dot(Tone::Serious);
            format!(r#"<div class="notice-row"><p role="status">{dot}{text}</p>{form}</div>"#)
        })
        .collect::<String>();
    let band = if rows.is_empty() {
        String::new()
    } else {
        format!(
            "<section><h2>{}</h2>{rows}</section>",
            language.labels().notices
        )
    };
    let notice = page.notice.as_ref().map(line).unwrap_or_default();
    format!(r#"<div id="notices">{band}{notice}</div>"#)
}

fn quota_tile(page: &Page<'_>) -> String {
    let language = page.language;
    let labels = language.labels();
    let title = labels.quota;
    let result = page
        .quota_result
        .as_ref()
        .map(|result| {
            let text = match &result.text {
                Ok(text) => text.clone(),
                Err(message) => language.api_message(message),
            };
            format!(
                r#"<p>{}<time datetime="{time}">{time}</time></p><pre class="quota-result">{text}</pre>"#,
                language.pick("Last result: ", "上次结果："),
                time = escape(&result.time),
                text = escape(&text)
            )
        })
        .unwrap_or_else(|| {
            format!(
                "<p>{}</p>",
                language.pick("No quota result yet.", "还没有配额结果。")
            )
        });
    let refresh = post_form(
        "/manage/refresh-quota",
        &submit(language.pick("Refresh the quota", "刷新配额")),
    );
    let refresh_rule = language.pick("Refresh at most once every 60 s.", "每 60 秒最多刷新一次。");
    let places_title = labels.places_and_queue;
    let places = figure(labels.places_in_use, page.view.places);
    let queued = figure(labels.requests_in_the_queue, page.view.queued);
    let limit_form = concurrency_form(page);
    format!(
        r#"<section id="quota" aria-labelledby="quota-title"><div class="two"><div><h2 id="quota-title">{title}</h2>{result}{refresh}<p class="fine">{refresh_rule}</p></div><div id="places"><h2>{places_title}</h2>{places}{queued}{limit_form}</div></div></section>"#
    )
}

fn concurrency_form(page: &Page<'_>) -> String {
    let language = page.language;
    let options = (1..=page.highest)
        .map(|limit| {
            format!(
                "<option value=\"{limit}\"{}>{limit}</option>",
                if limit == page.settings.concurrency_limit {
                    " selected"
                } else {
                    ""
                }
            )
        })
        .collect::<String>();
    post_form(
        "/manage/concurrency",
        &format!(
            r#"<label for="concurrency-limit">{}</label><select id="concurrency-limit" name="concurrency_limit">{options}</select>{}"#,
            language.labels().concurrency_limit,
            submit(language.pick("Save the concurrency limit", "保存并发上限")),
        ),
    )
}

fn settings_tile(page: &Page<'_>) -> String {
    let language = page.language;
    let labels = language.labels();
    let title = labels.settings;
    let limit = figure(labels.concurrency_limit, page.settings.concurrency_limit);
    let port = figure(language.pick("Port", "端口"), page.port);
    let folder_label = language.pick("Data folder", "数据文件夹");
    let data_folder = escape(&page.data_folder.display().to_string());
    let change = language.pick(
        "Change the port through <code>config.toml</code> or <code>--port</code>. Change the data folder through <code>--data-dir</code>.",
        "通过 <code>config.toml</code> 或 <code>--port</code> 修改端口。通过 <code>--data-dir</code> 修改数据文件夹。",
    );
    let password_title = labels.management_password;
    let password_rule = language.pick(
        "Changing the password ends every panel session.",
        "修改密码会结束每个面板会话。",
    );
    let password_form = post_form(
        "/manage/password",
        &format!(
            r#"<label for="new-password">{}</label><input type="password" id="new-password" name="password" autocomplete="new-password" required>{}"#,
            language.pick("New management password", "新管理密码"),
            submit(language.pick("Change the management password", "修改管理密码")),
        ),
    );
    format!(
        r#"<section id="settings"><h2>{title}</h2>{limit}{port}<p>{folder_label} <code>{data_folder}</code></p><p class="fine">{change}</p><h3>{password_title}</h3><p class="fine">{password_rule}</p>{password_form}</section>"#
    )
}

fn client_setup_texts(page: &Page<'_>, address: &str) -> [String; 3] {
    let model = page
        .view
        .model_names
        .first()
        .map(String::as_str)
        .unwrap_or("<MODEL_NAME>");
    let shell_model = format!("'{}'", model.replace('\'', "'\\''"));
    let window = page
        .view
        .context_sizes
        .get(model)
        .map(u64::to_string)
        .unwrap_or_else(|| "<CONTEXT_WINDOW>".into());
    let claude = format!(
        r#"export ANTHROPIC_BASE_URL="{address}"
export ANTHROPIC_AUTH_TOKEN="<LOCAL_API_KEY>"
export ANTHROPIC_MODEL={shell_model}
export ANTHROPIC_DEFAULT_HAIKU_MODEL={shell_model}
export ANTHROPIC_DEFAULT_SONNET_MODEL={shell_model}
export ANTHROPIC_DEFAULT_OPUS_MODEL={shell_model}
export CLAUDE_CODE_SUBAGENT_MODEL={shell_model}
export CLAUDE_CODE_MAX_CONTEXT_TOKENS={window}"#
    );
    let codex = format!(
        r#"model = {model}
model_context_window = {window}
model_provider = "acp2api"

[model_providers.acp2api]
name = "acp2api"
base_url = "{address}/v1"
wire_api = "responses"
experimental_bearer_token = "<LOCAL_API_KEY>""#,
        model = toml::Value::String(model.into())
    );
    let chat = serde_json::to_string_pretty(
        &serde_json::json!({"base_url":format!("{address}/v1"),"api_key":"<LOCAL_API_KEY>"}),
    )
    .unwrap();
    [claude, codex, chat]
}

fn local_api_key_entry(language: Language, key: &str) -> String {
    let key_label = language.pick("Local API key", "本地 API 密钥");
    let copy = language.pick("Copy local API key", "复制本地 API 密钥");
    let copied = language.pick("Local API key copied.", "本地 API 密钥已复制。");
    let copy_failed = language.pick(
        "Copy failed. Use Show key to copy it.",
        "复制失败。请用“显示密钥”复制。",
    );
    let show = language.pick("Show key", "显示密钥");
    let new_key_rule = language.pick(
        "A new key replaces the current key immediately.",
        "新密钥立刻替换当前密钥。",
    );
    let new_key = post_form(
        "/manage/key",
        &submit(language.pick("Make a new key", "换一个新密钥")),
    );
    format!(
        r#"<label for="local-api-key">{key_label}</label>
<input type="password" id="local-api-key" value="{key}" readonly autocomplete="off">
<button type="button" id="copy-key" data-copied="{copied}" data-failed="{copy_failed}">{copy}</button>
<p id="copy-key-status" role="status"></p>
<details><summary>{show}</summary><code>{key}</code></details>
<p class="fine">{new_key_rule}</p>
{new_key}"#
    )
}

fn model_names(page: &Page<'_>) -> String {
    if page.view.model_names.is_empty() {
        return post_form(
            "/manage/models",
            &submit(
                page.language
                    .pick("Read the model names now", "现在读取模型名"),
            ),
        );
    }
    let names = page
        .view
        .model_names
        .iter()
        .map(|name| format!("<li><code>{}</code></li>", escape(name)))
        .collect::<String>();
    format!("<ul>{names}</ul>")
}

fn setup_tile(page: &Page<'_>) -> String {
    let language = page.language;
    let address = format!("http://127.0.0.1:{}", page.port);
    let [claude, codex, chat] = client_setup_texts(page, &address).map(|text| escape(&text));
    let title = language.labels().setup;
    let address_label = language.pick("Address", "地址");
    let key = local_api_key_entry(language, &escape(&page.settings.local_api_key));
    let models_title = language.pick("Model names", "模型名");
    let models = model_names(page);
    let placeholders = language.pick(
        "Replace the placeholders below. Copy the local API key separately.",
        "替换下面的占位符。本地 API 密钥要单独复制。",
    );
    let chat_title = language.pick("Chat Completions client", "Chat Completions 客户端");
    format!(
        r#"<section id="setup">
<h2>{title}</h2>
<p>{address_label} <code>{address}</code></p>
{key}
<h3>{models_title}</h3>
{models}
<p>{placeholders}</p>
<h3>Claude Code</h3>
<pre>{claude}</pre>
<h3>Codex</h3>
<pre>{codex}</pre>
<h3>{chat_title}</h3>
<pre>{chat}</pre>
<script>{COPY_KEY}</script>
</section>"#
    )
}

fn table_cells(values: &[String]) -> String {
    values
        .iter()
        .map(|value| format!("<td>{}</td>", escape(value)))
        .collect()
}

fn request_row(language: Language, record: &crate::journal::RequestRecord) -> String {
    use crate::journal::{ResultKind, SessionUse};
    let session_use = record
        .session_use
        .map(|value| match value {
            SessionUse::New => language.pick("new", "新建"),
            SessionUse::Continue => language.pick("continue", "继续"),
            SessionUse::Rebuild => language.pick("rebuild", "重建"),
        })
        .unwrap_or_default();
    let (tone, result) = match record.result {
        ResultKind::Success => (Tone::Good, language.pick("success", "成功")),
        ResultKind::Refused => (Tone::Warning, language.pick("refused", "已拒绝")),
        ResultKind::Failed => (Tone::Serious, language.pick("failed", "失败")),
        ResultKind::Disconnected => (Tone::Warning, language.pick("disconnected", "已断开")),
    };
    let before = table_cells(&[
        record.time.clone(),
        record.request_id.clone(),
        record.api.log_name().into(),
        record.model.clone().unwrap_or_default(),
        record.session.clone().unwrap_or_default(),
        session_use.into(),
        record.duration_ms.to_string(),
        record
            .input_tokens
            .map(|value| value.to_string())
            .unwrap_or_default(),
        record
            .output_tokens
            .map(|value| value.to_string())
            .unwrap_or_default(),
    ]);
    let after = table_cells(&[
        record.error_code.clone().unwrap_or_default(),
        record.unknown_fields.join(", "),
    ]);
    let dot = dot(tone);
    format!("<tr>{before}<td class=\"result\">{dot}{result}</td>{after}</tr>")
}

fn logs_tile(
    language: Language,
    history: &crate::journal::History,
    data_folder: &std::path::Path,
) -> String {
    let headings = [
        language.pick("Time", "时间"),
        language.pick("Request ID", "请求 ID"),
        "API",
        language.pick("Model", "模型名"),
        language.pick("ACP session", "ACP 会话"),
        language.pick("Session use", "会话方式"),
        language.pick("Duration (ms)", "耗时（毫秒）"),
        language.pick("Input tokens", "输入 token"),
        language.pick("Output tokens", "输出 token"),
        language.pick("Result", "结果"),
        language.pick("Error code", "错误码"),
        language.pick("Unknown fields", "未知字段"),
    ]
    .map(|label| format!("<th scope=\"col\">{label}</th>"))
    .concat();
    let requests = history
        .requests
        .iter()
        .rev()
        .map(|record| request_row(language, record))
        .collect::<String>();
    let unknown_fields = history
        .unknown_fields
        .iter()
        .flat_map(|(name, rows)| rows.iter().map(move |(api, row)| (name, api, row)))
        .map(|(name, api, row)| {
            format!(
                "<tr><th scope=\"row\">{}</th><td>{}</td><td>{}</td><td>{}</td></tr>",
                escape(name),
                api.display_name(),
                row.count,
                escape(&row.last_time)
            )
        })
        .collect::<String>();
    let other_names = history
        .other_names
        .as_ref()
        .map(|row| {
            format!(
                "<tr><th scope=\"row\">{}</th><td>{}</td><td>{}</td><td>{}</td></tr>",
                language.pick("other names", "其他名称"),
                language.pick("All APIs", "所有 API"),
                row.count,
                escape(&row.last_time)
            )
        })
        .unwrap_or_default();
    let limit = crate::journal::REQUEST_HISTORY_LIMIT;
    let title = language.labels().logs;
    let folder_label = language.pick("Log folder", "日志文件夹");
    let folder = escape(&data_folder.join("logs").display().to_string());
    let requests_caption = match language {
        Language::English => format!("Last {limit} requests"),
        Language::Chinese => format!("最近 {limit} 个请求"),
    };
    let unknown_caption = language.pick("Unknown fields since start", "启动以来的未知字段");
    let field = language.pick("Field name", "字段名");
    let count = language.pick("Count", "次数");
    let last = language.pick("Last time", "最近一次");
    format!(
        r#"<section id="logs"><h2>{title}</h2><p>{folder_label} <code>{folder}</code></p><div class="log-table"><table><caption>{requests_caption}</caption><thead><tr>{headings}</tr></thead><tbody>{requests}</tbody></table></div><div class="log-table"><table><caption>{unknown_caption}</caption><thead><tr><th scope="col">{field}</th><th scope="col">API</th><th scope="col">{count}</th><th scope="col">{last}</th></tr></thead><tbody>{unknown_fields}{other_names}</tbody></table></div></section>"#
    )
}

fn current_build(language: Language, build: Option<crate::runtime_builds::CurrentBuild>) -> String {
    let Some(build) = build else {
        return format!(
            "<p>{}</p>",
            language.pick("No build in use.", "没有在用的构建。")
        );
    };
    let source = build
        .source
        .map(|source| format!("<code>{}</code>", escape(&source.display().to_string())))
        .unwrap_or_else(|| language.pick("Not found.", "没有找到。").into());
    format!(
        "{}<p>{}</p><p>{}{source}</p>",
        figure(language.labels().build_in_use, &build.version),
        language.build_acceptance(build.acceptance),
        language.pick("Source: ", "来源："),
    )
}

fn found_row(language: Language, row: &crate::runtime_builds::BuildRow) -> String {
    let check = if row.version.is_none() {
        format!(
            r#"<form method="post" action="/manage/compatibility-check" aria-describedby="compatibility-cost compatibility-risks">
<input type="hidden" name="build" value="{}">
{}
</form>"#,
            escape(&row.identity),
            submit(language.pick("Run the compatibility check", "做兼容性检查")),
        )
    } else {
        String::new()
    };
    let version = row
        .version
        .as_deref()
        .unwrap_or(language.pick("Not an accepted version", "不是已接受版本"));
    format!(
        "<li><code>{}</code> — {} {}{check}</li>",
        escape(&row.path.display().to_string()),
        escape(version),
        row.acceptance
            .map(|acceptance| language.build_acceptance(acceptance))
            .unwrap_or_default()
    )
}

fn newer_build(language: Language, version: &str) -> String {
    let found = match language {
        Language::English => format!("New build found: {version}"),
        Language::Chinese => format!("发现新构建 {version}"),
    };
    let dot = dot(Tone::Warning);
    format!("<p role=\"status\">{dot}{}</p>", escape(&found))
}

fn runtime_path_form(language: Language, settings: &crate::store::Settings) -> String {
    let path = settings
        .agy_acp_server_path
        .as_ref()
        .map(|path| escape(&path.display().to_string()))
        .unwrap_or_default();
    let label = language.pick("Path of agy_acp_server", "agy_acp_server 的路径");
    let save = submit(language.pick(
        "Set the path of agy_acp_server",
        "填写 agy_acp_server 的路径",
    ));
    post_form(
        "/manage/runtime-path",
        &format!(
            r#"
<label for="runtime-path">{label}</label>
<input id="runtime-path" name="agy_acp_server_path" value="{path}" required>
{save}
"#
        ),
    )
}

fn accepted_build_form(language: Language, choices: &[(String, String)]) -> String {
    let options = choices
        .iter()
        .map(|(identity, version)| {
            format!(
                "<option value=\"{}\">{}</option>",
                escape(identity),
                escape(version)
            )
        })
        .collect::<String>();
    let label = language.pick("Accepted build", "已接受的构建");
    let save = submit(language.pick("Use this build", "使用这个构建"));
    post_form(
        "/manage/build",
        &format!(
            r#"
<label for="runtime-build">{label}</label>
<select id="runtime-build" name="build">{options}</select>
{save}
"#
        ),
    )
}

fn runtime(
    language: Language,
    builds: Result<crate::runtime_builds::Summary, String>,
    settings: &crate::store::Settings,
    runtime_process: RuntimeProcess,
) -> String {
    let title = language.labels().official_runtime;
    let crate::runtime_builds::Summary {
        current,
        found,
        release,
        newer,
        choices,
    } = match builds {
        Ok(summary) => summary,
        Err(error) => {
            return format!(
                "<section id=\"runtime\"><h2>{title}</h2><p role=\"status\">{}</p></section>",
                escape(&error)
            );
        }
    };
    let current = current_build(language, current);
    let process_label = language.pick("Runtime process", "运行时进程");
    let process_dot = dot(runtime_process_tone(runtime_process));
    let process_word = language.runtime_process(runtime_process);
    let newer = newer
        .map(|version| newer_build(language, &version))
        .unwrap_or_default();
    let release_label =
        language.build_acceptance(crate::runtime_builds::BuildAcceptance::OnTheReleaseList);
    let release = release
        .iter()
        .map(|version| format!("<li>{} — {release_label}</li>", escape(version)))
        .collect::<String>();
    let retry = post_form("/manage/retry", &submit(language.pick("Retry", "重试")));
    let search = post_form(
        "/manage/search",
        &submit(language.pick("Search again", "重新搜索")),
    );
    let cost = language.pick(
        "A compatibility check uses 3 to 4 model requests through your Google account.",
        "一次兼容性检查通过你的 Google 账户发送 3 到 4 次模型请求。",
    );
    let risks = language.pick(
        "The check does not check the stderr lines of the lockout signals, three prompt turns at the same time, or the memory that decides N.",
        "这个检查不检查锁死信号的 stderr 行、三个同时运行的 prompt 轮次，以及决定 N 的内存。",
    );
    let rows = found
        .iter()
        .map(|row| found_row(language, row))
        .collect::<String>();
    let path_form = runtime_path_form(language, settings);
    let build_form = accepted_build_form(language, &choices);
    format!(
        r#"<section id="runtime">
<h2>{title}</h2>
{current}
<p>{process_label} <span>{process_dot}{process_word}</span></p>
{newer}
<ul>{release}</ul>
<div class="actions">{retry}{search}</div>
<p id="compatibility-cost">{cost}</p>
<p id="compatibility-risks">{risks}</p>
<ul>{rows}</ul>
{path_form}
{build_form}
</section>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn render(
        view: &crate::conductor::View,
        version: &str,
        source: &Path,
        data_folder: &Path,
        history: &crate::journal::History,
    ) -> String {
        let settings = crate::store::Settings {
            port: 8329,
            concurrency_limit: 2,
            local_api_key: "acp2api-test".into(),
            management_password_hash: String::new(),
            agy_acp_server_path: None,
        };
        let builds = crate::runtime_builds::Summary {
            current: Some(crate::runtime_builds::CurrentBuild {
                version: version.into(),
                acceptance: crate::runtime_builds::BuildAcceptance::OnTheReleaseList,
                source: Some(source.into()),
            }),
            found: vec![],
            release: vec![],
            choices: vec![],
            newer: None,
        };
        super::settings(Page {
            language: Language::English,
            settings: &settings,
            highest: 3,
            port: 8329,
            data_folder,
            notice: None,
            notices: &[],
            builds: Ok(builds),
            quota_result: &None,
            history,
            view,
            now: 0,
        })
    }

    #[test]
    fn empty_log_cells_are_blank_and_present_zero_is_visible() {
        let mut history = crate::journal::History::default();
        history
            .requests
            .push_back(std::sync::Arc::new(crate::journal::RequestRecord {
                time: "1970-01-01T00:00:00Z".into(),
                request_id: "req-example".into(),
                api: crate::journal::Api::Chat,
                model: None,
                session: None,
                session_use: None,
                duration_ms: 7,
                input_tokens: None,
                output_tokens: Some(0),
                result: crate::journal::ResultKind::Success,
                error_code: None,
                unknown_fields: vec!["field<&>".into()],
            }));
        let html = render(
            &crate::conductor::View::default(),
            "1.4.0",
            Path::new("source"),
            Path::new("data"),
            &history,
        );
        let row = html
            .split("<caption>Last 50 requests</caption>")
            .nth(1)
            .unwrap()
            .split("<tbody>")
            .nth(1)
            .unwrap()
            .split("</tbody>")
            .next()
            .unwrap();
        assert!(!row.contains("null"), "missing optional metadata is blank");
        assert_eq!(row.matches("<td></td>").count(), 5);
        assert!(row.contains("<td>0</td>"), "a reported zero is a value");
        assert!(
            row.contains("field&lt;&amp;&gt;"),
            "field names stay escaped"
        );
    }

    #[test]
    fn the_login_page_has_the_language_of_the_browser_and_no_switch() {
        let html = login(Language::Chinese, None);
        assert_eq!(
            (
                html.contains("<html lang=\"zh-CN\">"),
                html.contains("输入管理密码"),
                html.contains("/manage/language"),
            ),
            (true, true, false),
            "FR-009 a language POST needs a panel session"
        );
    }

    #[test]
    fn the_quota_tile_reads_occupied_and_queued_counts_from_the_view() {
        let view = crate::conductor::View {
            places: 1,
            queued: 2,
            ..Default::default()
        };
        let html = render(
            &view,
            "1.4.0",
            Path::new("source"),
            Path::new("data"),
            &crate::journal::History::default(),
        );
        let quota = html
            .split("<h2 id=\"quota-title\">Quota</h2>")
            .nth(1)
            .unwrap()
            .split("</section>")
            .next()
            .unwrap();
        assert_eq!(
            [
                quota.contains("<p>Places in use <span>1</span></p>"),
                quota.contains("<p>Requests in the queue <span>2</span></p>"),
            ],
            [true, true],
            "FR-054 the quota tile uses both live counts, not fixed zeroes"
        );
    }

    #[test]
    fn build_version_source_and_data_folder_are_escaped_in_their_tiles() {
        let html = render(
            &crate::conductor::View::default(),
            "1<\"&",
            Path::new("source<\"&"),
            Path::new("data<\"&"),
            &crate::journal::History::default(),
        );
        let runtime = html
            .split("<h2>Official runtime</h2>")
            .nth(1)
            .unwrap()
            .split("</section>")
            .next()
            .unwrap();
        let settings = html
            .split("<h2>Settings</h2>")
            .nth(1)
            .unwrap()
            .split("</section>")
            .next()
            .unwrap();
        assert_eq!(
            [
                runtime.contains("<p>Build in use <span>1&lt;&quot;&amp;</span></p>"),
                runtime.contains("<p>Source: <code>source&lt;&quot;&amp;</code></p>"),
                settings.contains("<p>Data folder <code>data&lt;&quot;&amp;</code></p>"),
            ],
            [true, true, true],
            "FR-054 dynamic build and folder text is HTML-escaped"
        );
    }

    #[test]
    fn the_first_tile_renders_the_runtime_unavailable_reason_in_both_languages() {
        use crate::messages::{RuntimeFault, Unavailable};
        let cases = [
            (
                RuntimeFault::NoExecutable,
                "acp2api found no official runtime, so it started no runtime process.",
                "没有找到官方运行时，所以 acp2api 没有启动运行时进程。",
            ),
            (
                RuntimeFault::NotAccepted,
                "No build that acp2api found is an accepted version, so it started no runtime process.",
                "找到的构建不是已接受版本，所以 acp2api 没有启动运行时进程。",
            ),
            (
                RuntimeFault::RepeatedFailures,
                "The runtime process failed 3 times in 10 minutes, so acp2api does not start it again by itself.",
                "运行时进程在 10 分钟内失败了 3 次，所以 acp2api 不再自动启动它。",
            ),
            (
                RuntimeFault::CopyFailed {
                    version: "1<4".into(),
                },
                "The copy of the accepted build 1&lt;4 failed, so acp2api started no runtime process.",
                "已接受构建 1&lt;4 的复制失败了，所以 acp2api 没有启动运行时进程。",
            ),
            (
                RuntimeFault::StateFileNotWritable,
                "acp2api cannot write its state file state.json, so it started no runtime process.",
                "acp2api 无法写入状态文件 state.json，所以没有启动运行时进程。",
            ),
        ];
        for (fault, english, chinese) in cases {
            let view = crate::conductor::View {
                state: State::RuntimeUnavailable,
                runtime_unavailable: Some(Unavailable {
                    fault,
                    versions: "1.4.0".into(),
                    port: 8329,
                }),
                ..Default::default()
            };
            for (language, sentence, m14_pointer) in [
                (Language::English, english, "See "),
                (Language::Chinese, chinese, "详见"),
            ] {
                let tile = first_tile(language, &view, 0, None);
                assert!(
                    tile.contains(&format!(r#"<p role="status">{sentence}</p>"#)),
                    "FR-054 the first tile shows the PRD sentence {sentence:?}: {tile}"
                );
                assert!(
                    !tile.contains(m14_pointer),
                    "FR-054 the first tile shows no M-14 text: {tile}"
                );
            }
        }
        let running = crate::conductor::View {
            state: State::Running,
            runtime_unavailable: None,
            ..Default::default()
        };
        for language in [Language::English, Language::Chinese] {
            assert!(
                !first_tile(language, &running, 0, None).contains(r#"<p role="status">"#),
                "FR-054 the state Running has no sentence of a runtime unavailable reason"
            );
        }
    }
}
