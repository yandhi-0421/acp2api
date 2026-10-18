# language: en
Feature: Endpoints and streams
  acp2api serves the Anthropic Messages API, the OpenAI Responses API, the OpenAI Chat Completions API, and the list of model names.
  Each POST endpoint gives a stream when "stream" is true, and one body when it is not.

  Background:
    Given acp2api is in the normal state
    And the official runtime offers the model name "gemini-3.8-flash"

  @FR-010 @issue-38 @wip
  Scenario Outline: Each POST endpoint answers a valid request in the success form of its API
    Given the official runtime replies "Hello" to the next prompt
    When an API client sends a valid request to "<endpoint>" with "stream" set to <stream>
    Then acp2api answers with HTTP 200 and <form> in the success form of the <api>
    And the reply text in the answer is "Hello"

    Examples:
      | endpoint                    | stream | form      | api                           |
      | POST /v1/messages           | false  | one body  | Anthropic Messages API        |
      | POST /v1/messages           | true   | a stream  | Anthropic Messages API        |
      | POST /v1/messages?beta=true | false  | one body  | Anthropic Messages API        |
      | POST /v1/responses          | false  | one body  | OpenAI Responses API          |
      | POST /v1/responses          | true   | a stream  | OpenAI Responses API          |
      | POST /v1/chat/completions   | false  | one body  | OpenAI Chat Completions API   |
      | POST /v1/chat/completions   | true   | a stream  | OpenAI Chat Completions API   |

  @FR-010 @issue-38 @wip
  Scenario: acp2api has no endpoint that counts tokens
    When an API client sends a valid request to "POST /v1/messages/count_tokens"
    Then acp2api answers with HTTP 404

  @FR-010 @issue-38 @wip
  Scenario: A stream of the Anthropic Messages API has its events in the order of that API
    Given the official runtime replies "Hello" to the next prompt
    When an API client sends a valid request with a stream to "POST /v1/messages"
    Then the answer has the content type "text/event-stream"
    And the stream has these events in this order:
      | event                                  |
      | message_start                          |
      | content_block_start                    |
      | one or more content_block_delta events |
      | content_block_stop                     |
      | message_delta                          |
      | message_stop                           |

  @FR-010 @issue-38 @wip
  Scenario: A stream of the OpenAI Responses API ends with response.completed
    Given the official runtime replies "Hello" to the next prompt
    When an API client sends a valid request with a stream to "POST /v1/responses"
    Then the answer has the content type "text/event-stream"
    And the last event of the stream is "response.completed"
    And the event "response.completed" carries "response.id"

  @FR-010 @issue-38 @wip
  Scenario: A stream of the OpenAI Chat Completions API ends with data: [DONE]
    Given the official runtime replies "Hello" to the next prompt
    When an API client sends a valid request with a stream to "POST /v1/chat/completions"
    Then the answer has the content type "text/event-stream"
    And each line of the stream before the last line is a "data:" line with a chunk
    And the last line of the stream is "data: [DONE]"

  @FR-010 @issue-38 @wip
  Scenario Outline: The first text delta leaves acp2api before the prompt turn ends
    Given the official runtime sends the text chunks "Hel" and "lo", and then waits 10 s before the end of the prompt turn
    When an API client sends a valid request with a stream to "<endpoint>"
    Then the API client gets a text delta with "Hel" before the prompt turn ends

    Examples:
      | endpoint                  |
      | POST /v1/messages         |
      | POST /v1/responses        |
      | POST /v1/chat/completions |

  @FR-011 @issue-65 @wip
  Scenario Outline: A stream gets keep-alive events while its request waits in the queue
    Given each place holds a prompt turn for the next 330 s
    When an API client sends a valid request with a stream to "<endpoint>"
    Then the stream has no gap of 300 s or more between two events

    Examples:
      | endpoint                  |
      | POST /v1/messages         |
      | POST /v1/responses        |
      | POST /v1/chat/completions |

  @FR-011 @issue-65 @wip
  Scenario Outline: A stream gets keep-alive events while its prompt turn sends no update
    Given the official runtime sends the text chunks "Hel" and "lo", and then sends no update until acp2api cancels the prompt turn
    When an API client sends a valid request with a stream to "<endpoint>"
    Then the stream has no gap of 300 s or more between two events

    Examples:
      | endpoint                  |
      | POST /v1/messages         |
      | POST /v1/responses        |
      | POST /v1/chat/completions |

  @FR-011 @issue-65 @wip
  Scenario: The keep-alive event of the Anthropic Messages API is ping
    Given each place holds a prompt turn for the next 330 s
    When an API client sends a valid request with a stream to "POST /v1/messages"
    Then the stream has "ping" events in the wait
    And each event of the stream in the wait, other than "message_start", is a "ping" event

  @FR-011 @issue-65 @wip
  Scenario: The keep-alive event of the OpenAI Responses API is an event of that API
    Given each place holds a prompt turn for the next 330 s
    When an API client sends a valid request with a stream to "POST /v1/responses"
    Then each keep-alive event of the stream is an event of the OpenAI Responses API, not a comment line
