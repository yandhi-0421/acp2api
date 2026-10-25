# language: en
Feature: No text of an upstream error as reply text
  The official runtime sends the text of an upstream error as normal reply text.
  acp2api holds the newest reply text chunk until the next reply text chunk.
  Thus no such text reaches the API client as reply text.

  Background:
    Given acp2api is in the normal state
    And the official runtime offers the model name "gemini-3.8-flash"

  @FR-028 @issue-50
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
      | 200 ms after the end of the prompt turn |

  @FR-028 @issue-50
  Scenario Outline: A non-reply update does not release the held error text
    Given the official runtime sends "Sure, " and then "503 The service is unavailable." as reply text chunks
    And the official runtime then sends <update>
    And the prompt turn ends
    And its stderr error line comes 150 ms after that end
    When an API client sends a valid request with a stream to "POST /v1/messages"
    Then the reply text is only "Sure, "
    And the stream ends with an "error" event

    Examples:
      | update                    |
      | a usage update            |
      | a chunk of thought text   |

  @FR-028 @issue-50
  Scenario: The wait ends early when both error lines are there
    Given the official runtime sends the text chunks "Sure, " and "503 The service is unavailable."
    And the prompt turn ends
    And 20 ms after the end, stderr has the line "Antigravity SDK error processing prompt for session" with the ID of the ACP session, and then the last line of its traceback
    When an API client sends a valid request with a stream to "POST /v1/messages"
    Then the stream ends with an "error" event less than 200 ms after the end of the prompt turn

  @FR-028 @issue-50
  Scenario: With no error line, the held chunk reaches the API client
    Given the official runtime sends the text chunks "Sure, " and "here it is."
    And the prompt turn ends, and no error line comes
    When an API client sends a valid request to "POST /v1/chat/completions"
    Then the reply text of the response is "Sure, here it is."

  @FR-028 @issue-50
  Scenario: An error line after the wait stops the ACP session and gives a log line
    Given an API client got the reply "Done." from a prompt turn in the ACP session "S1"
    And the error line of that ACP session came 201 ms after the end of the prompt turn
    When the API client sends the next request of that conversation after its retry wait
    Then the official runtime gets a new ACP session for that request
    And the log has a line about the late error line
