# acp2api

## Ignored request fields

acp2api accepts and ignores these hints and defaults. Their values do not change the prompt, system prompt addition, client tools, or ACP session configuration.

- Top level: `temperature`, `top_p`, `top_k`, `presence_penalty`, `frequency_penalty`, `seed`, `logit_bias`, `max_tokens`, `max_completion_tokens`, `max_output_tokens`, `prompt_cache_key`, `service_tier`, `truncation`, `prediction`, `user`, `safety_identifier`, `client_metadata`, `context_management`, `parallel_tool_calls`, `max_tool_calls`, `verbosity`, and `store` with each value
- Content blocks: `cache_control`
- Client tools: `strict` (also `function.strict`), `defer_loading`, `eager_input_streaming`
- Configuration objects: `metadata.user_id`, `output_config.task_budget`, `text.verbosity`, `reasoning.summary`, `thinking.display`
- Chat Completions: `response_format` with the type `json_object`
- Responses: `include: ["reasoning.encrypted_content"]`
- Other top-level fields: acp2api records their names in the local log

A value that disables a feature counts as an absent field. Examples are `logprobs: false`, `background: false`, `audio: null`, and document `citations.enabled: false`.
