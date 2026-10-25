use super::{Parsed, fields};
use crate::{
    conversation::{
        Conversation, Item, Part, ReplyForm, Role, SystemPart, ToolChoice, TurnRequest,
    },
    messages::{Message, Refusal},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::Value;

#[derive(Clone, Copy)]
pub(super) enum Api {
    Messages,
    Chat,
    Responses,
}
pub(super) fn parse(api: Api, body: &[u8]) -> Result<Parsed, Refusal> {
    let value = fields::parse_json(body)?;
    if matches!(api, Api::Messages)
        && let Some(kind) = value["tools"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|tool| tool["type"].as_str())
            .find(|kind| kind.starts_with("advisor_"))
    {
        return Err(Message::AdvisorTool(kind.into()).into());
    }
    fields::check(api, &value)?;
    let model = required(&value, "model")?.to_owned();
    let mut tool_text = String::new();
    let client_tools = tools(api, &value["tools"], &mut tool_text)?;
    let input = &value[if matches!(api, Api::Responses) {
        "input"
    } else {
        "messages"
    }];
    let mut items = Vec::new();
    let mut system = Vec::new();
    if matches!(api, Api::Messages) && !value["system"].is_null() {
        let mut texts = system_texts(api, &value["system"])?;
        if value["system"].is_array()
            && texts
                .first()
                .is_some_and(|text| text.starts_with("x-anthropic-billing-header:"))
        {
            texts.remove(0);
        }
        system.extend(system_part(Role::System, None, &texts));
    }
    if matches!(api, Api::Responses) && !value["instructions"].is_null() {
        system.push(SystemPart {
            role: Role::System,
            name: None,
            text: required(&value, "instructions")?.into(),
        });
    }
    if let (Api::Responses, Some(text)) = (api, input.as_str()) {
        items.push(message(Role::User, None, vec![Part::Text(text.into())]));
    } else {
        let mut at_start = true;
        for item in input
            .as_array()
            .ok_or_else(|| Refusal::invalid("message array required"))?
        {
            if matches!(api, Api::Responses) && item["type"] == "reasoning" {
                continue;
            }
            if matches!(api, Api::Chat) && item["role"] == "tool" {
                at_start = false;
                items.push(Item::ToolResult {
                    id: required(item, "tool_call_id")?.into(),
                    parts: result_parts(api, &item["content"], &client_tools)?,
                    error: false,
                });
                continue;
            }
            if matches!(api, Api::Responses)
                && let Some(tool) = responses_item(item, &client_tools)?
            {
                at_start = false;
                items.push(tool);
                continue;
            }
            let role = match required(item, "role")? {
                "user" => Role::User,
                "assistant" => Role::Assistant,
                "system" => Role::System,
                "developer" => Role::Developer,
                _ => return Err(Refusal::invalid("unsupported message role")),
            };
            if !matches!(api, Api::Messages)
                && at_start
                && matches!(role, Role::System | Role::Developer)
            {
                let texts = system_texts(api, &item["content"])?;
                system.extend(system_part(role, item["name"].as_str(), &texts));
            } else {
                at_start = false;
                items.extend(match api {
                    Api::Messages => messages_blocks(item, role, &client_tools)?,
                    Api::Chat => chat_assistant(item, role, &client_tools)?,
                    Api::Responses => vec![message(
                        role,
                        item["name"].as_str(),
                        parts(api, &item["content"], &client_tools)?,
                    )],
                });
            }
        }
    }
    let last = items
        .pop()
        .ok_or_else(|| Refusal::invalid("one user message required"))?;
    if matches!(
        last,
        Item::Message {
            role: Role::Assistant,
            ..
        } | Item::ToolCall { .. }
    ) {
        return Err(Message::Prefill.into());
    }
    let tool_choice = if value["tool_choice"] == "none" || value["tool_choice"]["type"] == "none" {
        ToolChoice::Off
    } else {
        ToolChoice::Auto
    };
    let conversation = Conversation {
        system,
        earlier: items,
        last,
    };
    let turn = TurnRequest {
        stop: match api {
            Api::Messages => value.get("stop_sequences"),
            Api::Chat => value.get("stop"),
            Api::Responses => None,
        }
        .map(|stop| {
            if let Some(text) = stop.as_str() {
                Ok(vec![text.to_owned()])
            } else {
                serde_json::from_value::<Vec<String>>(stop.clone())
                    .map_err(|_| Refusal::invalid("acp2api: stop sequences must be strings."))
            }
        })
        .transpose()?
        .unwrap_or_default(),
        input_tokens: crate::conversation::input_tokens(&conversation, &tool_text),
        conversation,
        model: model.clone(),
        effort: effort(api, &value),
        tools: client_tools,
        tool_choice,
    };
    Ok(Parsed {
        form: ReplyForm {
            stream: value["stream"].as_bool().unwrap_or(false),
            include_usage: value["stream_options"]["include_usage"] == true,
            input_tokens: turn.input_tokens,
            model_name: model,
            tools: value["tools"].as_array().cloned().unwrap_or_default(),
            tool_choice,
        },
        turn,
        unknown_fields: value
            .as_object()
            .into_iter()
            .flat_map(|o| o.keys())
            .filter(|name| !known_field(api, name))
            .cloned()
            .collect(),
    })
}
fn effort(api: Api, value: &Value) -> crate::conversation::EffortAsk {
    use crate::conversation::{EffortAsk, ThinkingLevel};
    let field = match api {
        Api::Chat => value.get("reasoning_effort"),
        Api::Responses => value["reasoning"].get("effort"),
        Api::Messages if value["output_config"]["effort"].is_null() => {
            return if value["thinking"]["type"] == "disabled" {
                EffortAsk::Level(ThinkingLevel::Low)
            } else {
                EffortAsk::Absent
            };
        }
        Api::Messages => value["output_config"].get("effort"),
    };
    match field {
        Some(Value::String(value)) => match value.as_str() {
            "none" | "minimal" | "low" => EffortAsk::Level(ThinkingLevel::Low),
            "medium" => EffortAsk::Level(ThinkingLevel::Medium),
            "high" | "xhigh" | "max" | "ultra" => EffortAsk::Level(ThinkingLevel::High),
            _ => EffortAsk::Unknown(if value.len() <= 256 {
                value.clone().into()
            } else {
                Value::Null
            }),
        },
        Some(Value::Null) => EffortAsk::Absent,
        Some(Value::Bool(_)) => EffortAsk::Unknown("boolean".into()),
        Some(Value::Number(_)) => EffortAsk::Unknown("number".into()),
        Some(Value::Array(_)) => EffortAsk::Unknown("array".into()),
        Some(Value::Object(_)) => EffortAsk::Unknown("object".into()),
        None => EffortAsk::Absent,
    }
}
fn message(role: Role, name: Option<&str>, parts: Vec<Part>) -> Item {
    Item::Message {
        role,
        name: name.map(str::to_owned),
        parts,
    }
}
fn messages_blocks(
    item: &Value,
    role: Role,
    tools: &[crate::conversation::ClientTool],
) -> Result<Vec<Item>, Refusal> {
    let Some(blocks) = item["content"].as_array().filter(|blocks| {
        blocks
            .iter()
            .any(|block| matches!(block["type"].as_str(), Some("tool_use" | "tool_result")))
    }) else {
        return Ok(vec![message(
            role,
            item["name"].as_str(),
            parts(Api::Messages, &item["content"], tools)?,
        )]);
    };
    let mut items = Vec::new();
    let mut pending = Vec::new();
    for block in blocks {
        let tool = match block["type"].as_str() {
            Some("tool_use") => {
                if !block["input"].is_object() {
                    return Err(Refusal::invalid("tool input must be an object"));
                }
                Item::ToolCall {
                    id: required(block, "id")?.into(),
                    name: required(block, "name")?.into(),
                    arguments: block["input"].to_string(),
                }
            }
            Some("tool_result") => Item::ToolResult {
                id: required(block, "tool_use_id")?.into(),
                parts: result_parts(Api::Messages, &block["content"], tools)?,
                error: block["is_error"].as_bool().unwrap_or(false),
            },
            _ => {
                push_part(Api::Messages, block, tools, &mut pending)?;
                continue;
            }
        };
        if !pending.is_empty() {
            items.push(message(
                role,
                item["name"].as_str(),
                std::mem::take(&mut pending),
            ));
        }
        items.push(tool);
    }
    if !pending.is_empty() {
        items.push(message(role, item["name"].as_str(), pending));
    }
    Ok(items)
}
fn chat_assistant(
    item: &Value,
    role: Role,
    tools: &[crate::conversation::ClientTool],
) -> Result<Vec<Item>, Refusal> {
    let mut items = Vec::new();
    let calls = item["tool_calls"].as_array();
    let has_content = !item["content"].is_null() && item["content"] != "";
    if calls.is_none() || has_content {
        items.push(message(
            role,
            item["name"].as_str(),
            parts(Api::Chat, &item["content"], tools)?,
        ));
    }
    if let Some(calls) = calls {
        for call in calls {
            items.push(Item::ToolCall {
                id: required(call, "id")?.into(),
                name: required(&call["function"], "name")?.into(),
                arguments: required(&call["function"], "arguments")?.into(),
            });
        }
    }
    Ok(items)
}
fn responses_item(
    item: &Value,
    tools: &[crate::conversation::ClientTool],
) -> Result<Option<Item>, Refusal> {
    Ok(match item["type"].as_str() {
        Some(kind @ ("function_call" | "custom_tool_call")) => Some(Item::ToolCall {
            id: required(item, "call_id")?.into(),
            name: required(item, "name")?.into(),
            arguments: required(
                item,
                if kind == "function_call" {
                    "arguments"
                } else {
                    "input"
                },
            )?
            .into(),
        }),
        Some("function_call_output" | "custom_tool_call_output") => Some(Item::ToolResult {
            id: required(item, "call_id")?.into(),
            parts: result_parts(Api::Responses, &item["output"], tools)?,
            error: false,
        }),
        None | Some("message") => None,
        Some(kind) => return Err(Message::Content(kind.into()).into()),
    })
}
fn known_field(api: Api, name: &str) -> bool {
    matches!(
        name,
        "model"
            | "stream"
            | "stream_options"
            | "tools"
            | "tool_choice"
            | "n"
            | "thinking"
            | "output_config"
            | "reasoning"
            | "reasoning_effort"
            | "temperature"
            | "top_p"
            | "top_k"
            | "presence_penalty"
            | "frequency_penalty"
            | "seed"
            | "logit_bias"
            | "max_tokens"
            | "max_completion_tokens"
            | "max_output_tokens"
            | "cache_control"
            | "prompt_cache_key"
            | "service_tier"
            | "store"
            | "truncation"
            | "prediction"
            | "metadata"
            | "user"
            | "safety_identifier"
            | "client_metadata"
            | "context_management"
            | "parallel_tool_calls"
            | "max_tool_calls"
            | "verbosity"
            | "text"
            | "response_format"
            | "include"
            | "stop"
            | "stop_sequences"
            | "logprobs"
            | "top_logprobs"
            | "web_search_options"
            | "audio"
            | "modalities"
            | "background"
            | "previous_response_id"
            | "conversation"
            | "citations"
    ) || match api {
        Api::Messages => matches!(name, "messages" | "system"),
        Api::Chat => name == "messages",
        Api::Responses => matches!(name, "input" | "instructions"),
    }
}
fn system_texts(api: Api, value: &Value) -> Result<Vec<&str>, Refusal> {
    if let Some(text) = value.as_str() {
        return Ok(vec![text]);
    }
    value
        .as_array()
        .ok_or_else(|| Refusal::invalid("system prompt must be text or text blocks"))?
        .iter()
        .map(|part| {
            let kind = required(part, "type")?;
            if !matches!(
                (api, kind),
                (Api::Messages | Api::Chat, "text") | (Api::Responses, "input_text")
            ) {
                return Err(Message::Content(kind.into()).into());
            }
            required(part, "text")
        })
        .collect()
}
fn system_part(role: Role, name: Option<&str>, texts: &[&str]) -> Option<SystemPart> {
    (!texts.is_empty()).then(|| SystemPart {
        role,
        name: name.map(str::to_owned),
        text: texts.join("\n\n"),
    })
}
fn required<'a>(v: &'a Value, key: &str) -> Result<&'a str, Refusal> {
    v[key]
        .as_str()
        .ok_or_else(|| Refusal::invalid(format!("string field {key} required")))
}
fn parts(
    api: Api,
    value: &Value,
    tools: &[crate::conversation::ClientTool],
) -> Result<Vec<Part>, Refusal> {
    if let Some(text) = value.as_str() {
        return Ok(vec![Part::Text(text.into())]);
    }
    let mut parts = Vec::new();
    for block in value
        .as_array()
        .ok_or_else(|| Refusal::invalid("content must be text or blocks"))?
    {
        push_part(api, block, tools, &mut parts)?;
    }
    Ok(parts)
}
fn push_part(
    api: Api,
    block: &Value,
    tools: &[crate::conversation::ClientTool],
    parts: &mut Vec<Part>,
) -> Result<(), Refusal> {
    if matches!(api, Api::Messages)
        && matches!(
            block["type"].as_str(),
            Some("thinking" | "redacted_thinking")
        )
    {
        return Ok(());
    }
    if matches!(api, Api::Messages) && block["type"] == "tool_addition" {
        let tool = &block["tool"];
        if tool["type"] != "tool_reference"
            || !tool["name"]
                .as_str()
                .is_some_and(|name| tools.iter().any(|tool| tool.name == name))
        {
            return Err(Message::Content("tool_addition".into()).into());
        }
    } else {
        parts.push(part(api, block)?);
    }
    Ok(())
}
fn result_parts(
    api: Api,
    value: &Value,
    tools: &[crate::conversation::ClientTool],
) -> Result<Vec<Part>, Refusal> {
    if matches!(api, Api::Messages) && value.is_null() {
        return Ok(Vec::new());
    }
    if value.is_string() {
        return parts(api, value, tools);
    }
    let mut parts = Vec::new();
    for block in value
        .as_array()
        .ok_or_else(|| Refusal::invalid("tool result must be text or blocks"))?
    {
        match push_part(api, block, tools, &mut parts) {
            Err(Refusal {
                message: Message::Pdf(_),
            }) => parts.push(Part::PdfNote),
            result => result?,
        }
    }
    Ok(parts)
}
fn part(api: Api, v: &Value) -> Result<Part, Refusal> {
    let kind = required(v, "type")?;
    match (api, kind) {
        (Api::Messages | Api::Chat, "text") | (Api::Responses, "input_text" | "output_text") => {
            Ok(Part::Text(required(v, "text")?.into()))
        }
        (Api::Messages, "image") => {
            let source = &v["source"];
            reject_id(source)?;
            if source["type"] == "url" || source["url"].is_string() {
                return Err(Message::Url(kind.into()).into());
            }
            if source["type"] != "base64" {
                return Err(Message::Content(kind.into()).into());
            }
            image(required(source, "media_type")?, required(source, "data")?)
        }
        (Api::Chat, "image_url") | (Api::Responses, "input_image") => {
            reject_id(v)?;
            let url = if matches!(api, Api::Chat) {
                required(&v["image_url"], "url")?
            } else {
                required(v, "image_url")?
            };
            let (media, data) = data_url(url, kind)?;
            image(media, data)
        }
        (Api::Messages, "document") | (Api::Chat, "file") | (Api::Responses, "input_file") => {
            document(api, v, kind)
        }
        _ => Err(Message::Content(kind.into()).into()),
    }
}
fn reject_id(value: &Value) -> Result<(), Refusal> {
    if !value["file_id"].is_null() {
        return Err(Refusal::field("file_id", None));
    }
    Ok(())
}
fn remote(value: &str) -> bool {
    value
        .get(..7)
        .is_some_and(|v| v.eq_ignore_ascii_case("http://"))
        || value
            .get(..8)
            .is_some_and(|v| v.eq_ignore_ascii_case("https://"))
}
fn data_url<'a>(url: &'a str, kind: &str) -> Result<(&'a str, &'a str), Refusal> {
    if remote(url) {
        return Err(Message::Url(kind.into()).into());
    }
    let (media, data) = url
        .strip_prefix("data:")
        .and_then(|v| v.split_once(";base64,"))
        .ok_or_else(|| Refusal::invalid("base64 data URL required"))?;
    Ok((media, data))
}
fn bytes(data: &str) -> Result<Vec<u8>, Refusal> {
    STANDARD
        .decode(data)
        .map_err(|_| Refusal::invalid("invalid base64 content"))
}
fn image(media: &str, data: &str) -> Result<Part, Refusal> {
    bytes(data)?;
    Ok(Part::Image {
        media_type: media.into(),
        base64: data.into(),
    })
}
fn document(api: Api, v: &Value, kind: &str) -> Result<Part, Refusal> {
    let file = if matches!(api, Api::Chat) {
        &v["file"]
    } else {
        v
    };
    reject_id(file)?;
    let source = &file["source"];
    reject_id(source)?;
    if file["file_url"].is_string() || source["type"] == "url" || source["url"].is_string() {
        return Err(Message::Url(kind.into()).into());
    }
    let title = v["title"]
        .as_str()
        .or_else(|| file["title"].as_str())
        .or_else(|| file["filename"].as_str())
        .map(str::to_owned);
    let text = if matches!(api, Api::Messages) {
        match source["type"].as_str() {
            Some("text") => required(source, "data")?.to_owned(),
            Some("content") => {
                let content = &source["content"];
                if let Some(t) = content.as_str() {
                    t.to_owned()
                } else {
                    let mut lines = Vec::new();
                    for part in content
                        .as_array()
                        .ok_or_else(|| Refusal::invalid("document content required"))?
                    {
                        if part["type"] != "text" {
                            return Err(Message::Content(
                                part["type"].as_str().unwrap_or("unknown").into(),
                            )
                            .into());
                        }
                        lines.push(required(part, "text")?);
                    }
                    lines.join("\n")
                }
            }
            Some("base64") => {
                let media = required(source, "media_type")?;
                validate_media(media, kind)?;
                decode_text(required(source, "data")?)?
            }
            _ => return Err(Message::Content(kind.into()).into()),
        }
    } else {
        let (media, data) = data_url(required(file, "file_data")?, kind)?;
        validate_media(media, kind)?;
        decode_text(data)?
    };
    Ok(Part::Document { title, text })
}
fn decode_text(data: &str) -> Result<String, Refusal> {
    String::from_utf8(bytes(data)?).map_err(|_| Refusal::invalid("text document must be UTF-8"))
}
fn validate_media(media: &str, kind: &str) -> Result<(), Refusal> {
    let essence = media
        .split_once(';')
        .map_or(media, |(essence, _)| essence)
        .trim();
    match essence.to_ascii_lowercase().as_str() {
        "application/pdf" => Err(Message::Pdf(kind.into()).into()),
        "text/plain" => Ok(()),
        _ => Err(Message::Media(media.into()).into()),
    }
}

fn tools(
    api: Api,
    value: &Value,
    text: &mut String,
) -> Result<Vec<crate::conversation::ClientTool>, Refusal> {
    if value.is_null() {
        return Ok(Vec::new());
    }
    let mut normalized = Vec::new();
    for tool in value
        .as_array()
        .ok_or_else(|| Refusal::invalid("tools must be an array"))?
    {
        if matches!(api, Api::Responses) && tool["type"] == "namespace" {
            let namespace = required(tool, "name")?;
            let namespace_description = tool["description"].as_str().unwrap_or("");
            for member in tool["tools"]
                .as_array()
                .ok_or_else(|| Refusal::invalid("namespace tools must be an array"))?
            {
                let mut member_tool = client_tool(api, member, Some(namespace), text)?;
                member_tool.description = format!(
                    "Namespace {namespace}:\n{namespace_description}\n\n{}",
                    member_tool.description
                );
                normalized.push(member_tool);
            }
        } else {
            normalized.push(client_tool(api, tool, None, text)?);
        }
    }
    Ok(normalized)
}
fn client_tool(
    api: Api,
    tool: &Value,
    namespace: Option<&str>,
    text: &mut String,
) -> Result<crate::conversation::ClientTool, Refusal> {
    use crate::conversation::{ClientTool, ToolKind};
    let kind = tool["type"].as_str().unwrap_or("function");
    if matches!(api, Api::Messages) && server_tool(kind) {
        return Err(Message::ServerTool {
            kind: kind.into(),
            name: required(
                tool,
                if kind == "mcp_toolset" {
                    "mcp_server_name"
                } else {
                    "name"
                },
            )?
            .into(),
        }
        .into());
    }
    if matches!(api, Api::Responses) && kind == "web_search" {
        return Err(Message::CodexWebSearch.into());
    }
    if kind == "tool_search" {
        return Err(Message::ToolSearch.into());
    }
    let kind = match kind {
        "function" => ToolKind::Function,
        "custom" if matches!(api, Api::Messages) => ToolKind::Function,
        "custom" if matches!(api, Api::Responses) => ToolKind::Custom,
        other => {
            return Err(Message::ToolType {
                kind: other.into(),
                name: tool["name"]
                    .as_str()
                    .or_else(|| tool["function"]["name"].as_str())
                    .unwrap_or("")
                    .into(),
            }
            .into());
        }
    };
    let function = if matches!(api, Api::Chat) {
        &tool["function"]
    } else {
        tool
    };
    let name = required(function, "name")?;
    if matches!(name, "list_resources" | "read_resource") {
        return Err(Message::ToolName(name.into()).into());
    }
    let mut description = function["description"].as_str().unwrap_or("").to_owned();
    text.push_str(name);
    text.push_str(&description);
    let schema = match kind {
        ToolKind::Custom => {
            match tool["format"]["type"].as_str() {
                Some("grammar") => {
                    let definition = required(&tool["format"], "definition")?;
                    let syntax = required(&tool["format"], "syntax")?;
                    text.push_str(definition);
                    description.push_str(&format!("\n\nGrammar ({syntax}):\n{definition}"));
                }
                None | Some("text") => {}
                Some(_) => return Err(Refusal::invalid("unsupported custom tool format")),
            }
            serde_json::json!({"type":"object","properties":{"input":{"type":"string"}},"required":["input"],"additionalProperties":false})
        }
        ToolKind::Function => {
            let schema = &function[if matches!(api, Api::Messages) {
                "input_schema"
            } else {
                "parameters"
            }];
            match schema {
                Value::Null if !matches!(api, Api::Messages) => {
                    serde_json::json!({"type":"object","properties":{}})
                }
                Value::Object(_) => {
                    text.push_str(&schema.to_string());
                    schema.clone()
                }
                _ => return Err(Refusal::invalid("tool schema must be an object")),
            }
        }
    };
    Ok(ClientTool {
        name: name.into(),
        description,
        schema,
        kind,
        namespace: namespace.map(str::to_owned),
    })
}
fn server_tool(kind: &str) -> bool {
    matches!(
        kind,
        "tool_search_tool_regex" | "tool_search_tool_bm25" | "mcp_toolset"
    ) || [
        "web_search_",
        "web_fetch_",
        "code_execution_",
        "tool_search_tool_regex_",
        "tool_search_tool_bm25_",
    ]
    .iter()
    .any(|prefix| {
        kind.strip_prefix(prefix)
            .is_some_and(|date| date.len() == 8 && date.bytes().all(|b| b.is_ascii_digit()))
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn messages_tool_results_allow_omitted_or_null_content() {
        for content in ["", r#", "content":null"#] {
            let body = format!(
                r#"{{"model":"m","messages":[{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"empty"{content}}}]}}]}}"#
            );
            let actual = parse(Api::Messages, body.as_bytes())
                .map(|parsed| match parsed.turn.conversation.last {
                    Item::ToolResult { id, parts, error } => {
                        (id, parts.len(), error, parsed.turn.input_tokens)
                    }
                    _ => panic!("expected a tool result"),
                })
                .map_err(|refusal| refusal.message.to_string());
            assert_eq!(actual, Ok(("empty".into(), 0, false, 0)));
        }
    }
    #[test]
    fn an_ignored_tool_addition_keeps_an_otherwise_empty_user_message() {
        let body = br#"{"model":"m","tools":[{"name":"read","input_schema":{}}],"messages":[
            {"role":"user","content":[{"type":"tool_addition","tool":{"type":"tool_reference","name":"read"}}]}
        ]}"#;
        let actual = rendered(Api::Messages, body).map(|(tokens, blocks, _)| (tokens, blocks));
        assert_eq!(
            actual,
            Ok((2, 0)),
            "FR-024 ignored blocks leave the user message intact"
        );
    }
    #[test]
    fn a_final_assistant_tool_call_is_still_an_unsupported_prefill() {
        for (api, body) in [
            (
                Api::Chat,
                r#"{"model":"m","messages":[{"role":"assistant","content":null,"tool_calls":[{"id":"c","function":{"name":"read","arguments":"{}"}}]}]}"#,
            ),
            (
                Api::Messages,
                r#"{"model":"m","messages":[{"role":"assistant","content":[{"type":"tool_use","id":"c","name":"read","input":{}}]}]}"#,
            ),
            (
                Api::Responses,
                r#"{"model":"m","input":[{"type":"function_call","call_id":"c","name":"read","arguments":"{}"}]}"#,
            ),
        ] {
            let actual = parse(api, body.as_bytes())
                .err()
                .map(|refusal| refusal.message.to_string());
            assert_eq!(
                actual.as_deref(),
                Some("acp2api: a last message from the assistant (a prefill) is not supported.")
            );
        }
    }
    #[test]
    fn responses_custom_tool_history_keeps_assistant_text_and_raw_input() {
        let body = r#"{"model":"m","input":[
            {"type":"message","role":"assistant","content":[{"type":"output_text","text":"Patch."}]},
            {"type":"custom_tool_call","call_id":"call_1","name":"patch","input":"*** Begin Patch"},
            {"type":"custom_tool_call_output","call_id":"call_1","output":"DONE"}
        ]}"#;
        let actual =
            rendered(Api::Responses, body.as_bytes()).map(|(tokens, _, text)| (tokens, text));
        assert_eq!(actual, Ok((8, concat!(
            "The text below is the earlier part of this conversation. Continue the conversation as the assistant from its last item.\n\n",
            "[assistant]\nPatch.\nYou called the tool patch with the arguments *** Begin Patch.\n",
            "Result of your tool call patch *** Begin Patch:\nDONE"
        ).into())));
    }
    #[test]
    fn responses_tool_results_keep_content_order_and_count_only_client_text() {
        let body = r#"{"model":"m","input":[
            {"type":"function_call","call_id":"call_1","name":"read","arguments":"{ \"a\": \"é\", \"b\": 1 }"},
            {"type":"function_call_output","call_id":"call_1","output":[
                {"type":"input_text","text":"DATA"},
                {"type":"input_file","file_data":"data:application/pdf;base64,JVBERi0="},
                {"type":"input_image","image_url":"data:image/png;base64,aQ=="},
                {"type":"input_text","text":"NEXT"}
            ]}
        ]}"#;
        let actual =
            rendered(Api::Responses, body.as_bytes()).map(|(tokens, _, text)| (tokens, text));
        assert_eq!(actual, Ok((9, concat!(
            "The text below is the earlier part of this conversation. Continue the conversation as the assistant from its last item.\n\n",
            "You called the tool read with the arguments { \"a\": \"é\", \"b\": 1 }.\n",
            "Result of your tool call read { \"a\": \"é\", \"b\": 1 }:\nDATA\n",
            "[A PDF document was here. acp2api cannot give PDF documents to the model.]\n",
            "[image:image/png:aQ==]NEXT"
        ).into())), "FR-023 content order and FR-025 client text count");
    }
    fn rendered(api: Api, body: &[u8]) -> Result<(u64, usize, String), String> {
        parse(api, body)
            .map(|parsed| {
                let conversation = &parsed.turn.conversation;
                let items = conversation.items();
                let input = if conversation.earlier.is_empty() {
                    crate::conversation::Prompt::New {
                        message: &conversation.last,
                    }
                } else {
                    crate::conversation::Prompt::Rebuild {
                        conversation_items: &items,
                    }
                };
                let (_, blocks) = crate::conversation::prompt(input, &items);
                let count = blocks.len();
                let text = blocks
                    .into_iter()
                    .map(|block| match block {
                        crate::conversation::PromptBlock::Text(text) => text,
                        crate::conversation::PromptBlock::Image { media_type, base64 } => {
                            format!("[image:{media_type}:{base64}]")
                        }
                    })
                    .collect();
                (parsed.turn.input_tokens, count, text)
            })
            .map_err(|refusal| refusal.message.to_string())
    }
    fn input_estimate(body: &str) -> u64 {
        let parsed = parse(Api::Responses, body.as_bytes())
            .unwrap_or_else(|refusal| panic!("refused: {}", refusal.message));
        parsed.turn.input_tokens
    }
    #[test]
    fn input_estimate_counts_the_declared_custom_tool_grammar() {
        let body = r#"{"model":"m","input":"","tools":[{"type":"custom","name":"edit","description":"Edit","format":{"type":"grammar","syntax":"lark","definition":"start: \"é\""}}]}"#;
        assert_eq!(input_estimate(body), 6);
    }
    #[test]
    fn input_estimate_counts_the_declared_namespace_member() {
        let body = r#"{"model":"m","input":"","tools":[{"type":"namespace","name":"files","description":"Files","tools":[{"type":"function","name":"read","description":"Read","parameters":{}}]}]}"#;
        assert_eq!(input_estimate(body), 3);
    }
    #[test]
    fn input_estimate_counts_no_schema_when_parameters_are_absent() {
        let body = r#"{"model":"m","input":"","tools":[{"type":"function","name":"now","description":"Time"}]}"#;
        assert_eq!(input_estimate(body), 2);
    }
    #[test]
    fn input_estimate_counts_a_compact_schema_with_non_alphabetical_keys() {
        let body = r#"{
            "model": "m", "input": "",
            "tools": [{"type": "function", "name": "read",
                "parameters": { "z": "é", "a": 1 }}]
        }"#;
        assert_eq!(input_estimate(body), 6);
    }
    fn text_of(api: Api, v: Value) -> String {
        match document(api, &v, "document") {
            Ok(Part::Document { text, .. }) => text,
            Ok(_) => panic!("not a document"),
            Err(refusal) => panic!("refused: {}", refusal.message),
        }
    }
    #[test]
    fn content_document_puts_a_line_break_between_text_blocks() {
        let doc = json!({"type":"document","source":{"type":"content","content":[
            {"type":"text","text":"Line one."},{"type":"text","text":"Line two."}
        ]}});
        assert_eq!(text_of(Api::Messages, doc), "Line one.\nLine two.");
    }
    #[test]
    fn media_type_compares_its_lowercase_essence() {
        let chat = json!({"type":"file","file":{"file_data":"data:text/plain;charset=utf-8;base64,TGluZSBvbmUu"}});
        assert_eq!(text_of(Api::Chat, chat), "Line one.");
        let messages = json!({"type":"document","source":{"type":"base64","media_type":"Text/Plain; charset=utf-8","data":"TGluZSBvbmUu"}});
        assert_eq!(text_of(Api::Messages, messages), "Line one.");
        let pdf = json!({"type":"input_file","file_data":"data:Application/PDF;name=a.pdf;base64,TGluZSBvbmUu"});
        assert!(matches!(
            document(Api::Responses, &pdf, "input_file").map_err(|r| r.message),
            Err(Message::Pdf(_))
        ));
    }
    #[test]
    fn a_function_tool_without_parameters_gets_an_empty_object_schema() {
        let schema = |api, tools_value| match tools(api, &tools_value, &mut String::new()) {
            Ok(tools) => tools[0].schema.to_string(),
            Err(refusal) => panic!("refused: {}", refusal.message),
        };
        let empty = r#"{"type":"object","properties":{}}"#;
        let chat = json!([{"type":"function","function":{"name":"now"}}]);
        assert_eq!(schema(Api::Chat, chat), empty);
        let responses = json!([{"type":"function","name":"now","parameters":null}]);
        assert_eq!(schema(Api::Responses, responses), empty);
        assert!(tools(Api::Messages, &json!([{"name":"now"}]), &mut String::new()).is_err());
    }
    #[test]
    fn an_advisor_tool_is_an_unknown_tool_type_outside_the_messages_api() {
        let message = |api, tools: Value| {
            let body = json!({"model":"m","tools":tools,"input":"Hi","messages":[]});
            match parse(api, body.to_string().as_bytes()) {
                Err(refusal) => refusal.message,
                Ok(_) => panic!("accepted"),
            }
        };
        let advisor = json!([{"type":"advisor_20260301","name":"advisor"}]);
        assert!(matches!(
            message(Api::Messages, advisor.clone()),
            Message::AdvisorTool(kind) if kind == "advisor_20260301"
        ));
        for api in [Api::Chat, Api::Responses] {
            assert!(matches!(
                message(api, advisor.clone()),
                Message::ToolType { kind, name } if kind == "advisor_20260301" && name == "advisor"
            ));
        }
    }
}
