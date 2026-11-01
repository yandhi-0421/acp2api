# language: en
Feature: Runtime process
  acp2api starts one runtime process, and all ACP sessions and all prompt turns use it.
  After 3 failures in 10 minutes, acp2api is in the state "runtime unavailable" until a new try succeeds.
  N is the number of ACP sessions after which acp2api replaces the runtime process. FR-041 gives its temporary value, and OD-1 of the PRD decides its real value.
  In a quoted message, a value in angle brackets is a value that acp2api fills in, as the section "Message texts" of the PRD gives.

  @FR-040 @issue-131
  Scenario: A failed automatic copy names the accepted build in M-14
    Given acp2api has no copy of the official runtime
    And the install folder of Zed holds the accepted build 1.4.0
    And the data folder denies creation of the automatic copy
    When the owner starts acp2api
    Then the result of the search lists the build in the install folder of Zed
    And acp2api is in the state "runtime unavailable"
    And a request gets the message "acp2api: the official runtime is unavailable: the copy of the accepted build 1.4.0 failed. Accepted versions: <versions>. See <address of the management panel>."
    And the official runtime gets no prompt

  @FR-040 @FR-042 @issue-58
  Scenario: With no build anywhere, acp2api is in the state runtime unavailable
    Given no place that acp2api searches holds a build of the official runtime
    When the owner starts acp2api
    Then acp2api is in the state "runtime unavailable"
    And a request gets the message "acp2api: the official runtime is unavailable: acp2api found no executable of the official runtime. Accepted versions: <versions>. See <address of the management panel>."

  @FR-040 @issue-68
  Scenario Outline: Each other reason gives the state runtime unavailable and M-14 with that reason
    Given <cause>
    Then acp2api is in the state "runtime unavailable"
    And a request gets the message "acp2api: the official runtime is unavailable: <reason>. Accepted versions: <versions>. See <address of the management panel>."
    And the official runtime gets no prompt

    Examples:
      | cause                                                                      | reason                                                  |
      | acp2api found only builds that are not accepted versions                    | no build that acp2api found is an accepted version      |
      | the runtime process had 3 failures in 10 minutes                            | the runtime process failed 3 times in 10 minutes        |

  @FR-040 @issue-68
  Scenario: In the state runtime unavailable, acp2api starts no runtime process by itself
    Given the runtime process had 3 failures in 10 minutes
    When 30 minutes pass with no event that starts a new try
    Then acp2api started no runtime process after the third failure

  @FR-040 @issue-68
  Scenario Outline: An event starts a new try, and a successful start ends the state
    Given acp2api is in the state "runtime unavailable"
    And an accepted version of the official runtime is now available
    And the official runtime has a valid sign-in
    And no condition other than "runtime unavailable" stops requests
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

  @FR-041 @issue-63
  Scenario: Two prompt turns at the same time use the one runtime process
    Given acp2api started in the normal state with the concurrency limit 2
    When two API clients send requests of two conversations at the same time
    Then the two prompt turns run at the same time
    And one runtime process runs

  @FR-041 @issue-63
  Scenario: acp2api does not stop the runtime process when no request comes
    Given acp2api is in the normal state with one runtime process
    When 60 minutes pass with no request
    Then the same runtime process still runs

  @FR-041 @issue-204
  Scenario: With N ACP sessions, a request that needs a new ACP session waits until acp2api replaces the runtime process
    Given the runtime process holds N ACP sessions
    And one prompt turn runs
    When a request comes that needs a new ACP session
    Then that request waits in the queue
    When the prompt turn that runs ends
    Then acp2api replaces the runtime process
    And the request gets its response from the new runtime process

  @FR-041 @issue-204
  Scenario: With N ACP sessions, a request that continues an ACP session runs at once
    Given the runtime process holds N ACP sessions
    When a request comes that continues one of these ACP sessions
    Then the prompt turn of the request starts at once in the same runtime process

  @FR-041 @issue-204
  Scenario: With fewer than N ACP sessions, a request that needs a new ACP session runs at once
    Given the runtime process holds N minus 1 ACP sessions
    When a request comes that needs a new ACP session
    Then the prompt turn of the request starts at once in the same runtime process

  @FR-041 @issue-204
  Scenario: An ACP session stays in the count after acp2api drops its record, and acp2api replaces an idle runtime process at once
    Given the runtime process holds N ACP sessions
    When 24 hours pass with no request
    And a request comes that needs a new ACP session
    Then acp2api replaces the runtime process
    And the request gets its response from the new runtime process

  @FR-041 @issue-204
  Scenario: The replacement waits for the end of each prompt turn that runs
    Given the runtime process holds N ACP sessions
    And two prompt turns run in these ACP sessions
    When a request comes that needs a new ACP session
    And the first prompt turn ends
    Then that request waits in the queue
    And the same runtime process still runs
    When the second prompt turn ends
    Then acp2api replaces the runtime process
    And the request gets its response from the new runtime process

  @FR-041 @issue-204
  Scenario: While a replacement waits, a request that continues an ACP session also waits, and the order of arrival stays
    Given the runtime process holds N ACP sessions
    And one prompt turn runs
    When a request comes that needs a new ACP session
    And a request comes that continues one of these ACP sessions
    Then each of these requests waits in the queue
    When the prompt turn that runs ends
    Then acp2api replaces the runtime process
    And the two requests get their responses from the new runtime process in the order of arrival
    And the official runtime gets "session/load" for the continued ACP session in the new runtime process

  @FR-041 @issue-204
  Scenario: After a replacement, the count of ACP sessions starts again
    Given the runtime process holds N ACP sessions
    When a request comes that needs a new ACP session
    Then acp2api replaces the runtime process
    And the request gets its response from the new runtime process
    When requests of new conversations bring the new runtime process to N ACP sessions
    Then each of these prompt turns ran in the new runtime process
    When a request comes that needs a new ACP session
    Then acp2api replaces the runtime process
    And the request gets its response from the new runtime process

  @FR-041 @issue-204
  Scenario: A session/load is one more ACP session of the new runtime process
    Given the runtime process holds N ACP sessions
    When a request comes that needs a new ACP session
    Then acp2api replaces the runtime process
    And the request gets its response from the new runtime process
    When requests of new conversations bring the new runtime process to N minus 1 ACP sessions
    And a request comes that continues an ACP session of the earlier runtime process
    Then the official runtime gets "session/load" for the continued ACP session in the same runtime process
    And the prompt turn of the request starts at once in the same runtime process
    When a request comes that continues an ACP session of the earlier runtime process
    Then acp2api replaces the runtime process
    And the official runtime gets "session/load" for the continued ACP session in the new runtime process
    And the request gets its response from the new runtime process

  @FR-041 @issue-204
  Scenario: A failed start in a replacement follows the failure rule, and the request stays in the queue
    Given the runtime process holds N ACP sessions
    And the start of the replacement gives no answer
    When a request comes that needs a new ACP session
    Then acp2api replaces the runtime process
    When 5 minutes pass after the start of the replacement
    Then acp2api starts one more runtime process at once
    And the request gets its response from the new runtime process

  @FR-041 @issue-63
  Scenario: An exit that acp2api did not cause ends each prompt turn of the runtime process
    Given the prompt turns of the requests "A" and "B" run in the runtime process
    When the runtime process exits, and acp2api did not cause the exit
    Then "A" and "B" each get the error of the 503 row with the message "acp2api: the runtime process failed, so acp2api ended the prompt turn."
    And acp2api starts a new runtime process at once

  @FR-041 @issue-63
  Scenario Outline: A prompt turn that does not end 30 s after its cancel is a failure
    Given the prompt turns of the requests "A" and "B" run in the runtime process
    And the API client of "A" disconnected, so acp2api cancelled the prompt turn of "A"
    When the prompt turn of "A" <end>
    Then <result>

    Examples:
      | end                                  | result                                                                                                  |
      | does not end 30 s after its cancel    | "B" gets the error of the 503 row with the message "acp2api: the runtime process failed, so acp2api ended the prompt turn." |
      | ends 20 s after its cancel           | the prompt turn of "B" continues to its end                                                             |

  @FR-041 @issue-63
  Scenario: A start with no answer in 5 minutes is a failure
    Given the next start of a runtime process gives no answer
    When 5 minutes pass after the start
    Then acp2api starts a new runtime process at once

  @FR-041 @issue-63
  Scenario Outline: The count of failures in 10 minutes decides the state runtime unavailable
    Given acp2api is in the normal state
    When the runtime process exits with no cause from acp2api at the times <times>
    Then <result>

    Examples:
      | times                              | result                                                  |
      | 0 minutes, 4 minutes, and 9 minutes | acp2api is in the state "runtime unavailable"          |
      | 0 minutes, 5 minutes, and 11 minutes | acp2api starts a new runtime process after each exit  |

  @FR-041 @issue-172
  Scenario: Three runtime processes with stalled stdin give the state runtime unavailable
    Given acp2api is in the normal state
    When three runtime processes stop reading stdin at 0 minutes, 4 minutes, and 9 minutes
    Then acp2api is in the state "runtime unavailable" with the reason "the runtime process failed 3 times in 10 minutes"

  @FR-041 @issue-211
  Scenario Outline: A non-prompt ACP call with no answer in 5 minutes is a failure
    Given <call> gives no answer
    When 5 minutes pass after acp2api sends that call
    Then acp2api replaces the runtime process at once
    And <caller> gets the message "acp2api: the runtime process failed, so acp2api ended the prompt turn." <form>

    Examples:
      | call                            | caller         | form                                 |
      | the "session/new" of a request  | the API client | with the 503 row                     |
      | the "session/load" of a request | the API client | with the 503 row                     |
      | the sign-in check of a request  | the API client | with the 503 row and no stream event |
      | the model read of the owner     | the owner      | in the management panel              |
      | the "/usage" of a quota refresh | the owner      | in the management panel              |

  @FR-041 @issue-211
  Scenario Outline: A non-prompt ACP call that answers just under 5 minutes is no failure
    Given <call> holds its answer
    When that call answers 1 ms before 5 minutes pass, and 1 minute more passes
    Then the caller of that call gets its normal result
    And acp2api starts no new runtime process

    Examples:
      | call                            |
      | the "session/new" of a request  |
      | the "session/load" of a request |
      | the sign-in check of a request  |
      | the model read of the owner     |
      | the "/usage" of a quota refresh |

  @FR-041 @FR-036 @issue-211
  Scenario: A "/usage" for a reset time with no answer in 5 minutes keeps the quota wait of 60 minutes
    Given the "/usage" for the reset time of a request gives no answer
    When 5 minutes pass after acp2api sends that call
    Then acp2api replaces the runtime process at once
    And the API client gets the message "acp2api: the runtime process failed, so acp2api ended the prompt turn." with the 503 row
    And the next request gets the quota message with "retry-after" for the 60 minutes after the quota signal

  @FR-041 @FR-034 @issue-211
  Scenario: A "/usage" after the stop reason refusal with no answer in 5 minutes starts no quota wait
    Given the "/usage" after the stop reason "refusal" of a request gives no answer
    When 5 minutes pass after acp2api sends that call
    Then acp2api replaces the runtime process at once
    And the API client gets the message "acp2api: the runtime process failed, so acp2api ended the prompt turn." with the 503 row
    And the next request gets HTTP 200

  @FR-041 @issue-211
  Scenario: An update gives a "/usage" no more time
    Given the "/usage" of a quota refresh sends an update after 4 minutes and does not end
    When 5 minutes pass after acp2api sends that call
    Then acp2api replaces the runtime process at once
    And the owner gets the message "acp2api: the runtime process failed, so acp2api ended the prompt turn." in the management panel

  @FR-041 @FR-050 @issue-211
  Scenario: The time limit starts when acp2api sends the call, not when the request comes
    Given the concurrency limit is 1
    And a prompt turn with updates runs
    And a request that needs a "session/new" with no answer waits in the queue
    When the prompt turn with updates ends 6 minutes after that request came
    And 5 minutes pass after acp2api sends that call
    Then acp2api replaces the runtime process at once
    And the API client gets the message "acp2api: the runtime process failed, so acp2api ended the prompt turn." with the 503 row

  @FR-041 @issue-211
  Scenario: A failure at the time limit ends each other prompt turn of the runtime process
    Given a prompt turn with updates runs
    And the "session/new" of a request gives no answer
    When 5 minutes pass after acp2api sends that call
    Then acp2api replaces the runtime process at once
    And the API client gets the message "acp2api: the runtime process failed, so acp2api ended the prompt turn." with the 503 row
    And the prompt turn with updates gets the message "acp2api: the runtime process failed, so acp2api ended the prompt turn." with the 503 row

  @FR-041 @FR-049 @issue-211
  Scenario: After a failure at the time limit, a request in the queue gets its response from the new runtime process
    Given the concurrency limit is 1
    And the "session/new" of a request gives no answer
    And another request waits in the queue
    When 5 minutes pass after acp2api sends that call
    Then acp2api replaces the runtime process at once
    And the API client gets the message "acp2api: the runtime process failed, so acp2api ended the prompt turn." with the 503 row
    And the request in the queue gets its response from the new runtime process

  @FR-041 @issue-211
  Scenario: A failure at the time limit ends the wait for a replacement at N ACP sessions
    Given the runtime process holds N minus 1 ACP sessions
    And the "session/new" of a request gives no answer
    When a request comes that needs a new ACP session
    Then that request waits in the queue
    When 5 minutes pass after acp2api sends that call
    Then acp2api replaces the runtime process at once
    And the API client gets the message "acp2api: the runtime process failed, so acp2api ended the prompt turn." with the 503 row
    And the request gets its response from the new runtime process

  @FR-041 @FR-040 @issue-211
  Scenario Outline: A failure at the time limit counts toward the 3 failures in 10 minutes
    Given acp2api is in the normal state
    When a "session/new" passes the time limit at 5 minutes
    And the runtime process exits with no cause from acp2api at 7 minutes
    And a "session/new" passes the time limit at <last> minutes
    Then <result>

    Examples:
      | last | result                                                                                                            |
      | 12   | acp2api is in the state "runtime unavailable" with the reason "the runtime process failed 3 times in 10 minutes" |
      | 16   | acp2api started a new runtime process after each failure                                                          |

  @FR-041 @issue-211
  Scenario Outline: A disconnect of the API client does not stop the time limit
    Given <call> gives no answer
    And the API client of that call disconnected
    When 5 minutes pass after acp2api sends that call
    Then acp2api replaces the runtime process at once

    Examples:
      | call                           |
      | the "session/new" of a request |
      | the sign-in check of a request |

  @FR-041 @issue-211
  Scenario: A late answer of the replaced runtime process has no effect
    Given the "session/new" of a request gives no answer
    And that runtime process writes the answer of that call when acp2api stops it
    When 5 minutes pass after acp2api sends that call
    Then acp2api replaces the runtime process at once
    And the API client gets the message "acp2api: the runtime process failed, so acp2api ended the prompt turn." with the 503 row
    And the next request gets HTTP 200
    And no ACP call uses the ACP session of the late answer

  @FR-041 @FR-051 @issue-211
  Scenario: A "session/set_config_option" with no answer follows the no-update rule and the cancel limit
    Given the "session/set_config_option" of a prompt turn gives no answer
    When 5 minutes pass after acp2api sends that call
    Then the API client gets the message "acp2api: the official runtime sent no update for 5 minutes, so acp2api cancelled the prompt turn." with the 503 row
    And acp2api starts no new runtime process
    When 30 s pass after that message
    Then acp2api replaces the runtime process at once

  @FR-045 @issue-63
  Scenario: At its exit, acp2api ends each request with a prompt turn and each request in the queue with M-15
    Given the concurrency limit is 1
    And the prompt turn of the request "A" with no stream runs
    And the request "B" with no stream waits in the queue
    When the owner stops acp2api in its terminal
    Then the official runtime gets a cancel of the prompt turn of "A"
    And "A" and "B" each get HTTP 400 with the message "acp2api: acp2api stopped before the end of this request."

  @FR-045 @issue-63
  Scenario Outline: The runtime process and all its child processes end after acp2api ends
    Given the runtime process runs with two harness processes
    When <end of acp2api>
    Then the runtime process and its two harness processes have ended with no step of the owner

    Examples:
      | end of acp2api                          |
      | the owner stops acp2api in its terminal |
      | a kill of acp2api ends it               |
