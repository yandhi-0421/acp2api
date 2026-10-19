# language: en
Feature: Safety toward Google and the official runtime
  acp2api reaches Google only through the official runtime, in a runtime home of its own.
  The official runtime acts as a pure model: no built-in tool runs, and each client tool call goes back to the API client.

  @FR-046 @issue-67 @wip
  Scenario: With a fake ACP agent, acp2api opens network connections only on 127.0.0.1
    Given acp2api runs with a fake ACP agent in place of the official runtime
    When an API client sends requests, the owner signs in, and the owner starts a new search
    Then each network connection that acp2api opens goes to 127.0.0.1

  @FR-046 @issue-73 @wip
  Scenario Outline: acp2api does not change the sign-in files of the official runtime
    Given the runtime home holds the sign-in files of the official runtime
    When <event>
    Then the sign-in files of the official runtime are unchanged

    Examples:
      | event                                |
      | a lockout signal starts a lockout    |
      | the owner stops acp2api              |

  @FR-046 @issue-57 @wip
  Scenario: The runtime process uses the runtime home in the data folder
    Given the data folder of acp2api is "D"
    When acp2api starts a runtime process
    Then the runtime home of the runtime process is a folder in "D"

  @FR-046 @issue-57 @wip
  Scenario: No global configuration of the owner reaches an ACP session
    Given the folder "~/.gemini" of the owner names the MCP server "owner-mcp"
    When an API client sends a request with no client tool
    Then the official runtime offers the model no tool of "owner-mcp"

  @FR-047 @issue-57 @wip
  Scenario: acp2api declares no file system capability and no terminal capability
    When acp2api starts a runtime process
    Then the official runtime gets no file system capability and no terminal capability from acp2api

  @FR-047 @issue-40
  Scenario: The permission request of a client tool cancels the prompt turn and returns the tool call
    Given the request has the client tool "read_file"
    When the official runtime sends the permission request "Run read_file?" for that tool
    Then the official runtime gets no allow
    And the official runtime gets a cancel of the prompt turn
    And the response holds the tool call of "read_file"

  @FR-047 @issue-57 @wip
  Scenario: acp2api refuses the permission request of a built-in tool and the prompt turn continues
    Given the official runtime sends the permission request of the built-in tool "view_file" in a prompt turn
    And the official runtime then replies "I cannot read files."
    When an API client sends a valid request
    Then the official runtime gets a refusal of that permission request
    And the reply text of the response is "I cannot read files."
    And the log has a line about the refused permission request

  @FR-047 @issue-57 @wip
  Scenario: A built-in tool that runs with no permission request ends the response with M-22
    Given the built-in tool "view_file" of the official runtime runs with no permission request in the next prompt turn
    When an API client sends a valid request with "stream" set to false
    Then the official runtime gets a cancel of the prompt turn
    And acp2api answers with HTTP 502 and the message "acp2api: the built-in tool view_file of the official runtime ran with no permission request, so acp2api cancelled the prompt turn."
    And the log has one line about the built-in tool "view_file"

  @FR-047 @issue-57 @wip
  Scenario: A built-in tool that runs with no permission request gives one notice until the owner dismisses it
    Given the built-in tool "view_file" ran with no permission request in one prompt turn
    Then the management panel shows one notice "The built-in tool view_file ran with no permission request. acp2api cancelled the prompt turn and sent an error to the API client."
    And the notice is still there after a reload of the management panel
    When the owner dismisses the notice
    Then the management panel shows no notice

  @FR-047 @issue-57 @wip
  Scenario: A built-in tool that runs with no permission request starts no new state
    Given the built-in tool "view_file" ran with no permission request in one prompt turn
    When an API client sends a valid request of another conversation
    Then acp2api is in the normal state
    And the official runtime gets a prompt for the request

  @FR-047 @issue-57 @wip
  Scenario: Each ACP session has a new empty cwd folder in the system temporary folder
    When two requests of two conversations make two new ACP sessions
    Then the cwd of each ACP session is a folder in the system temporary folder
    And each cwd was empty at the start of its ACP session
    And the two cwd folders are different

  @FR-048 @issue-63 @wip
  Scenario Outline: acp2api gives the official runtime its own identity at each start
    Given the version of acp2api is "1.0.0"
    And Claude Code sends requests to acp2api
    When acp2api starts <runtime process>
    Then the official runtime gets "clientInfo" with the name "acp2api", the title "acp2api", and the version "1.0.0"

    Examples:
      | runtime process                           |
      | the first runtime process                 |
      | a runtime process that replaces another   |
