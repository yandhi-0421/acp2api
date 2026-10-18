# language: en
Feature: Sign-in
  When the official runtime has no valid sign-in, acp2api starts no prompt turn.
  The owner signs in through the one sign-in entry of the management panel. The official runtime keeps the token.
  In a quoted message, a value in angle brackets is a value that acp2api fills in, as the section "Message texts" of the PRD gives.

  Background:
    Given the official runtime offers the model name "gemini-3.8-flash"

  @FR-038
  Scenario: A request in the state sign-in required gets M-13 and starts no prompt turn
    Given the official runtime has no valid sign-in
    And acp2api is in the state "sign-in required"
    When an API client sends a valid request to "POST /v1/messages"
    Then acp2api refuses the request with the message "acp2api: the official runtime is not signed in. Sign in at <address of the sign-in entry>."
    And the official runtime gets no prompt

  @FR-038
  Scenario: A passed sign-in check of a request ends the state and the request runs
    Given acp2api is in the state "sign-in required"
    And the official runtime has a valid sign-in again
    When an API client sends a valid request to "POST /v1/messages"
    Then acp2api checks the sign-in one time
    And acp2api is in the normal state
    And the official runtime gets a prompt for the request

  @FR-038
  Scenario: After a sign-in in the management panel, the next request runs
    Given acp2api is in the state "sign-in required"
    When the owner signs in through the management panel
    And an API client sends a valid request to "POST /v1/messages"
    Then the official runtime gets a prompt for the request

  @FR-038
  Scenario: acp2api keeps no record of the state and checks the sign-in at each start
    Given acp2api was in the state "sign-in required" and then stopped
    And the official runtime now has a valid sign-in
    When the owner starts acp2api
    Then acp2api is in the normal state
    And the state file holds no record of the state "sign-in required"

  @FR-039
  Scenario: The sign-in entry uses only the sign-in method oauth-personal
    Given acp2api is in the state "sign-in required"
    When the owner starts the sign-in in the management panel
    Then the official runtime gets a sign-in request with the method "oauth-personal"
    And the official runtime gets no sign-in request with another method

  @FR-039
  Scenario: The management panel shows the time left, the sign-in address, and a cancel entry during the wait
    Given acp2api is in the state "sign-in required"
    And the official runtime gives the sign-in address "https://accounts.example.com/signin"
    When the owner starts the sign-in in the management panel
    Then the management panel shows the time left, which starts at 10 minutes
    And the management panel shows the sign-in address "https://accounts.example.com/signin"
    And the management panel shows the entry "Cancel the sign-in"

  @FR-039
  Scenario Outline: The end of a sign-in wait replaces the runtime process and the state stays sign-in required
    Given acp2api is in the state "sign-in required"
    And the owner started the sign-in in the management panel
    When <event>
    Then acp2api replaces the runtime process
    And acp2api is in the state "sign-in required"

    Examples:
      | event                                          |
      | the owner cancels the sign-in                  |
      | 10 minutes pass with no sign-in                |

  @FR-039
  Scenario: The replacement after a sign-in wait is not a failure of the runtime process
    Given acp2api is in the state "sign-in required"
    When the owner starts and cancels the sign-in 3 times in 10 minutes
    Then acp2api is in the state "sign-in required"
    And acp2api is not in the state "runtime unavailable"

  @FR-039
  Scenario: No file of acp2api holds the token
    Given acp2api is in the state "sign-in required"
    When the owner signs in through the management panel, and the official runtime keeps the token "T1"
    Then the configuration file, the state file, and the logs do not hold "T1"

  @FR-039
  Scenario: A sign-in of an editor does not sign in acp2api
    Given an editor signed in the official runtime with its own runtime home
    When the owner starts acp2api for the first time
    Then acp2api is in the state "sign-in required"
