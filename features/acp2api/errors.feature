# language: en
Feature: Errors
  acp2api accepts a request after the checks of the local API key, of the request, of the state, and of the queue.
  Each refusal before the acceptance is an HTTP status. Each later error of a streamed request is a stream event.

  Background:
    Given acp2api is in the normal state
    And the official runtime offers the model name "gemini-3.8-flash"

  @FR-026 @issue-60
  Scenario: A streamed request that passed all admission checks has status 200 in the queue
    Given each place holds a prompt turn
    When an API client sends a valid request with a stream to "POST /v1/messages"
    Then the request waits in the queue
    And the API client already has the status 200 and the headers of the stream

  @FR-032 @FR-033 @issue-50
  Scenario Outline: An ACP error without an HTTP error line keeps its JSON-RPC values
    Given the next ACP call fails with code -32099 and message "Synthetic ACP failure"
    And the official runtime sends no stderr error line
    When an API client sends a valid request with no stream to "<endpoint>"
    Then the request gets HTTP 502 in the error envelope of its API
    And the error message is "acp2api: upstream ACP error -32099: Synthetic ACP failure"

    Examples:
      | endpoint                  |
      | POST /v1/messages         |
      | POST /v1/responses        |
      | POST /v1/chat/completions |

  @FR-026 @issue-49
  Scenario: A streamed request with a wrong local API key gets HTTP 401 and no stream event
    When an API client sends a valid request with a stream to "POST /v1/messages" with a wrong local API key
    Then acp2api refuses the request with HTTP 401 in the error envelope of the Anthropic Messages API
    And the answer has no stream event

  @FR-026 @issue-43
  Scenario Outline: A refusal before the acceptance has the error envelope of its API
    When an API client sends a request with a stream to "<endpoint>" with the model "claude-opus-5-5"
    Then acp2api refuses the request with HTTP 400 in the error envelope of the <api>
    And the answer has no stream event

    Examples:
      | endpoint                  | api                         |
      | POST /v1/messages         | Anthropic Messages API      |
      | POST /v1/responses        | OpenAI Responses API        |
      | POST /v1/chat/completions | OpenAI Chat Completions API |

  @FR-027 @issue-71 @wip
  Scenario Outline: An error after the acceptance is an error event in an Anthropic stream
    Given a streamed request to "POST /v1/messages" was accepted
    When <case> ends the request
    Then the stream has an "error" event with the "error.type" "<error type>"
    And no "message_stop" event follows the "error" event

    Examples:
      | case                                        | error type            |
      | a lockout                                   | invalid_request_error |
      | the start of the state "sign-in required"   | invalid_request_error |
      | the start of the state "runtime unavailable" | invalid_request_error |
      | the exit of acp2api                         | invalid_request_error |
      | the start of a quota wait                   | rate_limit_error      |
      | an error of the 503 row                     | api_error             |
      | an error of the 502 row                     | invalid_request_error |

  @FR-027 @issue-50
  Scenario: After text, an Anthropic stream closes the text block before the error event
    Given the official runtime sent the text chunks "Hel" and "lo" in a streamed request to "POST /v1/messages"
    When the prompt turn ends with an error of the 503 row
    Then the stream has the text "Hel"
    And the stream has a "content_block_stop" event for the open text block and then the "error" event

  @FR-027 @issue-71 @wip
  Scenario Outline: Each error other than the 503 row is a response.failed event with the code invalid_prompt
    Given a streamed request to "POST /v1/responses" was accepted
    When <case> ends the request
    Then the stream has a "response.failed" event with the code "invalid_prompt"

    Examples:
      | case                                        |
      | a lockout                                   |
      | the start of the state "sign-in required"   |
      | the start of the state "runtime unavailable" |
      | the exit of acp2api                         |
      | the start of a quota wait                   |
      | an error of the 502 row                     |

  @FR-027 @issue-50
  Scenario: An error of the 503 row is a response.failed event with the code server_error
    Given a streamed request to "POST /v1/responses" was accepted
    When an error of the 503 row ends the request
    Then the stream has a "response.failed" event with the code "server_error"
    And "error.headers" of the event holds "retry-after" with the value 30

  @FR-027 @issue-50
  Scenario: An error in a Chat Completions stream is one data line and no [DONE]
    Given a streamed request to "POST /v1/chat/completions" was accepted
    When an error of the 503 row ends the request
    Then the stream has one "data:" line with an "error" object
    And the stream ends after that line with no "data: [DONE]"

  @FR-032 @issue-71 @wip
  Scenario Outline: Each error of acp2api holds the text of its case
    When <case> ends a request to "<endpoint>"
    Then <place> holds the message "<message>"

    Examples:
      | case                                                     | endpoint                  | place                         | message                                                                                          |
      | an absent local API key                                  | POST /v1/messages         | the error body                | acp2api: the local API key is absent or wrong.                                                   |
      | a prompt turn with no update for 5 minutes in a stream   | POST /v1/messages         | the "error" event             | acp2api: the official runtime sent no update for 5 minutes, so acp2api cancelled the prompt turn. |
      | a failure of the runtime process in a stream             | POST /v1/responses        | the "response.failed" event   | acp2api: the runtime process failed, so acp2api ended the prompt turn.                           |
      | the stop reason "max_turn_requests" in a stream          | POST /v1/chat/completions | the "error" object            | acp2api: the prompt turn reached the limit of model requests of the official runtime.            |

  @FR-033 @issue-53
  Scenario Outline: A request that acp2api rejects gets the status and the error.type of its row
    Given <prior state>
    When an API client sends a request to "POST /v1/messages" with "stream" set to false
    Then acp2api answers with HTTP <status>
    And the error body has the "error.type" "<error type>"

    Examples:
      | prior state                                       | status | error type            |
      | the request has no local API key                  | 401    | authentication_error  |
      | the request has the field "output_config.format"  | 400    | invalid_request_error |

  @FR-033 @issue-71 @wip
  Scenario Outline: Each other error case of a request with no stream gets the status, the headers, and the error.type of its row
    Given <prior state>
    When an API client sends a request to "POST /v1/messages" with "stream" set to false
    Then acp2api answers with HTTP <status> and <headers>
    And the error body has the "error.type" "<error type>"

    Examples:
      | prior state                                                                        | status | headers                       | error type            |
      | acp2api is in lockout                                                              | 400    | the header "x-should-retry: false" | invalid_request_error |
      | acp2api is in the state "sign-in required"                                         | 400    | the header "x-should-retry: false" | invalid_request_error |
      | acp2api is in the state "runtime unavailable"                                      | 400    | the header "x-should-retry: false" | invalid_request_error |
      | the request waits in the queue when the owner stops acp2api                        | 400    | the header "x-should-retry: false" | invalid_request_error |
      | acp2api is in a quota wait whose reset time is 120 s away                          | 429    | the header "retry-after: 120" | rate_limit_error      |
      | the queue holds 32 requests                                                        | 529    | the header "retry-after: 10"  | overloaded_error      |
      | the next prompt turn gets the upstream error 503 UNAVAILABLE                       | 503    | the header "retry-after: 30"  | api_error             |
      | the next prompt turn sends no update for 5 minutes                                 | 503    | the header "retry-after: 30"  | api_error             |
      | the runtime process exits during the next prompt turn                              | 503    | the header "retry-after: 30"  | api_error             |
      | the next prompt turn gets the upstream error 400 INVALID_ARGUMENT                  | 502    | the header "x-should-retry: false" | api_error        |
      | the next prompt turn ends with the stop reason "max_turn_requests"                 | 502    | the header "x-should-retry: false" | api_error        |
      | a built-in tool runs with no permission request in the next prompt turn            | 502    | the header "x-should-retry: false" | api_error        |

  @FR-033 @issue-60
  Scenario Outline: A full queue gets HTTP 503 on the OpenAI APIs
    Given the queue holds 32 requests
    When an API client sends a valid request to "<endpoint>"
    Then acp2api answers with HTTP 503 and the header "retry-after: 10"

    Examples:
      | endpoint                  |
      | POST /v1/responses        |
      | POST /v1/chat/completions |

  @FR-033 @issue-61
  Scenario: An upstream error that is a quota signal gets the row of the quota wait
    Given the next prompt turn gets the upstream error 429 RESOURCE_EXHAUSTED with a reset time 120 s away
    When an API client sends a request to "POST /v1/messages" with "stream" set to false
    Then acp2api answers with HTTP 429 and the header "retry-after: 120"
    And the error body has the "error.type" "rate_limit_error"

  @FR-033 @issue-50
  Scenario: The message of an upstream error holds the values of the stderr error line
    Given the next prompt turn gets the stderr error line with the code 503, the status "UNAVAILABLE", and the message "The service is currently unavailable."
    When an API client sends a request to "POST /v1/messages" with "stream" set to false
    Then the error body holds the message "acp2api: upstream error 503 UNAVAILABLE: The service is currently unavailable."
