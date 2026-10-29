pub(super) const COUNTDOWN: &str = r#"const timer = document.getElementById('sign-in-time');
const seconds = Number(timer.dataset.seconds);
const started = performance.now();
function update() {
    const left = Math.max(0, seconds - Math.floor((performance.now() - started) / 1000));
    timer.textContent = `${Math.floor(left / 60)}:${String(left % 60).padStart(2, '0')}`;
    if (left > 0) setTimeout(update, 1000);
}
update();"#;
pub(super) fn sign_in_section(
    view: &crate::conductor::View,
    now: crate::Moment,
    build: Option<&str>,
) -> String {
    let sign_in = if let Some(wait) = &view.sign_in_wait {
        let seconds = wait.deadline.saturating_sub(now).div_ceil(1000);
        let address = wait
            .address
            .as_ref()
            .map(|address| {
                format!(
                    " If no browser opened, copy this address: <a href=\"{0}\">{0}</a>",
                    escape(address)
                )
            })
            .unwrap_or_default();
        format!(
            r#"<p>acp2api waits for the sign-in in the browser: <span id="sign-in-time" role="timer" data-seconds="{seconds}">{}:{:02}</span> left.{address}</p><form method="post" action="/manage/cancel-sign-in"><button type="submit">Cancel the sign-in</button></form><script>{COUNTDOWN}</script>"#,
            seconds / 60,
            seconds % 60
        )
    } else {
        r#"<form method="post" action="/manage/sign-in"><button type="submit">Sign in</button></form>"#.into()
    };
    format!(
        r#"<section id="{entry_id}"><h2>{}</h2><p>Places in use <span>{}</span></p><p>Requests in the queue <span>{}</span></p><p>Build in use <span>{}</span></p>{sign_in}</section>"#,
        view.state,
        view.places,
        view.queued,
        escape(build.unwrap_or("—")),
        entry_id = crate::conductor::sign_in::ENTRY_ID
    )
}
fn document(content: &str) -> String {
    format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="color-scheme" content="light"><title>acp2api</title><style>
:root{{color-scheme:light;font:17px/1.5 -apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;color:#1d1d1f;background:#f5f5f7}}*{{box-sizing:border-box}}body{{margin:0}}header{{background:#fff;border-bottom:1px solid #d2d2d7;padding:18px max(24px,calc((100% - 960px)/2))}}header strong{{font-size:21px;letter-spacing:-.3px}}nav{{padding:12px max(24px,calc((100% - 960px)/2));background:#ffffffb3;backdrop-filter:blur(18px);border-bottom:1px solid #d2d2d7;font-size:14px}}main{{max-width:960px;margin:60px auto;padding:0 24px}}h1{{font-size:40px;line-height:1.15;letter-spacing:-1px;margin:0 0 16px}}h2{{font-size:21px;margin:0 0 18px}}p{{max-width:65ch}}.intro{{color:#616166;margin-bottom:36px}}section{{background:#fff;border:1px solid #d2d2d7;border-radius:18px;padding:28px;margin:20px 0}}.login{{max-width:540px;margin:80px auto}}label{{display:block;margin-bottom:8px}}input,select,button,summary{{font:inherit}}input,select{{border:1px solid #86868b;border-radius:999px;padding:12px 18px;max-width:100%;width:100%;background:#fff;color:#1d1d1f}}input:focus-visible,select:focus-visible,button:focus-visible,summary:focus-visible{{outline:3px solid #0066cc;outline-offset:3px}}button{{background:#0066cc;color:#fff;border:0;border-radius:999px;padding:12px 22px;cursor:pointer;margin-top:18px}}summary{{color:#0066cc;cursor:pointer;padding:12px 0}}code{{overflow-wrap:anywhere;font-size:14px}}.quota-result{{white-space:pre-wrap;overflow-wrap:anywhere;font:inherit}}.fine{{font-size:14px;color:#616166}}.notice{{border-top:1px solid #d2d2d7;padding-top:16px;font-size:14px}}@media(max-width:600px){{main{{margin:36px auto}}section{{padding:22px}}h1{{font-size:34px}}.login{{margin:40px auto}}}}@media(forced-colors:active){{section,input,select{{border-color:CanvasText}}button{{border:1px solid ButtonText}}}}
</style></head><body><header><strong>acp2api</strong></header><nav>Management panel</nav>{content}</body></html>"#
    )
}
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
pub(super) fn login(error: Option<&str>) -> String {
    let error = error
        .map(|e| format!("<p class=\"notice\" role=\"alert\">{}</p>", escape(e)))
        .unwrap_or_default();
    document(&format!(
        r#"<main class="login"><h1>Welcome back.</h1><p class="intro">Enter the management password to open this panel.</p><section><form method="post" action="/manage/login"><label for="current-password">Management password</label><input type="password" id="current-password" name="password" autocomplete="current-password" required enterkeyhint="go"><button type="submit">Open the management panel</button></form>{error}</section><p class="fine">Use the password shown in the terminal at the first start.</p></main>"#
    ))
}
pub(super) fn settings(
    settings: &crate::store::Settings,
    panel: &super::Panel,
    notice: Option<&str>,
    notices: &[crate::conductor::Notice],
    runtime: &str,
    quota_result: &Option<crate::conductor::QuotaResult>,
    sign_in_section: &str,
) -> String {
    let key = escape(&settings.local_api_key);
    let port = panel.port;
    let data_folder = escape(&panel.conductor.data_folder.display().to_string());
    let limit = settings.concurrency_limit;
    let options = (1..=panel.conductor.highest)
        .map(|limit| {
            format!(
                "<option value=\"{limit}\"{}>{limit}</option>",
                if limit == settings.concurrency_limit {
                    " selected"
                } else {
                    ""
                }
            )
        })
        .collect::<String>();
    let notice = notice
        .map(|text| format!("<p class=\"notice\" role=\"status\">{}</p>", escape(text)))
        .unwrap_or_default();
    let runtime_notices = notices.iter().map(|notice| format!(
        r#"<section><h2>Runtime notice</h2><p role="status">{}</p><form method="post" action="/manage/notice"><input type="hidden" name="notice_id" value="{}"><button type="submit">Dismiss notice</button></form></section>"#,
        escape(&notice.text()),escape(&notice.id)
    )).collect::<String>();
    let result = quota_result.as_ref().map(|result| format!(r#"<p>Last result: <time datetime="{time}">{time}</time></p><pre class="quota-result">{text}</pre>"#,time=escape(&result.time),text=escape(&result.text))).unwrap_or_else(|| "<p>No quota result yet.</p>".into());
    let quota = format!(
        r#"<section aria-labelledby="quota-title"><h2 id="quota-title">Quota</h2>{result}<form method="post" action="/manage/refresh-quota"><button type="submit">Refresh quota</button></form><p class="fine">Refresh at most once every 60 s.</p><form method="post" action="/manage/concurrency"><label for="concurrency-limit">Concurrency limit</label><select id="concurrency-limit" name="concurrency_limit">{options}</select><button type="submit">Save the concurrency limit</button></form></section>"#
    );
    document(&format!(
        r#"<main><h1>Settings</h1><p class="intro">Manage access to acp2api and the concurrency limit.</p>{sign_in_section}{runtime_notices}{notice}{runtime}{quota}<section><h2>Local API key</h2><label for="local-api-key">Local API key</label><input type="password" id="local-api-key" value="{key}" readonly autocomplete="off"><details><summary>Show key</summary><code>{key}</code></details><p class="fine">A new key replaces the current key immediately.</p><form method="post" action="/manage/key"><button type="submit">Make a new key</button></form></section><section><h2>Settings</h2><p>Concurrency limit <span>{limit}</span></p><p>Port <span>{port}</span></p><p>Data folder <code>{data_folder}</code></p><p class="fine">Change the port through <code>config.toml</code> or <code>--port</code>. Change the data folder through <code>--data-dir</code>.</p><h3>Management password</h3><p class="fine">Changing the password ends every panel session.</p><form method="post" action="/manage/password"><label for="new-password">New management password</label><input type="password" id="new-password" name="password" autocomplete="new-password" required><button type="submit">Change the management password</button></form></section></main>"#
    ))
}

pub(super) fn runtime(
    builds: Result<crate::runtime_builds::Summary, String>,
    settings: &crate::store::Settings,
    process: &str,
) -> String {
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
                "<section><h2>Official runtime</h2><p role=\"status\">{}</p></section>",
                escape(&error)
            );
        }
    };
    let current = current
        .map(|build| {
            format!(
                "<p>Build in use <span>{}</span> — {}</p><p>Source: <code>{}</code></p>",
                escape(&build.version),
                build.acceptance,
                escape(&build.source.display().to_string())
            )
        })
        .unwrap_or_else(|| "<p>No build in use.</p>".into());
    let release = release
        .iter()
        .map(|version| format!("<li>{} — On the release list</li>", escape(version)))
        .collect::<String>();
    let rows = found
        .iter()
        .map(|row| {
            let check = if row.version.is_none() {
                format!(r#"<form method="post" action="/manage/compatibility-check" aria-describedby="compatibility-cost compatibility-risks"><input type="hidden" name="build" value="{}"><button type="submit">Run the compatibility check</button></form>"#, escape(&row.identity))
            } else { String::new() };
            format!(
                "<li><code>{}</code> — {} {}{check}</li>",
                escape(&row.path.display().to_string()),
                escape(row.version.as_deref().unwrap_or("Not an accepted version")),
                row.acceptance.unwrap_or("")
            )
        })
        .collect::<String>();
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
    let newer = newer
        .map(|version| {
            format!(
                "<p role=\"status\">New build found: {}</p>",
                escape(&version)
            )
        })
        .unwrap_or_default();
    let path = settings
        .agy_acp_server_path
        .as_ref()
        .map(|path| escape(&path.display().to_string()))
        .unwrap_or_default();
    format!(
        r#"<section><h2>Official runtime</h2>{current}<p>Runtime process <span>{process}</span></p>{newer}<ul>{release}</ul><form method="post" action="/manage/retry"><button type="submit">Retry</button></form><form method="post" action="/manage/search"><button type="submit">Search for builds</button></form><p id="compatibility-cost">A compatibility check uses 3 to 4 model requests through your Google account.</p><p id="compatibility-risks">The check does not check the stderr lines of the lockout signals, three prompt turns at the same time, or the memory that decides N.</p><ul>{rows}</ul><form method="post" action="/manage/runtime-path"><label for="runtime-path">Path of agy_acp_server</label><input id="runtime-path" name="agy_acp_server_path" value="{path}" required><button type="submit">Save the runtime path</button></form><form method="post" action="/manage/build"><label for="runtime-build">Accepted build</label><select id="runtime-build" name="build">{options}</select><button type="submit">Use this build</button></form></section>"#
    )
}
