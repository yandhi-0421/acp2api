use acp2api::{LaunchSpec, ProcessLauncher, ProcessTree, os::OsLauncher};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    path::Path,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::io::{AsyncRead, AsyncReadExt};

const CAP: u64 = 4 * 1024 * 1024;
const BLUE: [u8; 4] = [0, 102, 204, 255];
const INK: [u8; 4] = [29, 29, 31, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];
static CHROME: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Debug, Deserialize)]
pub struct Audit {
    pub browser: String,
    pub dark: bool,
    resources: usize,
    font_faces: Vec<String>,
    fonts: Vec<Font>,
    nodes: Vec<Node>,
}
#[derive(Debug, Deserialize)]
struct Font {
    family: String,
    status: String,
}
#[derive(Debug, Deserialize, PartialEq)]
struct Paint {
    css: BTreeMap<String, String>,
    colors: BTreeMap<String, [u8; 4]>,
}
#[derive(Debug, Deserialize)]
struct Node {
    index: usize,
    tag: String,
    role: String,
    href: bool,
    visible: bool,
    disabled: bool,
    rect: [f64; 2],
    text_beside: bool,
    paint: Paint,
    before: Paint,
    after: Paint,
    focus: Option<[Paint; 3]>,
    focus_active: bool,
    focus_visible: bool,
}

struct Folder(std::path::PathBuf);
impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct Tree {
    child: Option<Box<dyn ProcessTree>>,
    pid: u32,
}
impl Tree {
    async fn stop(&mut self) {
        self.child
            .take()
            .unwrap()
            .stop(Arc::new(acp2api::os::SystemClock::default()))
            .await;
        self.pid = 0;
    }
}
impl Drop for Tree {
    fn drop(&mut self) {
        // A cancelled Cucumber step must also stop Chrome's descendants on Unix.
        #[cfg(unix)]
        if self.pid != 0 {
            unsafe {
                libc::killpg(self.pid as i32, libc::SIGKILL);
            }
        }
        // On Windows, dropping the process tree closes its kill-on-close Job Object.
    }
}

pub async fn render(html: &str, root: &Path, dark: bool) -> Result<Audit, String> {
    let _single = CHROME.lock().await;
    #[cfg(target_os = "macos")]
    let chrome = Path::new("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome");
    #[cfg(target_os = "windows")]
    let chrome = Path::new(r"C:\Program Files\Google\Chrome\Application\chrome.exe");
    if !chrome.is_file() {
        return Err(format!(
            "Chrome audit: installed Google Chrome is missing at {}",
            chrome.display()
        ));
    }
    let folder = Folder(root.join(format!("panel-visual-{}", crate::setup::random_hex(8))));
    std::fs::create_dir(&folder.0).map_err(|error| format!("Chrome audit folder: {error}"))?;
    let profile = folder.0.join("profile");
    let page = folder.0.join("panel.html");
    let document = format!("{html}<script>{}</script>", include_str!("panel_visual.js"));
    std::fs::write(&page, document).map_err(|error| format!("Chrome audit HTML: {error}"))?;
    let mut args: Vec<std::ffi::OsString> = [
        "--headless",
        "--dump-dom",
        "--no-first-run",
        "--no-default-browser-check",
        "--disable-background-networking",
        "--disable-component-update",
        "--disable-sync",
        "--disable-default-apps",
        "--disable-domain-reliability",
        "--disable-extensions",
        "--host-resolver-rules=MAP * ~NOTFOUND",
        "--proxy-server=http://127.0.0.1:0",
        "--proxy-bypass-list=<-loopback>",
        "--window-size=1280,900",
        "--virtual-time-budget=2000",
    ]
    .map(Into::into)
    .into();
    args.push(format!("--user-data-dir={}", profile.display()).into());
    args.push(
        format!(
            "--blink-settings=preferredColorScheme={}",
            if dark { 0 } else { 1 }
        )
        .into(),
    );
    args.push(page.as_os_str().to_owned());
    let start = Instant::now();
    let launched = OsLauncher
        .launch(LaunchSpec {
            program: chrome.into(),
            args,
            env_set: Vec::new(),
            cwd: folder.0.clone(),
            own_temp: None,
        })
        .map_err(|error| format!("Chrome audit launch failed: {error:?}"))?;
    drop(launched.stdin);
    let mut tree = Tree {
        pid: launched.tree.pid(),
        child: Some(launched.tree),
    };
    let capture = async {
        tokio::try_join!(
            read_capped(launched.stdout),
            read_capped(launched.stderr),
            async { Ok::<_, String>(tree.child.as_mut().unwrap().exited().await) },
        )
    };
    let result = tokio::time::timeout(crate::real_limit(Duration::from_secs(30)), capture).await;
    tree.stop().await;
    let (stdout, stderr, status) = result
        .map_err(|_| "Chrome audit exceeded real_limit(30 s); process tree stopped".to_owned())??;
    eprintln!(
        "Chrome process: status={status}; elapsed_ms={}; unix_time={}",
        start.elapsed().as_millis(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    );
    if !status.success() {
        return Err(format!(
            "Chrome audit exited {status}: {}",
            String::from_utf8_lossy(&stderr)
        ));
    }
    let dom =
        String::from_utf8(stdout).map_err(|error| format!("Chrome audit stdout UTF-8: {error}"))?;
    let marker = "id=\"acp2api-visual-audit-result\"";
    let encoded = dom
        .split_once(marker)
        .and_then(|(_, tail)| tail.split_once('>'))
        .and_then(|(_, tail)| tail.split_once("</pre>"))
        .map(|(value, _)| value)
        .ok_or_else(|| {
            format!(
                "Chrome audit result is missing; stderr: {}",
                String::from_utf8_lossy(&stderr)
            )
        })?;
    let bytes = encoded
        .as_bytes()
        .chunks(2)
        .map(|pair| {
            std::str::from_utf8(pair)
                .ok()
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
                .ok_or_else(|| "Chrome audit has invalid hex JSON".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|error| format!("Chrome audit JSON: {error}"))?;
    if let Some(error) = value.get("error") {
        return Err(format!("Chrome audit script failed: {error}"));
    }
    let audit: Audit =
        serde_json::from_value(value).map_err(|error| format!("Chrome audit facts: {error}"))?;
    if audit.resources != 0 {
        return Err("Chrome audit refuses a document with <link> or <script src> or @import: file rendering cannot load those resources".into());
    }
    if audit.nodes.is_empty() {
        return Err("Chrome audit returned no elements".into());
    }
    if audit.dark != dark {
        return Err(format!(
            "Chrome audit preference failed: requested dark={dark}, matchMedia dark={}",
            audit.dark
        ));
    }
    eprintln!(
        "Chrome audit: {}; dark={}; elements={}; elapsed_ms={}; unix_time={}",
        audit.browser,
        audit.dark,
        audit.nodes.len(),
        start.elapsed().as_millis(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    );
    Ok(audit)
}

async fn read_capped(reader: impl AsyncRead + Unpin) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    reader
        .take(CAP + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| format!("Chrome audit output: {error}"))?;
    if bytes.len() as u64 > CAP {
        return Err("Chrome audit output exceeded 4 MiB".into());
    }
    Ok(bytes)
}

impl Node {
    fn control(&self) -> bool {
        matches!(self.tag.as_str(), "button" | "summary" | "select" | "input")
            || self.tag == "a" && self.href
            || self.role.split_whitespace().any(|role| {
                matches!(
                    role,
                    "button"
                        | "checkbox"
                        | "combobox"
                        | "gridcell"
                        | "link"
                        | "listbox"
                        | "menuitem"
                        | "menuitemcheckbox"
                        | "menuitemradio"
                        | "option"
                        | "radio"
                        | "scrollbar"
                        | "searchbox"
                        | "slider"
                        | "spinbutton"
                        | "switch"
                        | "tab"
                        | "textbox"
                        | "treeitem"
                )
            })
    }
    fn paints(&self) -> impl Iterator<Item = &Paint> {
        [&self.paint, &self.before, &self.after]
            .into_iter()
            .chain(self.focus.iter().flatten())
    }
    fn label(&self) -> String {
        format!("{} element {}", self.tag, self.index)
    }
}
impl Paint {
    fn rendered_colors(&self) -> impl Iterator<Item = (&str, &[u8; 4])> {
        self.colors
            .iter()
            .filter(|(name, rgba)| {
                if rgba[3] == 0 {
                    return false;
                }
                if let Some(side) = name
                    .strip_prefix("border-")
                    .and_then(|name| name.strip_suffix("-color"))
                {
                    if !matches!(side, "top" | "right" | "bottom" | "left") {
                        return false;
                    }
                    return self.css[&format!("border-{side}-style")] != "none"
                        && px(&self.css[&format!("border-{side}-width")]) > 0.;
                }
                if name.as_str() == "outline-color" {
                    return self.css["outline-style"] != "none"
                        && px(&self.css["outline-width"]) > 0.;
                }
                if name.as_str() == "text-decoration-color" {
                    return self.css["text-decoration-line"] != "none";
                }
                true
            })
            .map(|(name, color)| (name.as_str(), color))
    }
}

fn px(value: &str) -> f64 {
    value
        .strip_suffix("px")
        .unwrap_or(value)
        .parse()
        .expect("computed CSS length in px")
}
fn neutral(color: &[u8; 4]) -> bool {
    let rgb = &color[..3];
    rgb.iter().max().unwrap() - rgb.iter().min().unwrap() <= 6
}
fn rounded(node: &Node) -> bool {
    [
        "border-top-left-radius",
        "border-top-right-radius",
        "border-bottom-left-radius",
        "border-bottom-right-radius",
    ]
    .iter()
    .all(|name| {
        let radii: Vec<_> = node.paint.css[*name].split_whitespace().collect();
        [radii[0], *radii.get(1).unwrap_or(&radii[0])]
            .iter()
            .zip(node.rect)
            .all(|(radius, dimension)| {
                let pixels = radius
                    .strip_suffix('%')
                    .map(|percent| percent.parse::<f64>().unwrap() * dimension / 100.)
                    .unwrap_or_else(|| px(radius));
                pixels >= node.rect[0].min(node.rect[1]) / 2. - 0.05
            })
    })
}

pub fn controls(audit: &Audit) -> Result<(), String> {
    let controls: Vec<_> = audit
        .nodes
        .iter()
        .filter(|node| node.visible && node.control())
        .collect();
    if controls.is_empty() {
        return Err("FR-055 control audit found no visible controls".into());
    }
    for node in controls {
        for (index, paint) in node.paints().enumerate() {
            if matches!(index, 1 | 2 | 4 | 5)
                && (matches!(paint.css["content"].as_str(), "none" | "normal")
                    || paint.css["display"] == "none"
                    || paint.css["visibility"] != "visible"
                    || paint.css["opacity"] == "0")
            {
                continue;
            }
            for (property, color) in paint.rendered_colors() {
                let [r, g, b, _] = color.map(i32::from);
                let blue_hue = b > r && g >= r && b >= g && ((g - r) * 2 - (b - r)).abs() <= 1;
                if !neutral(color) && !blue_hue {
                    return Err(format!(
                        "FR-055 control {} has another hue in {property}: {color:?}",
                        node.label()
                    ));
                }
            }
        }
        if node.disabled {
            continue;
        }
        if node.tag == "button" || node.role == "button" {
            if node.paint.colors["background-color"] != BLUE || node.paint.colors["color"] != WHITE
            {
                return Err(format!(
                    "FR-055 control {} needs #0066cc background and #ffffff text",
                    node.label()
                ));
            }
        } else if node.tag == "summary" || node.tag == "a" && node.href || node.role == "link" {
            if node.paint.colors["color"] != BLUE {
                return Err(format!(
                    "FR-055 control {} needs #0066cc text",
                    node.label()
                ));
            }
        } else if matches!(node.tag.as_str(), "input" | "select") {
            let surface = matches!(
                node.paint.colors["background-color"],
                WHITE | [245, 245, 247, 255] | [39, 39, 41, 255]
            );
            let hairline = ["top", "right", "bottom", "left"].iter().all(|side| {
                let width = px(&node.paint.css[&format!("border-{side}-width")]);
                width > 0.
                    && width <= 1.
                    && neutral(&node.paint.colors[&format!("border-{side}-color")])
            });
            if !surface || !hairline || !rounded(node) || node.paint.colors["color"] != INK {
                return Err(format!(
                    "FR-055 control {} needs a pill input with neutral hairline, ink text and surface background",
                    node.label()
                ));
            }
            let focus = node
                .focus
                .as_ref()
                .expect("audit focuses visible enabled inputs and selects");
            let focus = &focus[0];
            if !node.focus_active
                || !node.focus_visible
                || focus.colors["outline-color"] != BLUE
                || px(&focus.css["outline-width"]) <= 0.
                || focus.css["outline-style"] == "none"
            {
                return Err(format!(
                    "FR-055 control {} needs a #0066cc keyboard-focus outline; active={}, focus-visible={}, outline={:?}",
                    node.label(),
                    node.focus_active,
                    node.focus_visible,
                    focus.colors["outline-color"]
                ));
            }
        }
    }
    Ok(())
}

pub fn shadows(audit: &Audit) -> Result<(), String> {
    for node in &audit.nodes {
        for paint in node.paints() {
            if paint.css["box-shadow"] != "none"
                || paint.css["text-shadow"] != "none"
                || paint.css["filter"].contains("drop-shadow(")
            {
                return Err(format!(
                    "FR-055 shadow on {}: box={}, text={}, filter={}",
                    node.label(),
                    paint.css["box-shadow"],
                    paint.css["text-shadow"],
                    paint.css["filter"]
                ));
            }
        }
    }
    Ok(())
}

pub fn fonts(audit: &Audit) -> Result<(), String> {
    if !audit.font_faces.is_empty() || audit.fonts.iter().any(|font| font.status == "loaded") {
        return Err(format!(
            "FR-055 web font: @font-face={:?}, loaded={:?}",
            audit.font_faces,
            audit
                .fonts
                .iter()
                .filter(|font| font.status == "loaded")
                .map(|font| &font.family)
                .collect::<Vec<_>>()
        ));
    }
    for node in &audit.nodes {
        for paint in node.paints() {
            for family in paint.css["font-family"]
                .split(',')
                .map(|value| value.trim().trim_matches(['\'', '"']).to_ascii_lowercase())
            {
                if !matches!(
                    family.as_str(),
                    "system-ui"
                        | "ui-sans-serif"
                        | "ui-serif"
                        | "ui-monospace"
                        | "ui-rounded"
                        | "sans-serif"
                        | "serif"
                        | "monospace"
                        | "cursive"
                        | "fantasy"
                        | "math"
                        | "emoji"
                        | "fangsong"
                        | "-apple-system"
                        | "blinkmacsystemfont"
                        | "segoe ui"
                        | "arial"
                        | "helvetica"
                        | "helvetica neue"
                        | "times new roman"
                        | "courier new"
                        | "menlo"
                        | "monaco"
                        | "consolas"
                        | "pingfang sc"
                        | "pingfang tc"
                        | "microsoft yahei"
                        | "simsun"
                        | "simhei"
                        | "heiti sc"
                        | "heiti tc"
                        | "noto sans cjk sc"
                ) {
                    return Err(format!(
                        "FR-055 non-system font on {}: {family}",
                        node.label()
                    ));
                }
            }
        }
    }
    Ok(())
}

pub fn status(audit: &Audit) -> Result<(), String> {
    let mut has_dot = false;
    for node in audit.nodes.iter().filter(|node| node.visible) {
        let colored = [&node.paint]
            .into_iter()
            .chain(node.focus.iter().map(|paint| &paint[0]))
            .flat_map(|paint| paint.rendered_colors())
            .any(|(property, color)| {
                matches!(
                    property,
                    "color"
                        | "background-color"
                        | "outline-color"
                        | "border-top-color"
                        | "border-right-color"
                        | "border-bottom-color"
                        | "border-left-color"
                ) && matches!(
                    *color,
                    [12, 163, 12, 255]
                        | [250, 178, 25, 255]
                        | [236, 131, 90, 255]
                        | [208, 59, 59, 255]
                )
            });
        if colored
            && (node.control()
                || node.rect.iter().any(|side| (side - 9.).abs() > 0.05)
                || !(node.tag == "circle" || rounded(node))
                || !node.text_beside)
        {
            return Err(format!(
                "FR-055 status color on {} needs a circular 9 px dot beside visible text; box={:?}, control={}, text-beside={}",
                node.label(),
                node.rect,
                node.control(),
                node.text_beside
            ));
        }
        has_dot |= colored;
    }
    if !has_dot {
        return Err(
            "FR-055 status audit requires at least one status-colored 9 px dot beside visible text"
                .into(),
        );
    }
    Ok(())
}

pub fn light_only(light: &Audit, dark: &Audit) -> Result<(), String> {
    if light.dark || !dark.dark {
        return Err(
            "FR-055 light-mode comparison did not exercise both browser preferences".into(),
        );
    }
    if light.nodes.len() != dark.nodes.len() {
        return Err("FR-055 color-scheme preference changed the element count".into());
    }
    for (a, b) in light.nodes.iter().zip(&dark.nodes) {
        let colors = |node: &Node| {
            node.paints()
                .map(|paint| paint.colors.clone())
                .collect::<Vec<_>>()
        };
        if a.tag != b.tag || a.visible != b.visible || colors(a) != colors(b) {
            return Err(format!(
                "FR-055 dark preference changed the recorded colors or visibility of {}",
                a.label()
            ));
        }
    }
    Ok(())
}
