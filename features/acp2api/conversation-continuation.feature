# language: en
Feature: The conversation in each prompt
  The model gets the conversation of each request one time, with no item that the API client did not send.
  A request that continues the conversation of an ACP session sends only its new items. Else acp2api does a rebuild.

  Background:
    Given acp2api is in the normal state
    And the official runtime offers the model name "gemini-3.8-flash"
    And an API client sent a request to "POST /v1/chat/completions" with the system prompt "Be kind." and the user message "Hello"
    And the official runtime replied "Hi." in the ACP session "S1"

  @FR-021 @issue-47 @wip
  Scenario: A request that adds one user message continues the ACP session
    When the API client sends the same conversation with the reply "Hi." and the new user message "How are you?"
    Then the official runtime gets the prompt "How are you?" in the ACP session "S1"
    And the official runtime gets no new ACP session

  @FR-021 @issue-47 @wip
  Scenario: A request with an edited earlier message does a rebuild
    When the API client sends the conversation with the user message "Hello there", the reply "Hi.", and the new user message "How are you?"
    Then the official runtime gets a new ACP session
    And the first prompt of the new ACP session is:
      """
      The text below is the earlier part of this conversation. Continue the conversation as the assistant from its last item.

      [user]
      Hello there
      [assistant]
      Hi.
      [user]
      How are you?
      """
    And the new ACP session gets the system prompt addition "Be kind."

  @FR-021 @issue-47 @wip
  Scenario: An image of the earlier part of a conversation is at its place in a rebuild
    Given an API client sent a conversation whose user message has the text "See this:" and an image, and the official runtime replied "A cat."
    When the API client sends that conversation with the reply "A dog." in place of "A cat.", and the new user message "Sure?"
    Then the first prompt of the new ACP session has the text "See this:", the image, and the line "[assistant]" in this order

  @FR-021 @issue-47 @wip
  Scenario: A new conversation with no earlier part is not a rebuild
    When an API client sends a new conversation with only the user message "Good morning"
    Then the official runtime gets a new ACP session
    And the first prompt of the new ACP session is "Good morning"
    And the log has no rebuild line

  @FR-021 @issue-66 @wip
  Scenario Outline: The next request after an event that stops an ACP session does a rebuild
    Given the next prompt turn in the ACP session "S1" ends with <event>
    And 30 s pass after that end
    When the API client sends the next request of that conversation
    Then the official runtime gets a new ACP session
    And the first prompt of the new ACP session starts with the neutral line

    Examples:
      | event                                   |
      | a cut at a stop sequence                |
      | a disconnect of the API client          |
      | the stderr error line 503 "UNAVAILABLE" |
      | the stop reason "refusal", and "/usage" shows quota for the model |

  @FR-021 @issue-51 @wip
  Scenario: The text of a rebuild after a cut holds the reply only up to the cut
    Given the API client sent the user message "Count to 3." with the stop sequence "3"
    And the official runtime replied "1, 2, 3, 4", so acp2api cut the reply to "1, 2, "
    When the API client sends that conversation with the reply "1, 2, " and the new user message "Go on."
    Then the first prompt of the new ACP session holds the reply "1, 2, "
    And that prompt holds no text "3, 4"

  @FR-021 @issue-47 @wip
  Scenario: After a restart of acp2api the next request of a conversation does a rebuild
    Given the owner restarted acp2api
    When the API client sends the same conversation with the reply "Hi." and the new user message "How are you?"
    Then the official runtime gets a new ACP session
    And the first prompt of the new ACP session starts with the neutral line

  @FR-021 @issue-47 @wip
  Scenario: acp2api uses the content of the conversation, not a session hint
    Given the first request had the header "x-claude-code-session-id: abc"
    When the API client sends the conversation with the edited user message "Hello there" and the header "x-claude-code-session-id: abc"
    Then the official runtime gets a new ACP session
    And the first prompt of the new ACP session starts with the neutral line

  @FR-021 @issue-47 @wip
  Scenario: A changed session hint does not stop a continuation
    Given the first request had "prompt_cache_key" set to "key-1"
    When the API client sends the same conversation with the reply "Hi.", the new user message "How are you?", and "prompt_cache_key" set to "key-2"
    Then the official runtime gets the prompt "How are you?" in the ACP session "S1"

  @FR-021 @issue-47 @wip
  Scenario: acp2api tells the API client nothing about a rebuild and writes a log line
    Given the official runtime replies "Fine." to the next prompt
    When the API client sends the conversation with the user message "Hello there", the reply "Hi.", and the new user message "How are you?"
    Then the reply text of the response is "Fine."
    And the log has one rebuild line
