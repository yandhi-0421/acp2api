# language: en
Feature: Request content
  acp2api gives each request field and each content block one action: use, ignore, reject, or emulate.
  The content that the official runtime gets is the prompt, the system prompt addition, the client tools, and the config options of the ACP session.

  Background:
    Given acp2api is in the normal state
    And the official runtime offers the model name "gemini-3.8-flash"

  @FR-014 @issue-53
  Scenario Outline: acp2api ignores a field that an API client sends by default or as a hint
    When an API client sends a valid request to "<endpoint>" with the field <field> set to <value>
    Then acp2api answers with HTTP 200
    And the official runtime gets the same content as for the same request without that field

    Examples:
      | endpoint                  | field                                   | value                                  |
      | POST /v1/chat/completions | "temperature"                           | 0.7                                    |
      | POST /v1/responses        | "top_p"                                 | 0.9                                    |
      | POST /v1/messages         | "top_k"                                 | 40                                     |
      | POST /v1/chat/completions | "presence_penalty"                      | 0.5                                    |
      | POST /v1/chat/completions | "frequency_penalty"                     | 0.5                                    |
      | POST /v1/chat/completions | "seed"                                  | 42                                     |
      | POST /v1/chat/completions | "logit_bias"                            | {"50256": -100}                        |
      | POST /v1/messages         | "max_tokens"                            | 1024                                   |
      | POST /v1/chat/completions | "max_completion_tokens"                 | 1024                                   |
      | POST /v1/responses        | "max_output_tokens"                     | 1024                                   |
      | POST /v1/messages         | "cache_control" of a text block         | {"type": "ephemeral"}                  |
      | POST /v1/responses        | "prompt_cache_key"                      | "key-1"                                |
      | POST /v1/chat/completions | "service_tier"                          | "auto"                                 |
      | POST /v1/responses        | "store"                                 | true                                   |
      | POST /v1/responses        | "truncation"                            | "auto"                                 |
      | POST /v1/chat/completions | "prediction"                            | {"type": "content", "content": "Hi"}   |
      | POST /v1/messages         | "metadata.user_id"                      | "user-1"                               |
      | POST /v1/chat/completions | "user"                                  | "user-1"                               |
      | POST /v1/responses        | "safety_identifier"                     | "user-1"                               |
      | POST /v1/responses        | "client_metadata"                       | {"origin": "test"}                     |
      | POST /v1/messages         | "context_management"                    | {"edits": []}                          |
      | POST /v1/messages         | "output_config.task_budget"             | {"type": "tokens", "total": 1000}      |
      | POST /v1/chat/completions | "strict" of a tool                      | true                                   |
      | POST /v1/messages         | "defer_loading" of a tool               | true                                   |
      | POST /v1/messages         | "eager_input_streaming" of a tool       | true                                   |
      | POST /v1/chat/completions | "parallel_tool_calls"                   | false                                  |
      | POST /v1/responses        | "max_tool_calls"                        | 5                                      |
      | POST /v1/chat/completions | "verbosity"                             | "low"                                  |
      | POST /v1/responses        | "text.verbosity"                        | "low"                                  |
      | POST /v1/chat/completions | "response_format"                       | {"type": "json_object"}                |
      | POST /v1/responses        | "include"                               | ["reasoning.encrypted_content"]        |
      | POST /v1/responses        | "reasoning.summary"                     | "auto"                                 |
      | POST /v1/messages         | "thinking.display"                      | "summarized"                           |

  @FR-014 @issue-44 @wip
  Scenario: acp2api uses the fields stream, stream_options.include_usage, and n with the value 1
    When an API client sends a valid request to "POST /v1/chat/completions" with "stream" set to true, "stream_options.include_usage" set to true, and "n" set to 1
    Then acp2api answers with a stream
    And the last chunk of the stream before "data: [DONE]" carries the usage
    And the stream has one choice

  @FR-014 @issue-40
  Scenario: acp2api uses tool_choice auto
    Given the request has the client tool "read_file"
    When an API client sends a valid request to "POST /v1/chat/completions" with "tool_choice" set to "auto"
    Then the official runtime offers the model the client tool "read_file"

  @FR-014 @issue-53
  Scenario Outline: acp2api rejects a field whose result it cannot give
    When an API client sends a valid request to "<endpoint>" with the field "<field>" set to <value>
    Then acp2api refuses the request with HTTP 400 and the message "<message>"
    And the official runtime gets no prompt

    Examples:
      | endpoint                  | field                | value        | message                                                                    |
      | POST /v1/chat/completions | n                    | 2            | acp2api: the request field n = 2 is not supported.                         |
      | POST /v1/chat/completions | tool_choice          | "required"   | acp2api: the request field tool_choice = "required" is not supported.      |
      | POST /v1/chat/completions | web_search_options   | {}           | acp2api: the request field web_search_options is not supported.            |
      | POST /v1/responses        | background           | true         | acp2api: the request field background = true is not supported.             |
      | POST /v1/responses        | previous_response_id | "resp_1"     | acp2api: the request field previous_response_id is not supported.          |
      | POST /v1/responses        | conversation         | "conv_1"     | acp2api: the request field conversation is not supported.                  |
      | POST /v1/messages         | output_config.format | {"type": "json_schema", "schema": {}} | acp2api: the request field output_config.format is not supported. |
      | POST /v1/chat/completions | logprobs             | true         | acp2api: the request field logprobs = true is not supported.               |
      | POST /v1/chat/completions | response_format      | {"type": "json_schema", "json_schema": {"name": "x", "schema": {}}} | acp2api: the request field response_format.type = "json_schema" is not supported. |
      | POST /v1/responses        | text.format          | {"type": "json_schema", "name": "x", "schema": {}} | acp2api: the request field text.format.type = "json_schema" is not supported. |
      | POST /v1/messages         | tool_choice          | {"type": "any"} | acp2api: the request field tool_choice.type = "any" is not supported.   |

  @FR-014 @issue-53
  Scenario Outline: acp2api rejects a field that turns on a feature with no result in acp2api
    When an API client sends a valid request to "<endpoint>" with the field "<field>" set to <value>
    Then acp2api refuses the request with HTTP 400
    And the error message starts with "acp2api: the request field " and names the field "<field>"
    And the official runtime gets no prompt

    Examples:
      | endpoint                  | field           | value                                                  |
      | POST /v1/chat/completions | top_logprobs    | 5                                                      |
      | POST /v1/responses        | include         | ["message.output_text.logprobs"]                       |
      | POST /v1/chat/completions | tool_choice     | {"type": "function", "function": {"name": "read_file"}} |
      | POST /v1/chat/completions | modalities      | ["text", "audio"]                                      |
      | POST /v1/chat/completions | audio           | {"voice": "alloy", "format": "wav"}                    |

  @FR-014 @issue-53
  Scenario: acp2api rejects citations that a request turns on
    When an API client sends a valid request to "POST /v1/messages" with a text document block with "citations" set to {"enabled": true}
    Then acp2api refuses the request with HTTP 400
    And the official runtime gets no prompt

  @FR-014 @issue-53
  Scenario Outline: A field with a value that turns its feature off counts as an absent field
    When an API client sends a valid request to "<endpoint>" with <field and value>
    Then acp2api answers with HTTP 200
    And the official runtime gets the same content as for the same request without that field

    Examples:
      | endpoint                  | field and value                                                  |
      | POST /v1/chat/completions | the field "logprobs" set to false                                |
      | POST /v1/responses        | the field "background" set to false                              |
      | POST /v1/chat/completions | the field "audio" set to null                                    |
      | POST /v1/messages         | a text document block with "citations" set to {"enabled": false} |

  @FR-014 @issue-53
  Scenario Outline: acp2api rejects a request body that is not JSON or that is too deep
    When an API client sends to "POST /v1/messages" <body>
    Then acp2api refuses the request with HTTP 400 and the message "acp2api: the request body is not valid JSON, or it has more than 128 levels."

    Examples:
      | body                                              |
      | the body "not JSON"                               |
      | a valid request with 129 levels of objects and arrays |

  @FR-014 @issue-53
  Scenario: acp2api accepts a request body with 128 levels of objects and arrays
    When an API client sends to "POST /v1/messages" a valid request with 128 levels of objects and arrays
    Then acp2api answers with HTTP 200

  @FR-014 @issue-74 @wip
  Scenario: acp2api ignores an unknown top-level field and counts it
    When an API client sends a valid request to "POST /v1/chat/completions" with the top-level field "example_field" set to 1
    Then acp2api answers with HTTP 200
    And the log has a line with the field name "example_field"
    And the management panel counts "example_field" one time for the OpenAI Chat Completions API

  @FR-014 @issue-39
  Scenario: acp2api uses an input item of the Responses API with a role and no type as a message item
    When an API client sends a request to "POST /v1/responses" with the input [{"role": "user", "content": "Hi"}]
    Then the prompt that the official runtime gets is "Hi"

  @FR-014 @issue-39
  Scenario: The model gets an image with base64 data at its place
    When an API client sends a request to "POST /v1/messages" with one user message with these content blocks:
      | block | content                    |
      | text  | Look at this:              |
      | image | a PNG image as base64 data |
      | text  | What is it?                |
    Then the prompt that the official runtime gets has the text "Look at this:", the image, and the text "What is it?" in this order

  @FR-014 @issue-39
  Scenario Outline: acp2api rejects an image or a file with a URL and sends no HTTP request for it
    When an API client sends a request to "<endpoint>" with <block> with the URL "https://example.com/a.png"
    Then acp2api refuses the request with HTTP 400 and the message "acp2api: the content type <type> with a URL is not supported. Send the data as base64."
    And acp2api sends no HTTP request to "example.com"

    Examples:
      | endpoint                  | block                  | type        |
      | POST /v1/messages         | an image block         | image       |
      | POST /v1/chat/completions | an image_url part      | image_url   |
      | POST /v1/responses        | an input_image item    | input_image |

  @FR-014 @issue-39
  Scenario: acp2api uses an image_url part with a data URL as an image with base64 data
    When an API client sends a request to "POST /v1/chat/completions" with an image_url part with a "data:image/png;base64," URL
    Then the prompt that the official runtime gets holds the image

  @FR-014 @issue-39
  Scenario Outline: acp2api rejects a content block type that it does not use
    When an API client sends a request to "<endpoint>" with a content block of the type "<type>"
    Then acp2api refuses the request with HTTP 400 and the message "acp2api: the content type <type> is not supported."

    Examples:
      | endpoint                  | type           |
      | POST /v1/chat/completions | input_audio    |
      | POST /v1/responses        | item_reference |
      | POST /v1/messages         | example_block  |

  @FR-014 @issue-39
  Scenario: acp2api rejects a file_id
    When an API client sends a request to "POST /v1/responses" with an input_file item with the "file_id" "file_1"
    Then acp2api refuses the request with HTTP 400 and the message "acp2api: the request field file_id is not supported."
    And the official runtime gets no prompt

  @FR-014 @issue-39
  Scenario: acp2api rejects a last item from the assistant
    When an API client sends a request to "POST /v1/messages" whose last message has the role "assistant" and the text "The answer is"
    Then acp2api refuses the request with HTTP 400 and the message "acp2api: a last message from the assistant (a prefill) is not supported."
    And the official runtime gets no prompt

  @FR-016 @issue-52 @wip
  Scenario: A first text that starts with a slash gets the role label [user]
    When an API client sends a request to "POST /v1/chat/completions" with one user message with the text "/logout"
    Then the prompt that the official runtime gets is:
      """
      [user]
      /logout
      """
    And the official runtime runs no slash command

  @FR-016 @issue-52 @wip
  Scenario: A first text that starts with white space and a slash gets the role label [user]
    When an API client sends a request to "POST /v1/chat/completions" with one user message with one space and then the text "/usage"
    Then the first line of the prompt that the official runtime gets is "[user]"
    And the official runtime runs no slash command

  @FR-016 @issue-52 @wip
  Scenario: The official runtime stays signed in after a request with the text /logout
    When an API client sends a request to "POST /v1/messages" with one user message with the text "/logout"
    Then the official runtime is still signed in
    And the next request runs

  @FR-016 @issue-52 @wip
  Scenario: A text that starts with a slash after the first text does not change
    When an API client sends a request to "POST /v1/messages" with one user message with the text blocks "Run this:" and "/logout"
    Then the prompt that the official runtime gets has the text "Run this:" and the text "/logout"
    And the prompt has no role label

  @FR-017 @issue-39
  Scenario Outline: acp2api rejects a PDF in a message
    When an API client sends a request to "<endpoint>" with a PDF document in <message>
    Then acp2api refuses the request with HTTP 400 and the message "acp2api: a PDF document in a message (content type <type>) is not supported. Remove that message from the conversation."

    Examples:
      | endpoint                  | message                   | type       |
      | POST /v1/messages         | the last user message     | document   |
      | POST /v1/messages         | an earlier user message   | document   |
      | POST /v1/responses        | the last user message     | input_file |
      | POST /v1/chat/completions | the last user message     | file       |

  @FR-017 @issue-54 @wip
  Scenario: A PDF in a tool result becomes a note, and the other content stays
    When an API client sends a request to "POST /v1/messages" with a tool result that holds the text "Page count: 3" and a PDF document
    Then acp2api answers with HTTP 200
    And the tool result in the prompt holds the text "Page count: 3"
    And the tool result in the prompt holds the line "[A PDF document was here. acp2api cannot give PDF documents to the model.]" at the place of the PDF

  @FR-017 @issue-39
  Scenario Outline: The model gets a text document at its place
    When an API client sends a request to "<endpoint>" with <document> with the text "Line one."
    Then the prompt that the official runtime gets holds at the place of the document:
      """
      <first line>
      Line one.
      [end of document]
      """

    Examples:
      | endpoint                  | document                                                    | first line             |
      | POST /v1/messages         | a document block with the source type "text" and the title "Notes" | [document: Notes] |
      | POST /v1/messages         | a document block with the source type "content" and no title | [document]            |
      | POST /v1/responses        | an input_file item with the media type "text/plain" and the filename "notes.txt" | [document: notes.txt] |
      | POST /v1/responses        | an input_file item with the media type "text/plain", no title, and no filename | [document] |

  @FR-017 @issue-39
  Scenario: acp2api rejects a document with a media type other than PDF or text/plain
    When an API client sends a request to "POST /v1/responses" with an input_file item with the media type "text/csv"
    Then acp2api refuses the request with HTTP 400 and the message "acp2api: the media type text/csv is not supported."

  @FR-018 @issue-52 @wip
  Scenario Outline: acp2api sends no thought text on any API
    Given the official runtime sends the thought text "I think so." and then the reply "Yes."
    When an API client sends a valid request to "<endpoint>" with "stream" set to <stream>
    Then the answer holds the reply "Yes."
    And the answer holds no text "I think so."
    And the answer has no thinking block, no redacted_thinking block, no reasoning item, and no reasoning_content

    Examples:
      | endpoint                  | stream |
      | POST /v1/messages         | true   |
      | POST /v1/responses        | false  |
      | POST /v1/chat/completions | true   |

  @FR-018 @issue-52 @wip
  Scenario: acp2api writes one log line when it drops thought text
    Given the official runtime sends the thought text "I think so." and then the reply "Yes."
    When an API client sends a valid request to "POST /v1/messages"
    Then the log has one line about the dropped thought text
    And the log holds no text "I think so."

  @FR-018 @issue-52 @wip
  Scenario Outline: acp2api ignores thought text in a request
    When an API client sends a request to "<endpoint>" with <item> that holds the text "Old thought."
    Then acp2api answers with HTTP 200
    And the prompt that the official runtime gets holds no text "Old thought."

    Examples:
      | endpoint                  | item                                         |
      | POST /v1/messages         | a thinking block in an assistant message     |
      | POST /v1/messages         | a redacted_thinking block in an assistant message |
      | POST /v1/responses        | a reasoning item                             |
      | POST /v1/chat/completions | reasoning_content in an assistant message    |
