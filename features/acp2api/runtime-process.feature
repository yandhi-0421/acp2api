# language: en
Feature: Runtime process
  acp2api starts one runtime process, and all ACP sessions and all prompt turns use it.
  After 3 failures in 10 minutes, acp2api is in the state "runtime unavailable" until a new try succeeds.
  N is the number of ACP sessions after which acp2api replaces the runtime process. OD-1 of the PRD decides its value.
  In a quoted message, a value in angle brackets is a value that acp2api fills in, as the section "Message texts" of the PRD gives.

  @FR-040 @FR-042
  Scenario: With no build anywhere, acp2api is in the state runtime unavailable
    Given no place that acp2api searches holds a build of the official runtime
    When the owner starts acp2api
    Then acp2api is in the state "runtime unavailable"
    And a request gets the message "acp2api: the official runtime is unavailable: acp2api found no executable of the official runtime. Accepted versions: <versions>. See <address of the management panel>."

  @FR-040
  Scenario Outline: Each other reason gives the state runtime unavailable and M-14 with that reason
    Given <cause>
    Then acp2api is in the state "runtime unavailable"
    And a request gets the message "acp2api: the official runtime is unavailable: <reason>. Accepted versions: <versions>. See <address of the management panel>."
    And the official runtime gets no prompt

    Examples:
      | cause                                                                      | reason                                                  |
      | acp2api found only builds that are not accepted versions                    | no build that acp2api found is an accepted version      |
      | the runtime process had 3 failures in 10 minutes                            | the runtime process failed 3 times in 10 minutes        |

  @FR-040
  Scenario: In the state runtime unavailable, acp2api starts no runtime process by itself
    Given the runtime process had 3 failures in 10 minutes
    When 30 minutes pass with no event that starts a new try
    Then acp2api started no runtime process after the third failure

  @FR-040
  Scenario Outline: An event starts a new try, and a successful start ends the state
    Given acp2api is in the state "runtime unavailable"
    And an accepted version of the official runtime is now available
    When <event>
    Then the start of a runtime process succeeds
    And the next request runs

    Examples:
      | event                                                         |
      | the owner uses the retry entry of the management panel        |
      | the owner restarts acp2api                                    |
      | the owner sets a new path of agy_acp_server                   |
      | the owner starts a new search                                 |
      | a compatibility check of a found build passes                 |

  @FR-041
  Scenario: Two prompt turns at the same time use the one runtime process
    Given acp2api started in the normal state with the concurrency limit 2
    When two API clients send requests of two conversations at the same time
    Then the two prompt turns run at the same time
    And one runtime process runs

  @FR-041
  Scenario: acp2api does not stop the runtime process when no request comes
    Given acp2api is in the normal state with one runtime process
    When 60 minutes pass with no request
    Then the same runtime process still runs

  @FR-041
  Scenario: With N ACP sessions, a request that needs a new ACP session waits until acp2api replaces the runtime process
    Given the runtime process holds N ACP sessions
    And one prompt turn runs
    When a request comes that needs a new ACP session
    Then the request waits in the queue
    When the prompt turn that runs ends
    Then acp2api replaces the runtime process
    And the request gets its response from the new runtime process

  @FR-041
  Scenario: With N ACP sessions, a request that continues an ACP session runs at once
    Given the runtime process holds N ACP sessions
    When a request comes that continues one of these ACP sessions
    Then the prompt turn of the request starts at once in the same runtime process

  @FR-041
  Scenario: With fewer than N ACP sessions, a request that needs a new ACP session runs at once
    Given the runtime process holds N minus 1 ACP sessions
    When a request comes that needs a new ACP session
    Then the prompt turn of the request starts at once in the same runtime process

  @FR-041
  Scenario: An exit that acp2api did not cause ends each prompt turn of the runtime process
    Given the prompt turns of the requests "A" and "B" run in the runtime process
    When the runtime process exits, and acp2api did not cause the exit
    Then "A" and "B" each get the error of the 503 row with the message "acp2api: the runtime process failed, so acp2api ended the prompt turn."
    And acp2api starts a new runtime process at once

  @FR-041
  Scenario Outline: A prompt turn that does not end 30 s after its cancel is a failure
    Given the prompt turns of the requests "A" and "B" run in the runtime process
    And the API client of "A" disconnected, so acp2api cancelled the prompt turn of "A"
    When the prompt turn of "A" <end>
    Then <result>

    Examples:
      | end                                  | result                                                                                                  |
      | does not end 30 s after its cancel    | "B" gets the error of the 503 row with the message "acp2api: the runtime process failed, so acp2api ended the prompt turn." |
      | ends 20 s after its cancel           | the prompt turn of "B" continues to its end                                                             |

  @FR-041
  Scenario: A start with no answer in 5 minutes is a failure
    Given the next start of a runtime process gives no answer
    When 5 minutes pass after the start
    Then acp2api starts a new runtime process at once

  @FR-041
  Scenario Outline: The count of failures in 10 minutes decides the state runtime unavailable
    Given acp2api is in the normal state
    When the runtime process exits with no cause from acp2api at the times <times>
    Then <result>

    Examples:
      | times                              | result                                                  |
      | 0 minutes, 4 minutes, and 9 minutes | acp2api is in the state "runtime unavailable"          |
      | 0 minutes, 5 minutes, and 11 minutes | acp2api starts a new runtime process after each exit  |

  @FR-045
  Scenario: At its exit, acp2api ends each request with a prompt turn and each request in the queue with M-15
    Given the concurrency limit is 1
    And the prompt turn of the request "A" with no stream runs
    And the request "B" with no stream waits in the queue
    When the owner stops acp2api in its terminal
    Then the official runtime gets a cancel of the prompt turn of "A"
    And "A" and "B" each get HTTP 400 with the message "acp2api: acp2api stopped before the end of this request."

  @FR-045
  Scenario Outline: The runtime process and all its child processes end after acp2api ends
    Given the runtime process runs with two harness processes
    When <end of acp2api>
    Then the runtime process and its two harness processes have ended with no step of the owner

    Examples:
      | end of acp2api                          |
      | the owner stops acp2api in its terminal |
      | a kill of acp2api ends it               |
