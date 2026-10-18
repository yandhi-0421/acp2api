# language: en
Feature: Token usage
  The input tokens come from the last usage update of the prompt turn.
  acp2api estimates the output tokens from the text and the tool call arguments that it returns.

  Background:
    Given acp2api is in the normal state
    And the official runtime offers the model name "gemini-3.8-flash"

  @FR-025
  Scenario Outline: The input tokens are the used field of the last usage update
    Given the prompt turn has the usage updates with "used" 4000 and then "used" 5183
    When an API client sends a valid request to "<endpoint>" with "stream" set to false
    Then the body of the response carries the usage with "<field>" set to 5183

    Examples:
      | endpoint                  | field         |
      | POST /v1/messages         | input_tokens  |
      | POST /v1/responses        | input_tokens  |
      | POST /v1/chat/completions | prompt_tokens |

  @FR-025
  Scenario Outline: The output tokens are the ASCII characters divided by 4 and rounded up, plus 1 for each other character
    Given the official runtime replies "<reply>" to the next prompt
    When an API client sends a valid request to "POST /v1/messages"
    Then the usage of the response has "output_tokens" set to <output tokens>

    Examples:
      | reply       | output tokens |
      | abcdefghéü  | 4             |
      | abcdefghiéü | 5             |

  @FR-025
  Scenario: The output tokens include the tool call arguments
    Given the request has the client tool "read_file"
    And the model calls "read_file" with the 8 ASCII characters {"a":12} as arguments and sends no text
    When an API client sends that request to "POST /v1/chat/completions"
    Then the usage of the response has "completion_tokens" set to 2

  @FR-025
  Scenario: The total tokens are the sum of the input tokens and the output tokens
    Given the prompt turn has the last usage update with "used" 5183
    And the official runtime replies "abcdefghéü" to the next prompt
    When an API client sends a valid request to "POST /v1/chat/completions"
    Then the usage of the response has "total_tokens" set to 5187

  @FR-025
  Scenario Outline: No response has a cache count or a reasoning count other than 0
    Given the prompt turn has the last usage update with "used" 5183
    When an API client sends a valid request to "<endpoint>"
    Then each cache token field and each reasoning token field in the usage is absent or 0

    Examples:
      | endpoint                  |
      | POST /v1/messages         |
      | POST /v1/responses        |
      | POST /v1/chat/completions |

  @FR-025
  Scenario: A prompt turn with no usage update gives the local estimate of the input tokens
    Given the prompt turn ends with no usage update
    When an API client sends a valid request with a stream to "POST /v1/messages"
    Then the "input_tokens" of the last "message_delta" event are the "input_tokens" of the "message_start" event

  @FR-025
  Scenario: An Anthropic stream carries a local estimate in message_start and the values in the last message_delta
    Given the prompt turn has the last usage update with "used" 5183
    And the official runtime replies "abcdefghéü" to the next prompt
    When an API client sends a valid request with a stream to "POST /v1/messages"
    Then the "message_start" event carries "input_tokens"
    And the last "message_delta" event carries "input_tokens" 5183 and "output_tokens" 4

  @FR-025
  Scenario: A Responses stream carries the usage in response.completed
    Given the prompt turn has the last usage update with "used" 5183
    When an API client sends a valid request with a stream to "POST /v1/responses"
    Then the "response.completed" event carries the usage with "input_tokens" 5183

  @FR-025
  Scenario Outline: A Chat Completions stream carries the usage only with include_usage
    Given the prompt turn has the last usage update with "used" 5183
    When an API client sends a valid request with a stream to "POST /v1/chat/completions" with "stream_options.include_usage" set to <include usage>
    Then <result>

    Examples:
      | include usage | result                                                               |
      | true          | the last chunk before "data: [DONE]" carries the usage with "prompt_tokens" 5183 |
      | false         | no chunk of the stream carries the usage                             |
