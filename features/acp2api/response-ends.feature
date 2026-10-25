# language: en
Feature: Ends of a response
  A response ends at a stop sequence, at a stop reason of the official runtime, or at a disconnect of the API client.

  Background:
    Given acp2api is in the normal state
    And the official runtime offers the model name "gemini-3.8-flash"

  @FR-015 @issue-51
  Scenario: A stop sequence cuts the reply of the Chat Completions API
    Given the official runtime replies "Hello STOP world" to the next prompt
    When an API client sends a valid request to "POST /v1/chat/completions" with "stop" set to ["STOP"]
    Then the reply text of the response is "Hello "
    And the response has "finish_reason" set to "stop"
    And the official runtime gets a cancel of the prompt turn

  @FR-015 @issue-51
  Scenario: A stop sequence cuts the reply of the Anthropic Messages API
    Given the official runtime replies "Hello STOP world" to the next prompt
    When an API client sends a valid request to "POST /v1/messages" with "stop_sequences" set to ["STOP"]
    Then the reply text of the response is "Hello "
    And the response has "stop_reason" set to "stop_sequence" and "stop_sequence" set to "STOP"
    And the official runtime gets a cancel of the prompt turn

  @FR-015 @issue-51
  Scenario: No stream event holds a character of the match or after it
    Given the official runtime sends the text chunks "Hello ST" and "OP world"
    When an API client sends a valid request with a stream to "POST /v1/chat/completions" with "stop" set to ["STOP"]
    Then the text of all stream events together is "Hello "
    And no stream event holds the character "S" of the match

  @FR-015 @issue-51
  Scenario: acp2api cuts the reply at the first match of any stop sequence
    Given the official runtime replies "one two three" to the next prompt
    When an API client sends a valid request to "POST /v1/messages" with "stop_sequences" set to ["three", "two"]
    Then the reply text of the response is "one "
    And the response has "stop_sequence" set to "two"

  @FR-030 @issue-51
  Scenario Outline: The stop reason max_tokens gives the end of each API
    Given the next prompt turn ends with the stop reason "max_tokens"
    When an API client sends a valid request to "<endpoint>"
    Then acp2api answers with HTTP 200 and no error
    And the response has <end>

    Examples:
      | endpoint                  | end                                                         |
      | POST /v1/messages         | "stop_reason" set to "max_tokens"                           |
      | POST /v1/chat/completions | "finish_reason" set to "length"                             |
      | POST /v1/responses        | "status" set to "incomplete" with the reason "max_output_tokens" |

  @FR-030 @issue-66 @wip
  Scenario Outline: The stop reason refusal with quota gives the end of each API
    Given the next prompt turn ends with the stop reason "refusal", and no error channel has a quota text
    And "/usage" shows quota for the model of the request
    When an API client sends a valid request to "<endpoint>"
    Then acp2api answers with HTTP 200 and no error
    And the response has <end>

    Examples:
      | endpoint                  | end                                                       |
      | POST /v1/messages         | "stop_reason" set to "refusal"                            |
      | POST /v1/chat/completions | "finish_reason" set to "content_filter"                   |
      | POST /v1/responses        | "status" set to "incomplete" with the reason "content_filter" |

  @FR-031 @issue-51
  Scenario: A disconnect during a prompt turn cancels the prompt turn at once
    Given an API client sent a valid request, and its prompt turn runs
    When the API client disconnects
    Then the official runtime gets a cancel of the prompt turn at once

  @FR-031 @issue-60
  Scenario: A request that waits in the queue leaves the queue at its disconnect
    Given each place holds a prompt turn
    And 2 requests wait in the queue
    When the API client of the first request in the queue disconnects
    Then the count of the requests in the queue is 1
    And the official runtime never gets a prompt for the request of that API client
