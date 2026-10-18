# language: en
Feature: Client tools
  The model gets the client tools of the request and no built-in tool.
  A tool call ends the response. The tool result goes to the model as text in the next prompt.

  Background:
    Given acp2api is in the normal state
    And the official runtime offers the model name "gemini-3.8-flash"

  @FR-022
  Scenario: The tools that the official runtime offers to the model are only the client tools of the request
    When an API client sends a request to "POST /v1/messages" with the client tools "read_file" and "run_command", each with a description and a schema
    Then the official runtime offers the model exactly the tools "read_file" and "run_command"
    And each tool has the description and the schema of the request

  @FR-022
  Scenario Outline: A model call of a client tool ends the response with that tool call
    Given the request has the client tool "read_file"
    And the model sends the text "Let me look." and then calls "read_file" with the arguments {"path":"a.txt"}
    When an API client sends that request to "<endpoint>"
    Then the response holds the text "Let me look." and then <tool call>
    And the response ends with <end>

    Examples:
      | endpoint                  | tool call                                | end                               |
      | POST /v1/messages         | a "tool_use" block for "read_file"       | "stop_reason" set to "tool_use"   |
      | POST /v1/chat/completions | "tool_calls" with "read_file"            | "finish_reason" set to "tool_calls" |
      | POST /v1/responses        | a "function_call" item for "read_file"   | the "function_call" item as the last output item |

  @FR-022
  Scenario: A response has no more than one tool call
    Given the request has the client tools "read_file" and "list_files"
    And the model calls "read_file" and "list_files" in one prompt turn
    When an API client sends that request to "POST /v1/messages"
    Then the response has exactly one "tool_use" block

  @FR-022
  Scenario: No prompt turn waits for a tool result
    Given the concurrency limit is 1
    And a request of the conversation "C1" got a response with a tool call
    When a request of another conversation "C2" comes
    Then the prompt turn of "C2" starts at once
    And the official runtime got a cancel of the prompt turn of "C1"

  @FR-022
  Scenario: A custom tool of Codex goes to the model as a tool with one string argument
    When Codex sends a request to "POST /v1/responses" with the custom tool "apply_patch" with a grammar
    Then the official runtime offers the model the tool "apply_patch" with one string argument "input"
    And the description of that tool holds the grammar

  @FR-022
  Scenario: A model call of a custom tool gives a custom_tool_call item
    Given Codex sent a request with the custom tool "apply_patch"
    When the model calls "apply_patch" with the input "*** Begin Patch"
    Then the response has a "custom_tool_call" item for "apply_patch" with the input "*** Begin Patch"

  @FR-022
  Scenario: Each tool of a namespace tool goes to the model as one tool
    When Codex sends a request to "POST /v1/responses" with the namespace tool "files" that holds the tools "read" and "write"
    Then the official runtime offers the model two tools, one for "read" and one for "write"

  @FR-022
  Scenario: acp2api returns the namespace in the function_call
    Given Codex sent a request with the namespace tool "files" that holds the tool "read"
    When the model calls the tool for "read"
    Then the response has a "function_call" item for "read" with the namespace "files"

  @FR-022
  Scenario: tool_choice none gives a response with no tool call
    Given the request has the client tool "read_file"
    And the model calls "read_file" when the official runtime offers it
    When an API client sends that request to "POST /v1/chat/completions" with "tool_choice" set to "none"
    Then the response has no tool call

  @FR-022
  Scenario Outline: acp2api rejects a tool that it cannot offer
    When an API client sends a request to "<endpoint>" with <tool>
    Then acp2api refuses the request with HTTP 400 and the message "<message>"
    And the official runtime gets no prompt

    Examples:
      | endpoint           | tool                                         | message                                                                                                   |
      | POST /v1/responses | a "tool_search" tool                         | acp2api: the tool tool_search is not supported.                                                           |
      | POST /v1/messages  | a client tool with the name "list_resources" | acp2api: the client tool name list_resources is not supported, because a built-in tool of the official runtime has this name. |
      | POST /v1/messages  | a client tool with the name "read_resource"  | acp2api: the client tool name read_resource is not supported, because a built-in tool of the official runtime has this name.  |

  @FR-023
  Scenario: The model gets a tool result as text in the next prompt
    Given the model called "read_file" with the arguments {"path":"a.txt"} in a request to "POST /v1/chat/completions"
    When the API client sends the tool result "abc" for that tool call
    Then the prompt that the official runtime gets has these lines in this order:
      | line                                                 |
      | Result of your tool call read_file {"path":"a.txt"}: |
      | abc                                                  |

  @FR-023
  Scenario: An error result gets a note after the arguments
    Given the model called "read_file" with the arguments {"path":"a.txt"} in a request to "POST /v1/messages"
    When the API client sends an error tool result "No such file" for that tool call
    Then the first line of the tool result in the prompt is:
      """
      Result of your tool call read_file {"path":"a.txt"} (the tool reported an error):
      """

  @FR-023
  Scenario: Each image of a tool result is at its place
    Given the model called "screenshot" in a request to "POST /v1/messages"
    When the API client sends a tool result with the text "Before:", an image, and the text "After."
    Then the prompt that the official runtime gets has the text "Before:", the image, and the text "After." in this order

  @FR-023
  Scenario Outline: The first line holds no more than 1,000 characters of the arguments
    Given the model called "write_file" with arguments of <length> characters in a request to "POST /v1/chat/completions"
    When the API client sends the tool result "done" for that tool call
    Then the first line of the tool result in the prompt holds <part>

    Examples:
      | length | part                                                              |
      | 1,000  | all 1,000 characters of the arguments and no mark                 |
      | 1,001  | only the first 1,000 characters and a mark that the arguments are cut |
      | 5,000  | only the first 1,000 characters and a mark that the arguments are cut |

  @FR-023
  Scenario: A tool result with an unknown ID gets the first line with its ID
    Given the conversation has no tool call with the ID "call_9"
    When an API client sends a request to "POST /v1/chat/completions" with a tool result for "call_9" with the text "abc"
    Then the first line of the tool result in the prompt is "Result of the tool call call_9:"

  @FR-023
  Scenario: The text of a rebuild holds an earlier tool call as a sentence with the full arguments
    Given an earlier tool call of the conversation called "write_file" with arguments of 5,000 characters
    When the API client sends a request of that conversation that does a rebuild
    Then the first prompt of the new ACP session holds a sentence that starts with "You called the tool write_file with the arguments " and ends with "."
    And the arguments in that sentence have all 5,000 characters

  @FR-023
  Scenario: A tool result has the same form in a continued ACP session and in the text of a rebuild
    Given the model called "read_file" with the arguments {"path":"a.txt"}, and the tool result "abc" went into the continued ACP session
    When the API client sends a request of that conversation that does a rebuild
    Then the first prompt of the new ACP session has these lines in this order:
      | line                                                 |
      | Result of your tool call read_file {"path":"a.txt"}: |
      | abc                                                  |
