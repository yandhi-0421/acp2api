# language: en
Feature: Start of acp2api
  The owner starts acp2api with one command in a terminal.
  The terminal shows the address of the management panel, one line for each request, and one line for each change of state.
  In a quoted message, a value in angle brackets is a value that acp2api fills in, as the section "Message texts" of the PRD gives.

  @FR-002 @issue-37
  Scenario: The help text lists only the four options
    When the owner runs "acp2api --help"
    Then the help text lists the options "--port", "--data-dir", "--version", and "--help"
    And the help text lists no other option and no command

  @FR-002 @issue-55 @wip
  Scenario: A start that is not the first start shows the address of the management panel
    Given acp2api started one time before with the data folder "D"
    When the owner starts acp2api with the data folder "D" and the port 8329
    Then the terminal shows the address of the management panel on the port 8329
    And the browser can open the management panel at that address

  @FR-002 @issue-42
  Scenario: The terminal shows one line for each request
    Given acp2api is in the normal state
    When an API client sends 3 requests with the model name "gemini-3.8-flash"
    Then the terminal shows 3 request lines
    And each request line holds the request ID, the API, and the model name of its request

  @FR-002 @issue-61 @wip
  Scenario: The terminal shows one line for each change of state
    Given acp2api is in the normal state
    When a quota signal starts a quota wait
    Then the terminal shows one line for the change to the state "quota wait"

  @FR-005 @FR-009 @issue-55 @wip
  Scenario: A first start makes a local API key and a management password and shows them
    Given the data folder "D" is empty
    When the owner starts acp2api with the data folder "D"
    Then acp2api makes the configuration file in "D"
    And the terminal shows the address of the management panel, the local API key, and the management password
    And a request with the local API key from the terminal gets no HTTP 401
    And the management password from the terminal opens the management panel
    And the configuration file holds only the hash of that management password

  @FR-005 @issue-37
  Scenario: A first start opens the management panel in the browser
    Given the data folder "D" is empty
    When the owner starts acp2api with the data folder "D"
    Then the browser opens the address of the management panel

  @FR-005 @issue-37
  Scenario: Two first starts make different secrets
    Given the data folders "D1" and "D2" are empty
    When the owner does a first start with the data folder "D1" and then a first start with the data folder "D2"
    Then the two local API keys are different
    And the two management passwords are different

  @FR-006 @issue-64 @wip
  Scenario Outline: A second start for the same computer account fails
    Given acp2api runs for a computer account with the data folder "D1"
    And its system temporary folder is "T1"
    When the same computer account starts acp2api with the data folder "<second data folder>"
    And that start uses the system temporary folder "<second temporary folder>"
    Then the second start fails with the message "acp2api: another acp2api runs for this computer account. Its management panel is at <address>."
    And the address in the message is the address of the management panel of the first acp2api

    Examples:
      | second data folder | second temporary folder |
      | D1                 | T1                      |
      | D2                 | T1                      |
      | D1                 | T2                      |
      | D2                 | T2                      |

  @FR-006 @issue-64 @wip
  Scenario: The first acp2api continues after a second start
    Given acp2api runs for a computer account with the data folder "D1"
    And its system temporary folder is "T1"
    And the first acp2api has a temporary folder and a session cwd folder
    When the same computer account starts acp2api with the data folder "D2"
    And that start uses the system temporary folder "T2"
    Then the first acp2api answers the next request
    And the temporary folder and the session cwd folder of the first acp2api are still there

  @FR-006 @issue-64 @wip
  Scenario: Two computer accounts keep their own temporary folders
    Given acp2api runs for computer account "A" with data folder "D1" and port 8329
    And its system temporary folder is "T"
    And the first acp2api has a temporary folder and a session cwd folder
    When computer account "B" starts acp2api with data folder "D2" and port 8330
    And that start uses the system temporary folder "T"
    Then both instances answer requests on their own ports
    When computer account "B" stops its acp2api
    Then the first acp2api answers the next request
    And the temporary folder and the session cwd folder of the first acp2api are still there

  @FR-006 @issue-64 @wip
  Scenario: A second start waits for the first panel address
    Given a first acp2api owns the account lock and has not published its panel address
    When a second acp2api starts for that account
    And 10 minutes pass before the first address exists
    Then the second start still waits and has shown no M-24
    When the first acp2api publishes its panel address
    Then the second start fails with M-24 and that address

  @FR-006 @issue-64 @wip
  Scenario: The owner can cancel a second start before the first address exists
    Given a second start waits for the first panel address
    When the owner cancels the second start
    Then the second start exits with no change to the first instance's files
    And the first acp2api can still publish its panel address and answer requests

  @FR-006 @issue-64 @wip
  Scenario: A waiting start can take ownership after the first start crashes before publication
    Given a second start waits for the first panel address
    When the first start crashes before it publishes that address
    Then the second start becomes the sole instance and publishes its own panel address
    And it shows no M-24 with a stale or absent address

  @FR-006 @issue-64 @wip
  Scenario Outline: Unusable coordination data stops the new start without disturbing the owner
    Given a first acp2api runs for a computer account
    And its account coordination data at "P" is <condition>
    When a second acp2api starts for that account
    Then it fails with "acp2api: cannot use the account coordination data at P."
    And it does not delete or replace an account lock
    And the first acp2api still answers requests

    Examples:
      | condition  |
      | unreadable |
      | damaged    |
