# language: en
Feature: Quota
  After a quota signal, acp2api starts no prompt turn until the reset time. The quota wait ends without the owner.
  acp2api sends "/usage" to the official runtime only in three cases.
  The cases are the reset time of a quota wait, the stop reason "refusal", and a refresh by the owner.
  In a quoted message, a value in angle brackets is a value that acp2api fills in, as the section "Message texts" of the PRD gives.

  Background:
    Given acp2api is in the normal state
    And the official runtime offers the model name "gemini-3.8-flash"

  @FR-036 @issue-61 @wip
  Scenario: A request in a quota wait gets HTTP 429 and starts no prompt turn
    Given a prompt turn ended with the stderr error line 429 "RESOURCE_EXHAUSTED" with a reset time 2 minutes away
    When an API client sends a valid request to "POST /v1/messages"
    Then acp2api refuses the request with HTTP 429 and the message "acp2api: the Google quota is used up until <reset time>. acp2api starts no prompt turn before that time."
    And the header "retry-after" has a value of 120 or less
    And the official runtime gets no prompt

  @FR-036 @issue-61 @wip
  Scenario: The first request after the reset time runs
    Given a prompt turn ended with the stderr error line 429 "RESOURCE_EXHAUSTED" with a reset time 2 minutes away
    When an API client sends a valid request 2 minutes and 1 second later
    Then the official runtime gets a prompt for the request

  @FR-036 @issue-61 @wip
  Scenario: acp2api reads the reset time from /usage when the error text has none
    Given a prompt turn ended with the stderr error line 429 "RESOURCE_EXHAUSTED" with no reset time
    And "/usage" gives a reset time 10 minutes away
    When an API client sends a valid request
    Then acp2api refuses the request with HTTP 429
    And the header "retry-after" has a value of 600 or less
    And the official runtime got "/usage" one time

  @FR-036 @FR-052 @issue-61 @wip
  Scenario: acp2api sends no quota query when the error text has a reset time
    Given a prompt turn ended with the stderr error line 429 "RESOURCE_EXHAUSTED" with a reset time 2 minutes away
    When an API client sends a valid request
    Then acp2api refuses the request with HTTP 429
    And the official runtime got no "/usage"

  @FR-036 @issue-61 @wip
  Scenario Outline: With no reset time anywhere, the quota wait lasts 60 minutes
    Given a prompt turn ended with the stderr error line 429 "RESOURCE_EXHAUSTED" with no reset time
    And "/usage" gives no reset time
    When an API client sends a valid request <time> later
    Then <result>

    Examples:
      | time                | result                                              |
      | 60 minutes minus 1 ms | acp2api refuses the request with HTTP 429         |
      | 60 minutes          | the official runtime gets a prompt for the request |
      | 60 minutes plus 1 ms | the official runtime gets a prompt for the request |

  @FR-036 @issue-61 @wip
  Scenario: A new quota signal after a quota wait starts a new quota wait
    Given a quota wait ended at its reset time
    When a prompt turn ends with the stderr error line 402
    Then acp2api is in the state "quota wait"

  @FR-052 @issue-66 @wip
  Scenario: An open management panel with no refresh sends no quota query
    Given the owner has the management panel open
    When one hour passes with no refresh of the quota and no API request
    Then the official runtime gets no "/usage"

  @FR-052 @issue-66 @wip
  Scenario Outline: acp2api refuses a second refresh of the quota in 60 s
    Given the owner refreshed the quota in the management panel
    When the owner refreshes the quota again <time> later
    Then the official runtime got "/usage" <count>

    Examples:
      | time     | count     |
      | 59999 ms | one time  |
      | 60 s     | two times |
      | 60001 ms | two times |

  @FR-052 @issue-66 @wip
  Scenario: The management panel shows the last result of /usage and its time
    Given "/usage" gives a result
    When the owner refreshes the quota in the management panel
    Then the tile "quota" shows that result and the time of that result
