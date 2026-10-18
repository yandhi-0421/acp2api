# language: en
Feature: Retry wait
  An error of the 503 row or the 502 row starts a retry wait of 30 s for its conversation and its model ID.
  In the retry wait, a request of that conversation on the same model ID starts no prompt turn.

  Background:
    Given acp2api is in the normal state
    And the official runtime offers the model name "gemini-3.8-flash" with the thinking levels "low", "medium", and "high"
    And the request "R" to "POST /v1/messages" has the system prompt "Be kind.", the user message "Hello", the client tool "read_file", and the model "gemini-3.8-flash"

  @FR-029 @issue-59 @wip
  Scenario: The same request 5 s after an error of the 503 row gets HTTP 503 and starts no prompt turn
    Given the prompt turn of "R" failed at the time 0 s with the stderr error line 503 "UNAVAILABLE" "The service is currently unavailable."
    When the API client sends "R" again at the time 5 s
    Then acp2api refuses the request with HTTP 503 and the header "retry-after: 25"
    And the message is "acp2api: a request of this conversation failed less than 30 s ago. Send the request again in 25 s. The error was: upstream error 503 UNAVAILABLE: The service is currently unavailable."
    And the official runtime gets no prompt

  @FR-029 @issue-59 @wip
  Scenario Outline: The retry wait ends at exactly 30 seconds
    Given the prompt turn of "R" failed at the time 0 s with an error of the 503 row
    When the API client sends "R" again at the time <time>
    Then <result>

    Examples:
      | time     | result                                                    |
      | 29999 ms | acp2api refuses the request with HTTP 503 and no prompt    |
      | 30 s     | the official runtime gets a prompt for "R"               |
      | 30001 ms | the official runtime gets a prompt for "R"               |

  @FR-029 @issue-59 @wip
  Scenario: A refused request does not make the retry wait longer
    Given the prompt turn of "R" failed at the time 0 s with an error of the 503 row
    And acp2api refused "R" at the time 25 s
    When the API client sends "R" again at the time 31 s
    Then the official runtime gets a prompt for "R"

  @FR-029 @issue-59 @wip
  Scenario: A request that continues the failed conversation gets the same status in the retry wait
    Given the prompt turn of "R" failed at the time 0 s with an error of the 503 row
    When the API client sends "R" with the new user message "Are you there?" at the time 5 s
    Then acp2api refuses the request with HTTP 503 and the header "retry-after: 25"
    And the official runtime gets no prompt

  @FR-029 @issue-59 @wip
  Scenario Outline: The model ID of the request decides if the retry wait applies
    Given the prompt turn of "R" failed at the time 0 s with an error of the 503 row
    When the API client sends "R" with <model selection> at the time 5 s
    Then <result>

    Examples:
      | model selection                                                     | result                                         |
      | the model "gemini-3.8-flash-low"                                    | the official runtime gets a prompt for the request |
      | the model "gemini-3.8-flash-high"                                   | acp2api refuses the request with HTTP 503      |
      | the model "gemini-3.8-flash" and "output_config.effort" set to "max" | acp2api refuses the request with HTTP 503      |

  @FR-029 @issue-59 @wip
  Scenario Outline: stream and max_tokens do not change the match
    Given the prompt turn of "R" failed at the time 0 s with an error of the 503 row
    When the API client sends "R" with <change> at the time 5 s
    Then acp2api refuses the request with HTTP 503

    Examples:
      | change                    |
      | "stream" set to true      |
      | "max_tokens" set to 2048  |

  @FR-029 @issue-59 @wip
  Scenario Outline: Another system prompt or another set of client tools is another conversation
    Given the prompt turn of "R" failed at the time 0 s with an error of the 503 row
    When the API client sends "R" with <change> at the time 5 s
    Then the official runtime gets a prompt for the request

    Examples:
      | change                                   |
      | the system prompt "Be brief."            |
      | the client tools "read_file" and "list_files" |

  @FR-029 @issue-59 @wip
  Scenario: An error of the 502 row starts a retry wait with its own status
    Given the prompt turn of "R" ended at the time 0 s with the stop reason "max_turn_requests"
    When the API client sends "R" again at the time 5 s
    Then acp2api refuses the request with HTTP 502
    And the message is "acp2api: a request of this conversation failed less than 30 s ago. Send the request again in 25 s. The error was: the prompt turn reached the limit of model requests of the official runtime."
    And the official runtime gets no prompt

  @FR-029 @issue-59 @wip
  Scenario: acp2api keeps each retry wait only in memory
    Given the prompt turn of "R" failed at the time 0 s with an error of the 503 row
    And the owner restarted acp2api at the time 5 s
    When the API client sends "R" again at the time 10 s
    Then the official runtime gets a prompt for "R"
