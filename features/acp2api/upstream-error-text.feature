# language: en
Feature: No text of an upstream error as reply text
  The official runtime sends the text of an upstream error as normal reply text.
  acp2api holds the newest text chunk of each prompt turn until the next update.
  Thus no such text reaches the API client as reply text.

  Background:
    Given acp2api is in the normal state
    And the official runtime offers the model name "gemini-3.8-flash"

  @FR-028
  Scenario Outline: An error line drops the held chunk and ends the response with the error
    Given the official runtime sends the text chunks "Sure, " and "503 The service is unavailable."
    And the prompt turn ends
    And the error line of the ACP session comes <time of the error line>
    When an API client sends a valid request with a stream to "POST /v1/messages"
    Then the stream has the text "Sure, "
    And the stream has no text "503 The service is unavailable."
    And the stream ends with an "error" event

    Examples:
      | time of the error line                  |
      | before the end of the prompt turn       |
      | 0 ms after the end of the prompt turn   |
      | 150 ms after the end of the prompt turn |

  @FR-028
  Scenario: With no error line, the held chunk reaches the API client
    Given the official runtime sends the text chunks "Sure, " and "here it is."
    And the prompt turn ends, and no error line comes
    When an API client sends a valid request to "POST /v1/chat/completions"
    Then the reply text of the response is "Sure, here it is."

  @FR-028
  Scenario: An error line after the wait stops the ACP session and gives a log line
    Given an API client got the reply "Done." from a prompt turn in the ACP session "S1"
    And the error line of that ACP session came 500 ms after the end of the prompt turn
    When the API client sends the next request of that conversation
    Then the official runtime gets a new ACP session for that request
    And the log has a line about the late error line
