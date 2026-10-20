# acp2api

## Ignored request fields

acp2api accepts these hints and defaults. Their values do not change the prompt, system prompt addition, client tools, or ACP session configuration.

| Field | Location or value | Action |
|---|---|---|
| `temperature` | Top level | Ignore |
| `top_p` | Top level | Ignore |
| `top_k` | Top level | Ignore |
| `presence_penalty` | Top level | Ignore |
| `frequency_penalty` | Top level | Ignore |
| `seed` | Top level | Ignore |
| `logit_bias` | Top level | Ignore |
| `max_tokens` | Top level | Ignore |
| `max_completion_tokens` | Top level | Ignore |
| `max_output_tokens` | Top level | Ignore |
| `cache_control` | Content blocks | Ignore |
| `prompt_cache_key` | Top level | Ignore |
| `service_tier` | Top level | Ignore |
| `store` with each value | Top level | Ignore |
| `truncation` | Top level | Ignore |
| `prediction` | Top level | Ignore |
| `metadata.user_id` | Request metadata | Ignore |
| `user` | Top level | Ignore |
| `safety_identifier` | Top level | Ignore |
| `client_metadata` | Top level | Ignore |
| `context_management` | Top level | Ignore |
| `output_config.task_budget` | Output configuration | Ignore |
| `strict` | Client tools, including `function.strict` | Ignore |
| `defer_loading` | Client tools | Ignore |
| `eager_input_streaming` | Client tools | Ignore |
| `parallel_tool_calls` | Top level | Ignore |
| `max_tool_calls` | Top level | Ignore |
| `verbosity` | Top level | Ignore |
| `text.verbosity` | Text configuration | Ignore |
| `response_format` with the type `json_object` | Chat Completions | Ignore |
| `include: ["reasoning.encrypted_content"]` | Responses | Ignore |
| `reasoning.summary` | Reasoning configuration | Ignore |
| `thinking.display` | Thinking configuration | Ignore |
| Other top-level fields | acp2api records their names in the local log. | Ignore |

A value that disables a feature counts as an absent field. Examples are `logprobs: false`, `background: false`, `audio: null`, and document `citations.enabled: false`.
