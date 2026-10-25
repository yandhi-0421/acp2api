# language: en
Feature: System prompt and role labels
  The model gets the system prompt as an addition to the system text of the official runtime.
  Each other item of a prompt gets its role label or its sentence, except when the new items are one user message.

  Background:
    Given acp2api is in the normal state
    And the official runtime offers the model name "gemini-3.8-flash"

  @FR-019 @issue-41
  Scenario: The model gets the system prompt as an addition to the system text, not as user text
    When an API client sends a request to "POST /v1/messages" with the system prompt "Answer in French." and one user message "Hello"
    Then the ACP session gets the system prompt addition "Answer in French."
    And the prompt that the official runtime gets is "Hello"

  @FR-019 @issue-41
  Scenario Outline: acp2api joins the messages of the system prompt with one blank line
    When an API client sends a request to "<endpoint>" with <parts> and then one user message "Hello"
    Then the ACP session gets this system prompt addition:
      """
      Part one.

      Part two.
      """

    Examples:
      | endpoint                  | parts                                                                    |
      | POST /v1/chat/completions | the system message "Part one." and the developer message "Part two."     |
      | POST /v1/responses        | the instructions "Part one." and the developer message "Part two."       |

  @FR-019 @issue-41
  Scenario: The text blocks of one system message have no added separator
    When an API client sends a request to "POST /v1/messages" with the system blocks "Part one." and "Part two." and then one user message "Hello"
    Then the ACP session gets the system prompt addition "Part one.Part two."

  @FR-019 @issue-41
  Scenario Outline: A named system message gets one role label before all its text blocks
    When an API client sends a request to "<endpoint>" with the system message named "example_user" and these text blocks:
      | text      |
      | Part one. |
      | Part two. |
    Then the ACP session gets this system prompt addition:
      """
      [system: example_user]
      Part one.Part two.
      """

    Examples:
      | endpoint                  |
      | POST /v1/chat/completions |
      | POST /v1/responses        |

  @FR-019 @issue-41
  Scenario: acp2api removes the attribution block of Claude Code
    When Claude Code sends a request to "POST /v1/messages" whose first system block is the attribution block of Claude Code, and whose second system block is "You are Claude Code."
    Then the ACP session gets the system prompt addition "You are Claude Code."
    And no content that the official runtime gets holds the text of the attribution block

  @FR-019 @FR-020 @issue-47
  Scenario: A system message after a user message stays at its place with its role label
    Given an API client sent a request to "POST /v1/chat/completions" with the user message "Hello", and the official runtime replied "Hi."
    When the API client sends the next request of that conversation with the user message "Tell me more." and then the system message "Be brief."
    Then the prompt that the official runtime gets is:
      """
      [user]
      Tell me more.
      [system]
      Be brief.
      """
    And the system prompt addition of the ACP session does not hold "Be brief."

  @FR-020 @issue-36
  Scenario: A prompt with one new unnamed user message has no role label
    When an API client sends a request to "POST /v1/messages" with one unnamed user message "Hello"
    Then the prompt that the official runtime gets is "Hello"

  @FR-020 @issue-47
  Scenario: Two user messages with names keep their role labels
    Given an API client sent a request to "POST /v1/chat/completions" with the user message "Hello", and the official runtime replied "Hi."
    When the API client sends the next request of that conversation with the user messages "I agree." with the name "Alice" and "Me too." with the name "Bob"
    Then the prompt that the official runtime gets is:
      """
      [user: Alice]
      I agree.
      [user: Bob]
      Me too.
      """

  @FR-020 @issue-47
  Scenario Outline: One new user message with a name keeps its role label
    Given <history>
    When an API client sends the only new user message "Hello" with the name "Alice" to "POST /v1/chat/completions"
    Then the prompt that the official runtime gets is:
      """
      [user: Alice]
      Hello
      """

    Examples:
      | history                                                    |
      | the request has no earlier conversation                    |
      | the request continues the last reply of an ACP session     |

  @FR-020 @issue-54 @wip
  Scenario: A tool result gets a sentence and a system message gets a role label
    Given an API client sent a request to "POST /v1/chat/completions" with the client tool "read_file", and the model called "read_file" with the arguments {"path":"a.txt"}
    When the API client sends the next request of that conversation with the tool result "abc" and then the system message "Be brief."
    Then the prompt that the official runtime gets is:
      """
      Result of your tool call read_file {"path":"a.txt"}:
      abc
      [system]
      Be brief.
      """

  @FR-020 @issue-47
  Scenario: The text of a rebuild keeps the name of each message of each role
    When an API client sends a new conversation to "POST /v1/chat/completions" with these messages:
      | role      | name  | text         |
      | user      | Alice | Hello.       |
      | assistant | Bob   | Hi, Alice.   |
      | developer |       | Stay polite. |
      | user      | Alice | Thanks.      |
    Then the first prompt of the new ACP session is:
      """
      The text below is the earlier part of this conversation. Continue the conversation as the assistant from its last item.

      [user: Alice]
      Hello.
      [assistant: Bob]
      Hi, Alice.
      [developer]
      Stay polite.
      [user: Alice]
      Thanks.
      """
