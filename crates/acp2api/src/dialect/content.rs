use super::{Parsed, parsed};
use crate::{
    conversation::{Conversation, Item, Part, Role, SystemPart},
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
    let value: Value = serde_json::from_slice(body).map_err(|e| Refusal::invalid(e.to_string()))?;
    if let Some(tools) = value["tools"].as_array() {
        for tool in tools {
            if let Some(kind) = tool["type"]
                .as_str()
                .filter(|kind| kind.starts_with("advisor_"))
            {
                return Err(Message::AdvisorTool(kind.into()).into());
            }
        }
    }
    let model = required(&value, "model")?.to_owned();
    let client_tools = tools(api, &value["tools"])?;
    let input = &value[if matches!(api, Api::Responses) {
        "input"
    } else {
        "messages"
    }];
    let mut items = Vec::new();
    let mut system = Vec::new();
    if matches!(api, Api::Messages) && !value["system"].is_null() {
        system = system_parts(Role::System, None, &value["system"], "text")?;
        if value["system"].is_array()
            && system
                .first()
                .is_some_and(|part| part.text.starts_with("x-anthropic-billing-header:"))
        {
            system.remove(0);
        }
    }
    if matches!(api, Api::Responses) && !value["instructions"].is_null() {
        system.push(SystemPart {
            role: Role::System,
            name: None,
            text: required(&value, "instructions")?.into(),
        });
    }
    if matches!(api, Api::Responses) && input.is_string() {
        items.push(Item::Message {
            role: Role::User,
            parts: vec![Part::Text(input.as_str().unwrap().into())],
        });
    } else {
        let mut at_start = true;
        for item in input
            .as_array()
            .ok_or_else(|| Refusal::invalid("message array required"))?
        {
            let kind = item["type"].as_str();
            if matches!(api, Api::Responses) && kind.is_some_and(|k| k != "message") {
                return Err(Message::Content(kind.unwrap().into()).into());
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
                system.extend(system_parts(
                    role,
                    item["name"].as_str(),
                    &item["content"],
                    if matches!(api, Api::Responses) {
                        "input_text"
                    } else {
                        "text"
                    },
                )?);
            } else {
                at_start = false;
                let parts = parts(api, &item["content"], &client_tools)?;
                items.push(Item::Message { role, parts });
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
        }
    ) {
        return Err(Message::Prefill.into());
    }
    let mut parsed = parsed(
        model,
        Conversation {
            system,
            earlier: items,
            last,
        },
        value["stream"].as_bool().unwrap_or(false),
    );
    parsed.turn.tools = client_tools;
    parsed.turn.tool_choice =
        if value["tool_choice"] == "none" || value["tool_choice"]["type"] == "none" {
            crate::conversation::ToolChoice::Off
        } else {
            crate::conversation::ToolChoice::Auto
        };
    parsed.unknown_fields = value
        .as_object()
        .into_iter()
        .flat_map(|o| o.keys())
        .filter(|name| !known_field(api, name))
        .cloned()
        .collect();
    Ok(parsed)
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
fn system_parts(
    role: Role,
    name: Option<&str>,
    value: &Value,
    text_kind: &str,
) -> Result<Vec<SystemPart>, Refusal> {
    let make = |text: &str| SystemPart {
        role,
        name: name.map(str::to_owned),
        text: text.into(),
    };
    if let Some(text) = value.as_str() {
        return Ok(vec![make(text)]);
    }
    value
        .as_array()
        .ok_or_else(|| Refusal::invalid("system prompt must be text or text blocks"))?
        .iter()
        .map(|part| {
            let kind = required(part, "type")?;
            if kind != text_kind {
                return Err(Message::Content(kind.into()).into());
            }
            Ok(make(required(part, "text")?))
        })
        .collect()
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
    }
    Ok(parts)
}
fn part(api: Api, v: &Value) -> Result<Part, Refusal> {
    let kind = required(v, "type")?;
    match (api, kind) {
        (Api::Messages | Api::Chat, "text") | (Api::Responses, "input_text") => {
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
        return Err(Message::Field("file_id".into()).into());
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
    let (media, text) = if matches!(api, Api::Messages) {
        match source["type"].as_str() {
            Some("text") => ("text/plain", required(source, "data")?.to_owned()),
            Some("content") => {
                let content = &source["content"];
                let text = if let Some(t) = content.as_str() {
                    t.to_owned()
                } else {
                    let mut text = String::new();
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
                        text.push_str(required(part, "text")?);
                    }
                    text
                };
                ("text/plain", text)
            }
            Some("base64") => {
                let media = required(source, "media_type")?;
                validate_media(media, kind)?;
                (media, decode_text(required(source, "data")?)?)
            }
            _ => return Err(Message::Content(kind.into()).into()),
        }
    } else {
        let (media, data) = data_url(required(file, "file_data")?, kind)?;
        validate_media(media, kind)?;
        (media, decode_text(data)?)
    };
    validate_media(media, kind)?;
    Ok(Part::Document { title, text })
}
fn decode_text(data: &str) -> Result<String, Refusal> {
    String::from_utf8(bytes(data)?).map_err(|_| Refusal::invalid("text document must be UTF-8"))
}
fn validate_media(media: &str, kind: &str) -> Result<(), Refusal> {
    match media {
        "application/pdf" => Err(Message::Pdf(kind.into()).into()),
        "text/plain" => Ok(()),
        _ => Err(Message::Media(media.into()).into()),
    }
}

fn tools(api: Api, value: &Value) -> Result<Vec<crate::conversation::ClientTool>, Refusal> {
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
                let mut member_tool = client_tool(api, member, Some(namespace))?;
                member_tool.description = format!(
                    "Namespace {namespace}:\n{namespace_description}\n\n{}",
                    member_tool.description
                );
                normalized.push(member_tool);
            }
        } else {
            normalized.push(client_tool(api, tool, None)?);
        }
    }
    Ok(normalized)
}
fn client_tool(
    api: Api,
    tool: &Value,
    namespace: Option<&str>,
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
    let schema = match kind {
        ToolKind::Custom => {
            match tool["format"]["type"].as_str() {
                Some("grammar") => {
                    let definition = required(&tool["format"], "definition")?;
                    let syntax = required(&tool["format"], "syntax")?;
                    description.push_str(&format!("\n\nGrammar ({syntax}):\n{definition}"));
                }
                None | Some("text") => {}
                Some(_) => return Err(Refusal::invalid("unsupported custom tool format")),
            }
            serde_json::json!({"type":"object","properties":{"input":{"type":"string"}},"required":["input"],"additionalProperties":false}).to_string()
        }
        ToolKind::Function => {
            let schema = &function[if matches!(api, Api::Messages) {
                "input_schema"
            } else {
                "parameters"
            }];
            if !schema.is_object() {
                return Err(Refusal::invalid("tool schema must be an object"));
            }
            schema.to_string()
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
