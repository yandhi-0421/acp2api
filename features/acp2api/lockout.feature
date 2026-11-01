# language: en
Feature: Lockout
  acp2api reads lockout signals only from two error channels.
  These are the error of each ACP call, and the error lines of the ACP session in the stderr of the official runtime.
  During a lockout, acp2api sends no request to the official runtime. Only the owner ends a lockout.
  In a quoted message, a value in angle brackets is a value that acp2api fills in, as the section "Message texts" of the PRD gives.

  Background:
    Given acp2api is in the normal state
    And the official runtime offers the model name "gemini-3.8-flash"

  @FR-034 @issue-70
  Scenario Outline: Each upstream error in an error channel gives the state of its row
    When a prompt turn ends with <error>
    Then acp2api is in the state "<state>"

    Examples:
      | error                                                                                         | state            |
      | the stderr error line 403 "PERMISSION_DENIED" with the reason "TOS_VIOLATION"                 | lockout          |
      | the stderr error line 403 with the reason "VALIDATION_REQUIRED"                               | lockout          |
      | an error of the ACP call with the status "PERMISSION_DENIED", the text "Verify your account", and no 403 | lockout |
      | the stderr error line "PERMISSION_DENIED" with the reason "SUBSCRIPTION_REQUIRED" and no 403 | lockout          |
      | an error of the ACP call with the text "Terms of Service" and no 403                          | lockout          |
      | an error of the ACP call with the code -32000 "Authentication required" and the text "Terms of Service" | lockout |
      | the stderr error line 401 "UNAUTHENTICATED" with a 403 in its message                         | lockout          |
      | the stderr error line 401                                                                     | sign-in required |
      | the stderr error line with the status "UNAUTHENTICATED" and no 401                            | sign-in required |
      | an error of the ACP call with the code -32000 "Authentication required"                       | sign-in required |
      | an error of the ACP call with the text "logged_out"                                           | sign-in required |
      | the stderr error line 429 "RESOURCE_EXHAUSTED"                                                | quota wait       |
      | the stderr error line 402                                                                     | quota wait       |
      | the stderr error line 400 "INVALID_ARGUMENT"                                                  | normal state     |
      | the stderr error line 503 "UNAVAILABLE"                                                       | normal state     |
      | an exit of the official runtime                                                               | normal state     |

  @FR-034 @issue-70
  Scenario: A normal reply about the Terms of Service gives no lockout
    Given the official runtime replies "Please read the Terms of Service." to the next prompt, with no error line
    When an API client sends a valid request
    Then the reply text of the response is "Please read the Terms of Service."
    And acp2api is in the normal state

  @FR-034 @issue-70
  Scenario: Reply text about the Terms of Service counts in a prompt turn that stderr marks as failed
    Given the official runtime replies "Your account violates the Terms of Service." to the next prompt
    And stderr marks the prompt turn as failed with the error line 400 "INVALID_ARGUMENT"
    When an API client sends a valid request
    Then acp2api is in the state "lockout"

  @FR-034 @issue-66
  Scenario Outline: The stop reason refusal gives one quota query
    Given the next prompt turn ends with the stop reason "refusal", and no error channel has a quota text
    And "/usage" shows <quota> for the model of the request
    When an API client sends a valid request
    Then the official runtime gets "/usage" one time
    And <result>

    Examples:
      | quota    | result                                                      |
      | quota    | the response ends with "stop_reason" set to "refusal"       |
      | no quota | acp2api is in the state "quota wait"                        |

  @FR-034 @issue-70
  Scenario: A lockout signal starts a lockout also in a quota wait
    Given a prompt turn runs
    And a quota signal then starts a quota wait
    When the prompt turn that runs ends with the stderr error line 403 "PERMISSION_DENIED"
    Then acp2api is in the state "lockout"

  @FR-035 @issue-70
  Scenario: The start of a lockout cancels each prompt turn, refuses each request in the queue, and stops the runtime process
    Given the concurrency limit is 2
    And the prompt turns of the requests "A" and "B" run
    And the request "C" waits in the queue
    When the prompt turn of "A" ends with the stderr error line 403 "PERMISSION_DENIED"
    Then the official runtime gets a cancel of the prompt turn of "B"
    And acp2api refuses "C" with the lockout message
    And no runtime process runs

  @FR-035 @issue-70
  Scenario: The response that started the lockout holds the original error text of Google
    Given the next prompt turn ends with the stderr error line 403 "PERMISSION_DENIED" with the message "Appeal at https://example.com/appeal"
    When an API client sends a valid request to "POST /v1/messages" with "stream" set to false
    Then the error body holds the message "acp2api: lockout since <start time> after <signal>; acp2api sends no request to the official runtime until the owner ends the lockout at <address of the management panel>. Original error: <original error text>"
    And the original error text in the message holds "Appeal at https://example.com/appeal"

  @FR-035 @issue-70
  Scenario: Each later request gets the lockout message and the official runtime gets no request
    Given acp2api is in lockout since a lockout signal
    When an API client sends a valid request to "POST /v1/chat/completions"
    Then acp2api refuses the request with the message "acp2api: lockout since <start time> after <signal>; acp2api sends no request to the official runtime until the owner ends the lockout at <address of the management panel>."
    And the message holds no text "Original error:"
    And the official runtime gets no request

  @FR-035 @issue-73
  Scenario: acp2api shows a lockout in the terminal and in the dark first tile of the management panel
    When a lockout signal starts a lockout
    Then the terminal shows one line with the lockout notice
    And the first tile of the management panel is dark and shows the state "Lockout"

  @FR-035 @issue-73
  Scenario: During a lockout acp2api starts no runtime process and sends no quota query
    Given acp2api is in lockout since a lockout signal
    When the owner restarts acp2api and asks for a refresh of the quota in the management panel
    Then no runtime process runs
    And the official runtime gets no "/usage"

  @FR-035 @FR-039 @issue-73
  Scenario: The management panel has no sign-in entry during a lockout
    Given acp2api is in lockout since a lockout signal
    When the owner opens the management panel
    Then the management panel has no sign-in entry

  @FR-035 @FR-056 @issue-70
  Scenario: acp2api writes the lockout and its redacted evidence into the state file
    When a prompt turn ends with the stderr error line 403 "PERMISSION_DENIED" with the message "Account name%40example.com is suspended"
    Then the state file holds the lockout with the signal, the time, and the original error text
    And the original error text in the state file holds "<REDACTED>" in place of "name%40example.com"

  @FR-035 @issue-70
  Scenario Outline: A restart of acp2api does not end a lockout
    Given <state file>
    When the owner restarts acp2api
    Then acp2api is in the state "lockout"
    And no runtime process runs

    Examples:
      | state file                                        |
      | the state file holds a lockout                    |
      | the state file is damaged                         |
      | the state file exists, and acp2api cannot read it |

  @FR-035 @issue-70
  Scenario: No timer ends a lockout
    Given acp2api is in lockout since a lockout signal
    When 30 days pass
    Then acp2api is in the state "lockout"

  @FR-035 @issue-73
  Scenario: The owner ends a lockout with the management password
    Given acp2api is in lockout since a lockout signal
    And the management panel shows the evidence of the lockout
    When the owner ends the lockout with the management password
    Then acp2api is in the normal state at once
    And the next request runs

  @FR-035 @issue-73
  Scenario: The owner ends a lockout that a restart kept
    Given acp2api is in lockout since a lockout signal
    When the owner restarts acp2api
    And the owner opens the management panel
    And the owner ends the lockout with the management password
    Then acp2api is in the normal state at once
    And the next request runs

  @FR-009 @FR-035 @issue-73
  Scenario: An open panel session does not end a lockout without the management password
    Given acp2api is in lockout since a lockout signal
    And the owner has a panel session
    When the owner asks for the end of the lockout with no management password
    Then acp2api is in the state "lockout"

  @FR-035 @issue-73
  Scenario: acp2api sends no test request after the end of a lockout
    Given the owner ended a lockout
    When no API request comes for 10 minutes
    Then the official runtime gets no prompt

  @FR-035 @issue-73
  Scenario: The next lockout signal after the end starts a new lockout
    Given the owner ended a lockout
    When a prompt turn ends with the stderr error line 403 "PERMISSION_DENIED"
    Then acp2api is in the state "lockout"

  @FR-035 @issue-70
  Scenario: The deletion of the state file ends a lockout
    Given acp2api is stopped, and the state file holds a lockout
    When the owner deletes the state file and starts acp2api
    Then acp2api is not in the state "lockout"
    And no request gets the lockout message
    And the data folder holds a new state file
