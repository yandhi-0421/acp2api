use crate::*;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
pub(crate) struct Runtime {
    input: ProcessWrite,
    lines: mpsc::Receiver<(Value, Hold)>,
    tree: Box<dyn ProcessTree>,
    clock: Arc<dyn Clock>,
    next_id: u64,
    session: String,
}
impl Runtime {
    pub(crate) async fn start(
        ports: &Ports,
        data: &std::path::Path,
        cwd: PathBuf,
        mcp_url: &str,
    ) -> Result<Self, String> {
        std::fs::create_dir_all(data.join("runtime-home")).map_err(|e| e.to_string())?;
        let program = data.join("official-runtime").join(if cfg!(windows) {
            "agy_acp_server.exe"
        } else {
            "agy_acp_server.par"
        });
        std::fs::create_dir_all(&cwd).map_err(|e| e.to_string())?;
        let child = ports
            .launcher
            .launch(LaunchSpec {
                program,
                args: Vec::new(),
                env_set: vec![(
                    "GEMINI_HOME".into(),
                    data.join("runtime-home").into_os_string(),
                )],
                cwd: cwd.clone(),
                own_temp: None,
            })
            .map_err(|e| format!("runtime launch: {e:?}"))?;
        let (tx, lines) = mpsc::channel(128);
        let clock = ports.clock.clone();
        tokio::spawn(async move {
            let mut input = BufReader::new(child.stdout).lines();
            while let Ok(Some(line)) = input.next_line().await {
                let hold = clock.hold(HoldKind::RuntimeLine);
                if let Ok(value) = serde_json::from_str(&line)
                    && tx.send((value, hold)).await.is_err()
                {
                    break;
                }
            }
        });
        let clock = ports.clock.clone();
        tokio::spawn(async move {
            let mut input = BufReader::new(child.stderr).lines();
            while let Ok(Some(_line)) = input.next_line().await {
                let _hold = clock.hold(HoldKind::RuntimeLine);
            }
        });
        let mut runtime = Self {
            input: child.stdin,
            lines,
            tree: child.tree,
            clock: ports.clock.clone(),
            next_id: 0,
            session: String::new(),
        };
        runtime.call("initialize", json!({"protocolVersion":1,"clientInfo":{"name":"acp2api","title":"acp2api","version":ports.release.version()},"clientCapabilities":{}})).await?;
        let session = runtime.call("session/new", json!({"cwd": cwd,"mcpServers":[{"type":"http","name":"client-tools","url":mcp_url,"headers":[]}],"_meta":{"antigravity":{"enabledTools":[],"eagerMcpServers":true}}})).await?;
        runtime.session = session["sessionId"]
            .as_str()
            .ok_or("missing sessionId")?
            .to_owned();
        Ok(runtime)
    }
    async fn send(&mut self, method: &str, params: Value) -> Result<u64, String> {
        self.next_id += 1;
        let id = self.next_id;
        let mut bytes =
            serde_json::to_vec(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
                .map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        self.input
            .write_all(&bytes)
            .await
            .map_err(|e| e.to_string())?;
        self.input.flush().await.map_err(|e| e.to_string())?;
        Ok(id)
    }
    async fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.send(method, params).await?;
        while let Some((value, _hold)) = self.lines.recv().await {
            if value["id"] == id {
                return value
                    .get("result")
                    .cloned()
                    .ok_or_else(|| value["error"].to_string());
            }
        }
        Err("runtime stdout closed".into())
    }
    pub(crate) async fn prompt(
        &mut self,
        prompt: String,
        _model: &str,
    ) -> Result<(String, u64), String> {
        let id = self
            .send(
                "session/prompt",
                json!({"sessionId":self.session,"prompt":[{"type":"text","text":prompt}]}),
            )
            .await?;
        let mut text = String::new();
        let mut used = 0;
        while let Some((value, _hold)) = self.lines.recv().await {
            if value["id"] == id {
                if value.get("error").is_some() {
                    return Err(value["error"].to_string());
                }
                return Ok((text, used));
            }
            let update = &value["params"]["update"];
            if update["sessionUpdate"] == "agent_message_chunk"
                && let Some(chunk) = update["content"]["text"].as_str()
            {
                text.push_str(chunk);
            }
            if update["sessionUpdate"] == "usage_update" {
                used = update["used"].as_u64().unwrap_or(used);
            }
        }
        Err("runtime stdout closed".into())
    }
    pub(crate) async fn stop(mut self) {
        let _ = self.input.shutdown().await;
        drop(self.input);
        self.tree.stop(self.clock).await;
    }
}
