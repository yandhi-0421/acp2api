# language: en
Feature: Model names and thinking levels
  acp2api offers a model name for each value of the config option "model" of the official runtime, also with the suffix of each thinking level.
  The model name and the effort fields of a request select the thinking level of the prompt turn.
  In a quoted message, a value in angle brackets is a value that acp2api fills in, as the section "Message texts" of the PRD gives.

  Background:
    Given acp2api is in the normal state
    And the config option "model" of the official runtime has one value "gemini-3.8-flash" with the name "Gemini 3.8 Flash"
    And the model "gemini-3.8-flash" has the thinking levels "low", "medium", and "high"

  @FR-012 @issue-43
  Scenario: The list of model names holds each value with each thinking level suffix
    When an API client sends "GET /v1/models"
    Then the list holds exactly these model names:
      | model name              |
      | gemini-3.8-flash        |
      | gemini-3.8-flash-low    |
      | gemini-3.8-flash-medium |
      | gemini-3.8-flash-high   |

  @FR-012 @issue-43
  Scenario: Each entry of the list has the fields of the OpenAI APIs and of the Anthropic Messages API
    When an API client sends "GET /v1/models"
    Then the answer has "object" with the value "list"
    And each entry of "data" has the fields "id", "object", "type", "display_name", "created", "created_at", and "owned_by"
    And each entry has "object" with the value "model" and "type" with the value "model"
    And the entry with the "id" "gemini-3.8-flash" has the "display_name" "Gemini 3.8 Flash"

  @FR-012 @issue-43
  Scenario: The startup sign-in check supplies the model names
    Given acp2api started and got no request
    Then the official runtime has one ACP session from the sign-in check
    And the official runtime got no prompt
    When an API client sends "GET /v1/models"
    Then the list holds "gemini-3.8-flash" and its three thinking level suffixes
    And the official runtime still has only that ACP session

  @FR-012 @issue-43
  Scenario: The list holds a new model after the next new ACP session
    Given an API client read the list of model names
    And the official runtime then adds the value "gemini-3.9-pro" with the thinking level "high" to the config option "model"
    When a request makes a new ACP session
    And an API client sends "GET /v1/models"
    Then the list holds the model names "gemini-3.9-pro" and "gemini-3.9-pro-high"

  @FR-012 @issue-43
  Scenario Outline: A request with a model that is not a model name gets HTTP 400
    When an API client sends a request to "POST /v1/messages" with the model "<model>"
    Then acp2api refuses the request with HTTP 400 and the message "acp2api: the model name <model> is not available. The model names are: <names>."
    And the message lists the four model names
    And the official runtime gets no prompt

    Examples:
      | model                 | names   |
      | claude-opus-5-5       | <names> |
      | gpt-5.5               | <names> |
      | gemini-3.8-flash-xhigh | <names> |

  @FR-012 @issue-71 @wip
  Scenario Outline: The list of model names gives the error of a state that stops requests
    Given acp2api is in the state "<state>"
    When an API client sends "GET /v1/models"
    Then acp2api refuses the request with HTTP <status> and the message of the state "<state>"

    Examples:
      | state               | status |
      | lockout             | 400    |
      | quota wait          | 429    |
      | sign-in required    | 400    |
      | runtime unavailable | 400    |

  @FR-013 @issue-48
  Scenario: The suffix of a model name sets the thinking level and acp2api ignores the effort fields
    When an API client sends a request to "POST /v1/responses" with the model "gemini-3.8-flash-low" and "reasoning.effort" set to "high"
    Then the prompt turn runs with the thinking level "low"

  @FR-013 @issue-48
  Scenario Outline: Each value of an effort field gives its thinking level
    When an API client sends a request to "POST /v1/chat/completions" with the model "gemini-3.8-flash" and "reasoning_effort" set to "<value>"
    Then the prompt turn runs with the thinking level "<level>"

    Examples:
      | value   | level  |
      | none    | low    |
      | minimal | low    |
      | low     | low    |
      | medium  | medium |
      | high    | high   |
      | xhigh   | high   |
      | max     | high   |
      | ultra   | high   |

  @FR-013 @issue-48
  Scenario Outline: Each effort field of each API sets the thinking level
    When an API client sends a request to "<endpoint>" with the model "gemini-3.8-flash" and <effort field>
    Then the prompt turn runs with the thinking level "<level>"

    Examples:
      | endpoint                  | effort field                                    | level  |
      | POST /v1/responses        | "reasoning.effort" set to "medium"              | medium |
      | POST /v1/messages         | "output_config.effort" set to "low"             | low    |
      | POST /v1/messages         | "thinking" with the type "disabled"             | low    |
      | POST /v1/messages         | "thinking" with the type "adaptive"             | high   |
      | POST /v1/messages         | "thinking" with the type "enabled" and "budget_tokens" 1024 | high |

  @FR-013 @issue-48
  Scenario: A request with no effort field runs at the thinking level high
    When an API client sends a request to "POST /v1/messages" with the model "gemini-3.8-flash" and no effort field
    Then the prompt turn runs with the thinking level "high"

  @FR-013 @issue-48
  Scenario: output_config.effort sets the thinking level when the request also has thinking
    When an API client sends a request to "POST /v1/messages" with the model "gemini-3.8-flash", "output_config.effort" set to "medium", and "thinking" with the type "disabled"
    Then the prompt turn runs with the thinking level "medium"

  @FR-013 @issue-48
  Scenario: acp2api ignores an effort value that the table does not give and writes it into the log
    When an API client sends a request to "POST /v1/chat/completions" with the model "gemini-3.8-flash" and "reasoning_effort" set to "extreme"
    Then the prompt turn runs with the thinking level "high"
    And the log has a line with the value "extreme"
    And the API client gets no error

  @FR-013 @issue-48
  Scenario Outline: A thinking level that the model does not have gives another level of the model
    Given the model "gemini-3.8-flash" has only the thinking levels <levels of the model>
    When an API client sends a request to "POST /v1/chat/completions" with the model "gemini-3.8-flash" and "reasoning_effort" set to "<value>"
    Then the prompt turn runs with the thinking level "<level>"

    Examples:
      | levels of the model   | value  | level  |
      | "low" and "high"      | medium | high   |
      | "low" and "medium"    | high   | medium |

  @FR-013 @issue-48
  Scenario Outline: The model field of each response holds the model name of the request
    When an API client sends a request to "POST /v1/chat/completions" with the model "<model name>"
    Then the response has "model" with the value "<model name>"

    Examples:
      | model name           |
      | gemini-3.8-flash     |
      | gemini-3.8-flash-low |
