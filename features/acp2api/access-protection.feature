# language: en
Feature: Access protection
  The local API key protects the APIs. The management password protects the management panel.
  acp2api refuses requests from web pages, and it accepts no connection from another network address.

  Background:
    Given acp2api is in the normal state on the port 8329
    And the local API key is "K1"

  @FR-007 @issue-49
  Scenario Outline: A request with no local API key or a wrong local API key gets HTTP 401
    When an API client sends "<request>" with <key>
    Then acp2api refuses the request with HTTP 401 and the message "acp2api: the local API key is absent or wrong."
    And the official runtime gets no prompt

    Examples:
      | request                   | key                       |
      | POST /v1/messages         | no local API key          |
      | POST /v1/responses        | the wrong local API key "K9" |
      | POST /v1/chat/completions | no local API key          |
      | GET /v1/models            | the wrong local API key "K9" |

  @FR-007 @issue-49
  Scenario Outline: The local API key goes in either of two headers
    When an API client sends "GET /v1/models" with the local API key "K1" in the header "<header>"
    Then acp2api answers with HTTP 200

    Examples:
      | header                |
      | Authorization: Bearer |
      | x-api-key             |

  @FR-007 @issue-55 @wip
  Scenario: A new local API key replaces the old key at once
    When the owner makes a new local API key in the management panel
    Then a request with the local API key "K1" gets HTTP 401
    And a request with the new local API key gets HTTP 200

  @FR-008 @issue-49
  Scenario: acp2api accepts no connection on another network address of the computer
    Given the computer has a network address other than 127.0.0.1
    When a program connects to the port 8329 on that network address
    Then acp2api accepts no connection

  @FR-008 @issue-49
  Scenario: A request to an API with the Origin of a web page gets a 4xx status
    When an API client sends "POST /v1/messages" with the header "Origin: https://example.com"
    Then acp2api answers with an HTTP status from 400 to 499
    And no prompt turn starts

  @FR-008 @issue-49
  Scenario: A request to an API with the Origin of the management panel gets a 4xx status
    When a request with the Origin of the management panel goes to "POST /v1/chat/completions"
    Then acp2api answers with an HTTP status from 400 to 499
    And no prompt turn starts

  @FR-008 @issue-55 @wip
  Scenario: A request to the management panel with the Origin of the management panel is accepted
    Given the owner has a panel session
    When a request with the Origin of the management panel goes to a management endpoint
    Then acp2api answers with an HTTP status from 200 to 299

  @FR-008 @issue-49
  Scenario Outline: acp2api accepts only a local Host header with its own port
    When an API client sends "GET /v1/models" with the header "Host: <host>"
    Then acp2api answers with <result>

    Examples:
      | host           | result                         |
      | 127.0.0.1:8329 | HTTP 200                       |
      | localhost:8329 | HTTP 200                       |
      | example.com    | an HTTP status from 400 to 499 |
      | localhost:8330 | an HTTP status from 400 to 499 |

  @FR-008 @issue-49
  Scenario: An OPTIONS request gets no CORS header
    When a program sends "OPTIONS /v1/messages" with the headers "Origin: https://example.com" and "Access-Control-Request-Method: POST"
    Then the answer has no header whose name starts with "Access-Control-"

  @FR-008 @issue-49
  Scenario: An accepted request gets no CORS header
    When an API client sends "GET /v1/models"
    Then acp2api answers with HTTP 200
    And the answer has no header whose name starts with "Access-Control-"

  @FR-009 @issue-55 @wip
  Scenario: The management panel shows only the password entry with no panel session
    Given a browser with no panel session
    When the browser opens the management panel
    Then the page holds the entry for the management password
    And the page holds no local API key and no state of acp2api

  @FR-009 @issue-55 @wip
  Scenario: A management endpoint refuses a request with no panel session
    Given a request with no panel session
    When the request asks a management endpoint for a new local API key
    Then acp2api answers with an HTTP status from 400 to 499
    And the local API key is still "K1"

  @FR-009 @issue-55 @wip
  Scenario: A correct management password gives a panel session cookie
    When the owner enters the correct management password
    Then acp2api gives a panel session cookie with the attributes "HttpOnly" and "SameSite=Strict"

  @FR-009 @issue-55 @wip
  Scenario: A wrong management password gives no panel session
    When the owner enters a wrong management password
    Then acp2api gives no panel session cookie

  @FR-009 @issue-55 @wip
  Scenario Outline: The age of a panel session decides if it is valid
    Given the owner opened a panel session
    When <age> pass with no restart of acp2api
    Then the panel session is <validity>

    Examples:
      | age                   | validity       |
      | 11 hours 59 minutes   | still valid    |
      | 12 hours              | no longer valid |

  @FR-009 @issue-55 @wip
  Scenario: A restart of acp2api ends a panel session
    Given the owner opened a panel session
    When the owner restarts acp2api
    Then the panel session is no longer valid

  @FR-009 @issue-55 @wip
  Scenario: A change of the management password ends each panel session
    Given the owner has a panel session in the browser "B1" and a panel session in the browser "B2"
    When the owner changes the management password in the browser "B1"
    Then the panel session in the browser "B1" is no longer valid
    And the panel session in the browser "B2" is no longer valid

  @FR-009 @issue-55 @wip
  Scenario: acp2api keeps only a hash of a management password that the owner sets
    When the owner changes the management password to "P2"
    Then the configuration file does not hold "P2"
    And the configuration file holds a hash of the management password
    And "P2" opens the management panel
