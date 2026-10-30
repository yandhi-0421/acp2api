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

// Texts that other modules give to the panel in English. A line that is not here,
// for example an OS error or an API message, stays as it is.
const CHINESE_LINES: &[(&str, &str)] = &[
    ("Running", "运行中"),
    ("Quota wait", "配额等待"),
    ("Sign-in required", "需要登录"),
    ("Runtime unavailable", "运行时不可用"),
    ("Lockout", "已锁死"),
    ("running", "运行中"),
    ("starting", "正在启动"),
    ("stopping", "正在停止"),
    ("stopped", "已停止"),
    ("On the release list", "发布列表"),
    ("Checked on this computer", "本机已检查"),
    (
        "This folder has no localharness_external, so acp2api did not use this path.",
        "这个文件夹里没有 localharness_external，acp2api 没有使用这个路径。",
    ),
    (
        "Enter an absolute path of agy_acp_server.",
        "填写 agy_acp_server 的绝对路径。",
    ),
    (
        "This path has no agy_acp_server.",
        "这个路径上没有 agy_acp_server。",
    ),
    (
        "Choose a build that is not an accepted version from the search results.",
        "从搜索结果里选择一个不是已接受版本的构建。",
    ),
    (
        "Choose an accepted build from the search results.",
        "从搜索结果里选择一个已接受版本的构建。",
    ),
    (
        "Sign-in cannot start while a sign-in or runtime replacement is in progress.",
        "登录或运行时进程替换正在进行，现在不能开始登录。",
    ),
    (
        "Wait 60 s between quota refreshes.",
        "两次刷新配额至少间隔 60 秒。",
    ),
    (
        "acp2api found no executable of the official runtime",
        "acp2api 没有找到官方运行时的可执行文件",
    ),
    (
        "no build that acp2api found is an accepted version",
        "acp2api 找到的构建都不是已接受版本",
    ),
    ("acp2api is exiting", "acp2api 正在退出"),
];

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
    pub(super) fn line(self, english: &str) -> String {
        if self == Self::English {
            return english.into();
        }
        if let Some((_, chinese)) = CHINESE_LINES.iter().find(|(line, _)| *line == english) {
            return (*chinese).into();
        }
        let sentence = |prefix: &str| english.strip_prefix(prefix)?.strip_suffix('.');
        if let Some(version) = sentence("The compatibility check passed for ") {
            return format!("构建 {version} 通过了兼容性检查。");
        }
        if let Some(error) = sentence("The compatibility check failed: ") {
            return format!("兼容性检查没有通过：{}。", self.line(error));
        }
        english.into()
    }
}

#[cfg(test)]
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

    #[test]
    fn lines_of_other_modules_keep_their_values_in_chinese() {
        let chinese = Language::Chinese;
        assert_eq!(
            [
                chinese.line("The compatibility check passed for 1.6.0."),
                chinese.line(
                    "The compatibility check failed: no build that acp2api found is an accepted version."
                ),
                chinese.line("permission denied"),
            ],
            [
                "构建 1.6.0 通过了兼容性检查。",
                "兼容性检查没有通过：acp2api 找到的构建都不是已接受版本。",
                "permission denied",
            ]
        );
    }
}
