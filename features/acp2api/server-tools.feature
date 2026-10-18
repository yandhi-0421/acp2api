# language: en
Feature: Server tools
  A server tool runs on the servers of the provider of an API, and acp2api has no such servers.
  acp2api refuses each server tool and each unknown tool type with a message that the API client can use.

  Background:
    Given acp2api is in the normal state
    And the official runtime offers the model name "gemini-3.8-flash"

  @FR-024
  Scenario: An advisor tool gets the Input tag message in the error envelope of the Anthropic Messages API
    When an API client sends a request to "POST /v1/messages" with a tool of the type "advisor_20260301"
    Then acp2api refuses the request with HTTP 400 in the error envelope of the Anthropic Messages API
    And the message is "Input tag 'advisor_20260301' found using 'type' does not match any of the expected tags"
    And the log has a line for the advisor tool
    And the official runtime gets no prompt

  @FR-024
  Scenario: Another server tool of the Anthropic Messages API gets a capability_rejected message
    When an API client sends a request to "POST /v1/messages" with a tool of the type "web_fetch_20250910" and the name "web_fetch"
    Then acp2api refuses the request with HTTP 400 and the message "capability_rejected: server tool web_fetch_20250910 (name web_fetch) is not supported by acp2api."

  @FR-024
  Scenario: The web search tool of the Anthropic Messages API gets a sentence about the Claude Code settings
    When an API client sends a request to "POST /v1/messages" with a tool of the type "web_search_20250305" and the name "web_search"
    Then acp2api refuses the request with HTTP 400
    And the message starts with "capability_rejected: server tool web_search_20250305 (name web_search) is not supported by acp2api."
    And the message ends with "To stop these calls, deny the WebSearch tool in the Claude Code settings."

  @FR-024
  Scenario: The web_search tool of Codex gets message M-11
    When Codex sends a request to "POST /v1/responses" with the tool "web_search"
    Then acp2api refuses the request with HTTP 400 and the message "acp2api: the server tool web_search is not supported. To stop these calls, set web_search = "disabled" in the Codex configuration."

  @FR-024
  Scenario Outline: A tool with a type that acp2api does not know gets HTTP 400 on each API
    When an API client sends a request to "<endpoint>" with a tool of the type "example_type" and the name "example_tool"
    Then acp2api refuses the request with HTTP 400 and the message "capability_rejected: tool type example_type (name example_tool) is not supported by acp2api."

    Examples:
      | endpoint                  |
      | POST /v1/messages         |
      | POST /v1/responses        |
      | POST /v1/chat/completions |

  @FR-024
  Scenario: acp2api ignores a tool_addition block with a tool_reference to a tool of the request
    When an API client sends a request to "POST /v1/messages" with the client tool "read_file" and a tool_addition block whose tool is a tool_reference to "read_file"
    Then acp2api answers with HTTP 200
    And the official runtime gets the same content as for the same request without the tool_addition block

  @FR-024
  Scenario Outline: acp2api rejects each other form of a tool_addition block
    When an API client sends a request to "POST /v1/messages" with the client tool "read_file" and a tool_addition block <form>
    Then acp2api refuses the request with HTTP 400 and the message "acp2api: the content type tool_addition is not supported."

    Examples:
      | form                                                  |
      | whose tool is a tool_reference to "list_files"        |
      | whose tool is a full tool definition of "list_files"  |

  @FR-024
  Scenario: An advisor tool gets the Input tag message also with a tool_addition block
    When an API client sends a request to "POST /v1/messages" with a tool of the type "advisor_20260301" and a tool_addition block whose tool is a full tool definition
    Then acp2api refuses the request with HTTP 400 in the error envelope of the Anthropic Messages API
    And the message is "Input tag 'advisor_20260301' found using 'type' does not match any of the expected tags"
