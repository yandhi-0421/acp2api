# Gemini thinking controls

Date checked: 2026-10-13 (Asia/Taipei).

Provenance: The maintainer wrote this note on 2026-10-13 for an earlier project, a bridge that used the Antigravity CLI (`agy`) as its engine. The note moved to acp2api on 2026-10-18, because that project will be deleted. The move removed the links to the files of that project and changed no fact. The note was never published in that project.

Scope: public Google documentation, and a local inspection of the CLI and of the earlier bridge. No generation request, credential read, or configuration change was necessary.

Words of the earlier project:

- The earlier bridge: the earlier CLI bridge of the maintainer. It translated the three APIs of acp2api to the Antigravity CLI.
- The CLI: the Antigravity CLI `agy`, the engine of the earlier bridge.

## Result for the earlier bridge

The path of the earlier bridge had no confirmed switch that turns Gemini thinking off. The selection of `gemini-3.8-flash-low` gives the lowest available effort variant. This selection decreases the effort. It does not promise zero thinking tokens.

Local checks on 2026-10-13 found:

- `agy --version` returned `1.3.4`. An earlier status screenshot showed `1.3.1`.
- `agy --help` listed `--effort low|medium|high|xhigh|max`. It listed no thinking budget, no thinking level, and no flag that turns thinking off.
- `agy models` listed `gemini-3.8-flash-high`, `gemini-3.8-flash-medium`, and `gemini-3.8-flash-low`. It listed no `minimal` or `off` Gemini variant.
- The earlier bridge accepted `reasoning.effort`, `reasoning_effort`, and `output_config.effort`, and the fields had no effect.
- The earlier bridge started each CLI turn with `--model <id>`, and it did not send `--effort`.

Thus a SillyTavern request such as `reasoning_effort: "none"` could not turn thinking off through the earlier bridge, because its code did not forward that field.

The public [Antigravity headless guide](https://www.antigravity.google/docs/cli/headless/) documents model slugs and `--effort low|medium|high`. It does not document a setting that turns thinking off. The local CLI help has two more effort values. Neither source proves that the CLI forwards the thinking fields of the Google API.

acp2api: the effort fields select the thinking level of the ACP session, and `none` and `minimal` select `low`, see [Which model names does the API of acp2api offer?](https://github.com/yandhi-0421/acp2api/issues/13).

## Direct Google API controls

The Google API has controls for each model. They do not automatically apply to the Antigravity CLI or to the official runtime.

| Model family | Available control | Can it turn thinking off? |
| --- | --- | --- |
| Gemini 3.8 Flash | `thinkingLevel: "low"`, `"medium"`, or `"high"`; default `"medium"` | No documented off value. `"minimal"` returns an error. |
| Gemini 3.6 Flash and Gemini 3 Flash | `"minimal"`, `"low"`, `"medium"`, `"high"` | Full thinking-off is not supported. Minimal can still use thought tokens. |
| Gemini 3.1 Pro | `"low"`, `"medium"`, `"high"` | Cannot disable thinking. |
| Gemini 2.5 Flash and Flash-Lite | `thinkingBudget: 0` | Yes. `-1` enables dynamic thinking instead. |
| Gemini 2.5 Pro | `thinkingBudget` | Cannot disable thinking. |

Source: Google [GenerateContent thinking guide](https://ai.google.dev/gemini-api/docs/generate-content/thinking?hl=en), updated 2026-10-09 UTC.

For direct GenerateContent requests, the configuration fragments are:

```json
{"generationConfig":{"thinkingConfig":{"thinkingBudget":0}}}
```

This fragment is only for a model that can turn thinking off, such as Gemini 2.5 Flash.

```json
{"generationConfig":{"thinkingConfig":{"thinkingLevel":"low"}}}
```

This fragment decreases the thinking of Gemini 3.8 Flash. These fragments describe the endpoint of Google, not a request of acp2api. acp2api never sends a request to Google itself.

The newer [Gemini Interactions thinking guide](https://ai.google.dev/gemini-api/docs/thinking?hl=en), updated 2026-10-09 UTC, uses `generation_config.thinking_level`. It also lists only `low`, `medium`, and `high` for `gemini-3.8-flash`. It warns that a small `max_output_tokens` limit can end a response during the reasoning and leave truncated or empty output. A token limit is not a control that turns thinking off.

The [Google Cloud thinking guide](https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/thinking), updated 2026-10-12 UTC, confirms the same three levels for Gemini 3.8 Flash. It warns that `thinking_budget` is deprecated for Gemini 3 and can cause intermittent `400: INVALID_ARGUMENT` errors. The zero budget of Gemini 2.5 is not a switch for Gemini 3.8.

## Output visibility and limits of the finding

The Google field `includeThoughts` controls whether GenerateContent returns thought summaries. It does not control the amount of internal reasoning. The [GenerateContent guide](https://ai.google.dev/gemini-api/docs/generate-content/thinking?hl=en) separates summaries from thinking levels and budgets.

The Antigravity setting `verbosity: "low"` controls the visual progress and the display of thoughts. The [CLI reference](https://www.antigravity.google/docs/cli/reference/) describes it as a setting of the display. It does not show a decrease of the model compute.

The Antigravity slugs identify effort variants, but the public documentation does not show their exact backend request or numeric thought budget. The table of the direct API shows the public model capabilities. It does not prove the internal map of the CLI. No generation probe measured the thinking tokens of each CLI variant.
