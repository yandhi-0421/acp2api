use super::{Parsed, parsed};
use crate::{
    conversation::{Conversation, Item, Part, Role},
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
    let model = required(&value, "model")?.to_owned();
    let input = &value[if matches!(api, Api::Responses) {
        "input"
    } else {
        "messages"
    }];
    let mut items = Vec::new();
    if matches!(api, Api::Responses) && input.is_string() {
        items.push(Item::Message {
            role: Role::User,
            parts: vec![Part::Text(input.as_str().unwrap().into())],
        });
    } else {
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
            let parts = parts(api, &item["content"])?;
            items.push(Item::Message { role, parts });
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
    Ok(parsed(
        model,
        Conversation {
            earlier: items,
            last,
        },
        value["stream"].as_bool().unwrap_or(false),
    ))
}
fn required<'a>(v: &'a Value, key: &str) -> Result<&'a str, Refusal> {
    v[key]
        .as_str()
        .ok_or_else(|| Refusal::invalid(format!("string field {key} required")))
}
fn parts(api: Api, value: &Value) -> Result<Vec<Part>, Refusal> {
    if let Some(text) = value.as_str() {
        return Ok(vec![Part::Text(text.into())]);
    }
    value
        .as_array()
        .ok_or_else(|| Refusal::invalid("content must be text or blocks"))?
        .iter()
        .map(|v| part(api, v))
        .collect()
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
