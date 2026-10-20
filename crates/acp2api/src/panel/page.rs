fn document(content: &str) -> String {
    format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="color-scheme" content="light"><title>acp2api</title><style>
:root{{color-scheme:light;font:17px/1.5 -apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;color:#1d1d1f;background:#f5f5f7}}*{{box-sizing:border-box}}body{{margin:0}}header{{background:#fff;border-bottom:1px solid #d2d2d7;padding:18px max(24px,calc((100% - 960px)/2))}}header strong{{font-size:21px;letter-spacing:-.3px}}nav{{padding:12px max(24px,calc((100% - 960px)/2));background:#ffffffb3;backdrop-filter:blur(18px);border-bottom:1px solid #d2d2d7;font-size:14px}}main{{max-width:960px;margin:60px auto;padding:0 24px}}h1{{font-size:40px;line-height:1.15;letter-spacing:-1px;margin:0 0 16px}}h2{{font-size:21px;margin:0 0 18px}}p{{max-width:65ch}}.intro{{color:#616166;margin-bottom:36px}}section{{background:#fff;border:1px solid #d2d2d7;border-radius:18px;padding:28px;margin:20px 0}}.login{{max-width:540px;margin:80px auto}}label{{display:block;margin-bottom:8px}}input,select,button,summary{{font:inherit}}input,select{{border:1px solid #86868b;border-radius:999px;padding:12px 18px;max-width:100%;width:100%;background:#fff;color:#1d1d1f}}input:focus-visible,select:focus-visible,button:focus-visible,summary:focus-visible{{outline:3px solid #0066cc;outline-offset:3px}}button{{background:#0066cc;color:#fff;border:0;border-radius:999px;padding:12px 22px;cursor:pointer;margin-top:18px}}summary{{color:#0066cc;cursor:pointer;padding:12px 0}}code{{overflow-wrap:anywhere;font-size:14px}}.fine{{font-size:14px;color:#616166}}.notice{{border-top:1px solid #d2d2d7;padding-top:16px;font-size:14px}}@media(max-width:600px){{main{{margin:36px auto}}section{{padding:22px}}h1{{font-size:34px}}.login{{margin:40px auto}}}}@media(forced-colors:active){{section,input,select{{border-color:CanvasText}}button{{border:1px solid ButtonText}}}}
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
    highest: u8,
    notice: Option<&str>,
) -> String {
    let key = escape(&settings.local_api_key);
    let options = (1..=highest)
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
    document(&format!(
        r#"<main><h1>Settings</h1><p class="intro">Manage access to acp2api and the concurrency limit.</p>{notice}<section><h2>Local API key</h2><label for="local-api-key">Local API key</label><input type="password" id="local-api-key" value="{key}" readonly autocomplete="off"><details><summary>Show key</summary><code>{key}</code></details><p class="fine">A new key replaces the current key immediately.</p><form method="post" action="/manage/key"><button type="submit">Make a new key</button></form></section><section><h2>Concurrency limit</h2><form method="post" action="/manage/concurrency"><label for="concurrency-limit">Concurrency limit</label><select id="concurrency-limit" name="concurrency_limit">{options}</select><button type="submit">Save the concurrency limit</button></form></section><section><h2>Management password</h2><p class="fine">Changing the password ends every panel session.</p><form method="post" action="/manage/password"><label for="new-password">New management password</label><input type="password" id="new-password" name="password" autocomplete="new-password" required><button type="submit">Change the management password</button></form></section></main>"#
    ))
}
