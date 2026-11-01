# language: en
Feature: Management panel
  The management panel is the only place for each action of the owner.
  It is one long page of tiles in Chinese and English, and it follows the visual rules of the PRD.
  In a quoted panel text, a value in angle brackets is a value that the management panel fills in.

  @FR-053 @issue-75 @wip
  Scenario Outline: The management panel offers the entry of each action of the owner
    Given the browser language is "en-US"
    And <state>
    When the owner opens the management panel with a panel session
    Then the management panel has the entry "<entry>"

    Examples:
      | state                                                             | entry                                         |
      | acp2api is in the state "sign-in required"                        | Sign in with Google                           |
      | the owner started the sign-in                                     | Cancel the sign-in                            |
      | acp2api is in the normal state                                    | Search again                                  |
      | acp2api is in the normal state                                    | Set the path of agy_acp_server                |
      | acp2api is in the state "runtime unavailable"                     | Retry                                         |
      | acp2api is in lockout                                             | End the lockout with the management password  |
      | acp2api is in the normal state                                    | Make a new key                                |
      | acp2api is in the normal state                                    | Change the management password                |
      | acp2api is in the normal state                                    | Refresh the quota                             |
      | acp2api has not read the model names                              | Read the model names now                      |
      | the management panel has a notice                                 | Dismiss                                       |

  @FR-053 @issue-194 @wip
  Scenario: The management panel offers the compatibility check of a found build
    Given the browser language is "en-US"
    And acp2api found the build 1.6.0, which is not an accepted version
    When the owner opens the management panel with a panel session
    Then the management panel has the entry "Run the compatibility check for 1.6.0"

  @FR-053 @issue-75 @wip
  Scenario Outline: No CLI command does an action of the owner
    Given <state>
    When the owner runs "acp2api <command>" in another terminal
    Then <result>

    Examples:
      | state                                       | command | result                                   |
      | acp2api is in lockout                       | unlock  | acp2api is still in the state "lockout"  |
      | acp2api is in the normal state              | stop    | the first acp2api still answers requests |

  @FR-054 @issue-75 @wip
  Scenario Outline: The page has its parts in the order of variant A of the prototype
    Given <case>
    When the owner opens the management panel with a panel session
    Then the page has these parts in this order:
      | part                                                                          |
      | a black global bar with the language switch and the exit of the panel         |
      | a frosted bar with the anchors of the sections and the main action of the state |
      | the first tile                                                                |
      | the tile "official runtime"                                                   |
      | the tile "quota"                                                              |
      | the tile "setup"                                                              |
      | the tile "logs"                                                               |
      | the tile "settings"                                                           |
      | a footer with the version of acp2api                                          |

    Examples:
      | case                                                           |
      | acp2api is in the normal state                                 |
      | acp2api is in the normal state, and acp2api found a new build  |
      | acp2api is in the state "sign-in required"                     |
      | acp2api is in the state "quota wait"                           |
      | acp2api is in the state "runtime unavailable"                  |
      | acp2api is in lockout                                          |

  @FR-054 @issue-75 @wip
  Scenario Outline: The first tile shows the state, its sentence, and its shade
    Given the browser language is "en-US"
    And acp2api is in <state>
    When the owner opens the management panel with a panel session
    Then the first tile shows "<state text>" in large type
    And the first tile shows the sentence "<sentence>"
    And the first tile is <shade>

    Examples:
      | state                                                  | state text          | sentence                                                                                                                         | shade |
      | the normal state                                       | Running             | The official runtime is ready. acp2api forwards the requests.                                                                    | light |
      | the state "quota wait"                                 | Quota wait          | The Google quota is used up. Until <reset time>, acp2api starts no prompt turn and refuses each request. The quota wait then ends automatically. | light |
      | the state "sign-in required"                           | Sign-in required    | The official runtime has no valid sign-in. Until a sign-in, acp2api refuses each request.                                       | dark  |
      | the state "runtime unavailable" with no executable     | Runtime unavailable | acp2api found no official runtime, so it started no runtime process.                                                             | dark  |
      | the state "runtime unavailable" with no accepted version | Runtime unavailable | No build that acp2api found is an accepted version, so it started no runtime process.                                          | dark  |
      | the state "runtime unavailable" after 3 failures       | Runtime unavailable | The runtime process failed 3 times in 10 minutes, so acp2api does not start it again by itself.                                  | dark  |
      | lockout                                                | Lockout             | At <time>, the official runtime gave a response that shows a risk to the Google account. Since then, acp2api sends no request to the official runtime. | dark |

  @FR-054 @issue-75 @wip
  Scenario Outline: The first tile shows the action of a state that stops requests
    Given the browser language is "en-US"
    And acp2api is in the state "<state>"
    When the owner opens the management panel with a panel session
    Then the first tile has the action "<action>"

    Examples:
      | state               | action                                       |
      | sign-in required    | Sign in with Google                          |
      | runtime unavailable | Retry                                        |
      | lockout             | End the lockout with the management password |

  @FR-054 @issue-69
  Scenario: The first tile shows the three figures
    Given the concurrency limit is 2
    And one prompt turn runs, and no request waits in the queue
    And the build in use is 1.4.0
    When the owner opens the management panel with a panel session
    Then the first tile shows 1 place in use, 0 requests in the queue, and the build in use 1.4.0

  @FR-054 @issue-75 @wip
  Scenario: In a lockout, the first tile shows the evidence
    Given acp2api is in lockout after the stderr error line 403 "PERMISSION_DENIED"
    When the owner opens the management panel with a panel session
    Then the first tile shows the signal, the time, and the original error text of the lockout

  @FR-054 @issue-75 @wip
  Scenario Outline: The band of notices shows only when a notice exists
    Given <notices>
    When the owner opens the management panel with a panel session
    Then the page <band> the band of notices

    Examples:
      | notices                                                    | band       |
      | no notice exists                                           | has no     |
      | a built-in tool ran with no permission request             | has        |

  @FR-054 @issue-69
  Scenario: The tile official runtime shows the build in use and the result of the search
    Given the runtime process runs from the copy of the build 1.4.0 from the install folder of Zed
    And acp2api found the accepted build 1.5.0
    When the owner opens the management panel with a panel session
    Then the tile "official runtime" shows the build 1.4.0 with its label "On the release list" and its source, the install folder of Zed
    And the tile "official runtime" shows the runtime process, the result of the search, and "New build found: 1.5.0"

  @FR-054 @issue-69
  Scenario: The tile quota shows the last result of /usage, the places, and the queue
    Given the owner refreshed the quota at a known time
    When the owner opens the management panel with a panel session
    Then the tile "quota" shows the last result of "/usage" and its time
    And the tile "quota" shows the refresh entry and the concurrency limit

  @FR-054 @issue-74
  Scenario: The tile logs shows the last 50 requests from memory
    Given acp2api answered 51 requests since its start
    When the owner opens the management panel with a panel session
    Then the tile "logs" shows 50 requests with their metadata
    And the tile "logs" does not show the first request

  @FR-054 @issue-74
  Scenario: The tile logs shows a table of the unknown field names since the start
    Given an API client sent 2 requests to "POST /v1/chat/completions" with the top-level field "example_field"
    When the owner opens the management panel with a panel session
    Then the table of the unknown field names has the row "example_field" with the API "OpenAI Chat Completions API", the count 2, and the time of the last request
    And the tile "logs" shows the log folder

  @FR-054 @issue-69
  Scenario: The tile settings shows the read-only port and data folder
    Given acp2api runs on the port 8329 with the data folder "D"
    When the owner opens the management panel with a panel session
    Then the tile "settings" shows the port 8329 and the data folder "D"
    And the tile "settings" has no entry that changes the port or the data folder

  @FR-054 @issue-74
  Scenario: Before the first read of the model names, an entry reads them now
    Given acp2api has not read the model names
    When the owner uses the entry "Read the model names now"
    Then the read holds one place and makes one ACP session
    And the tile "setup" shows the model names

  @FR-054 @issue-74
  Scenario: The setup text holds the values for each API client and a placeholder for the local API key
    Given the address of acp2api is "http://127.0.0.1:8329"
    And the official runtime reported the context size 1,048,576 for "gemini-3.8-flash"
    When the owner opens the management panel with a panel session
    Then the setup text for Claude Code has "ANTHROPIC_BASE_URL" set to "http://127.0.0.1:8329"
    And the setup text for Claude Code has a model name in "ANTHROPIC_MODEL", in each "ANTHROPIC_DEFAULT_*_MODEL", and in "CLAUDE_CODE_SUBAGENT_MODEL"
    And the setup text for Claude Code has "CLAUDE_CODE_MAX_CONTEXT_TOKENS" set to 1048576
    And the setup text for Codex has a model name in "model", "model_context_window" set to 1048576, the base URL "http://127.0.0.1:8329/v1", and "wire_api = "responses""
    And the setup text for a Chat Completions client has the base URL "http://127.0.0.1:8329/v1"
    And the setup text has a placeholder in each place of the local API key, also in "ANTHROPIC_AUTH_TOKEN"
    And the setup text does not hold the local API key

  @FR-054 @issue-74
  Scenario: Before the official runtime reports a context size, the setup text has a placeholder for the window
    Given the official runtime reported no context size
    When the owner opens the management panel with a panel session
    Then "CLAUDE_CODE_MAX_CONTEXT_TOKENS" and "model_context_window" in the setup text hold a placeholder

  @FR-054 @issue-69
  Scenario: No page of the management panel holds the email address of the Google account
    Given the official runtime is signed in with the Google account "owner@example.com"
    When the owner opens each page of the management panel with a panel session
    Then no page holds "owner@example.com"

  @FR-054 @issue-69
  Scenario: A view of the management panel sends no request to the official runtime
    Given acp2api is in the normal state
    When the owner opens the management panel with a panel session and reloads it 3 times
    Then the official runtime gets no call

  @FR-054 @issue-53
  Scenario: The README lists each request field that acp2api ignores
    Given the README of acp2api
    Then the README lists these request fields as fields that acp2api ignores:
      | field                                   |
      | temperature                             |
      | top_p                                   |
      | top_k                                   |
      | presence_penalty                        |
      | frequency_penalty                       |
      | seed                                    |
      | logit_bias                              |
      | max_tokens                              |
      | max_completion_tokens                   |
      | max_output_tokens                       |
      | cache_control                           |
      | prompt_cache_key                        |
      | service_tier                            |
      | store with each value                   |
      | truncation                              |
      | prediction                              |
      | metadata.user_id                        |
      | user                                    |
      | safety_identifier                       |
      | client_metadata                         |
      | context_management                      |
      | output_config.task_budget               |
      | strict                                  |
      | defer_loading                           |
      | eager_input_streaming                   |
      | parallel_tool_calls                     |
      | max_tool_calls                          |
      | verbosity                               |
      | text.verbosity                          |
      | response_format with the type json_object |
      | include: ["reasoning.encrypted_content"] |
      | reasoning.summary                       |
      | thinking.display                        |

  @FR-055 @issue-72
  Scenario Outline: The language of the browser selects the default language
    Given the browser language is "<browser language>"
    And acp2api is in the normal state
    When the owner opens the management panel with a panel session
    Then the first tile shows "<state text>"

    Examples:
      | browser language | state text |
      | zh-CN            | 运行中     |
      | en-US            | Running    |
      | fr-FR            | Running    |

  @FR-055 @issue-72
  Scenario: The language switch changes the language
    Given the browser language is "en-US"
    And the owner opened the management panel
    When the owner selects Chinese with the language switch
    Then the first tile shows "运行中"
    And the sections are "官方运行时、配额、名额与队列、接入、日志、设置、通知"

  @FR-055 @issue-72
  Scenario: A load of the page gives requests only to acp2api
    When the owner opens the management panel on the port 8329
    Then each request of the browser goes to "127.0.0.1:8329"
    And the page loads no font from an external address

  @FR-055 @issue-72
  Scenario: Each control uses the color #0066cc
    When the owner opens the management panel with a panel session
    Then each control of the page has the color "#0066cc"

  @FR-055 @issue-72
  Scenario: The styles of the page have no shadow and no web font
    When the owner opens the management panel with a panel session
    Then no element of the page has a shadow
    And the page uses only the system fonts

  @FR-055 @issue-72
  Scenario: Each status color is only on a dot of 9 px next to its text
    Given acp2api is in the state "quota wait"
    When the owner opens the management panel with a panel session
    Then each element with one of the status colors "#0ca30c", "#fab219", "#ec835a", and "#d03b3b" is a dot of 9 px with text next to it
    And no control and no background has a status color

  @FR-055 @issue-72
  Scenario: v1 has only the light mode
    Given the browser prefers the dark color scheme
    When the owner opens the management panel with a panel session
    Then the page has the same colors as with the light color scheme
