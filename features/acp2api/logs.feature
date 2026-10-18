# language: en
Feature: Logs
  A log holds only metadata. No log line, request line of the terminal, or state file holds the content of a request or a secret.
  The logs have a total size cap of 50 MB, and acp2api deletes each log file that is older than 30 days.

  Background:
    Given acp2api is in the normal state
    And the official runtime offers the model name "gemini-3.8-flash"

  @FR-056
  Scenario: A request gives one log line with its metadata
    When an API client sends a valid request to "POST /v1/chat/completions" with the model "gemini-3.8-flash" and the unknown field "example_field"
    Then the log has one line for the request with these fields:
      | field                         |
      | the time                      |
      | the request ID                |
      | the API                       |
      | the model name                |
      | a short ID of the ACP session |
      | new, continue, or rebuild     |
      | the duration                  |
      | the usage                     |
      | the result                    |
      | the error code                |
      | the unknown field names       |
    And the unknown field names of the line hold "example_field"

  @FR-056
  Scenario Outline: The log line shows how the request used an ACP session
    Given <history>
    When an API client sends that request
    Then the log line of the request has "<kind>"

    Examples:
      | history                                                      | kind     |
      | the request starts a new conversation                         | new      |
      | the request adds one user message to the last conversation    | continue |
      | the request has an edited earlier message                     | rebuild  |

  @FR-056
  Scenario: A change of state gives one log line
    When a quota signal starts a quota wait
    Then the log has one line for the change to the state "quota wait"

  @FR-056
  Scenario: No log, terminal, or state file holds the content of a request
    When an API client sends a request with the marker "MARKER-7Q" in the message text, the system prompt, a tool argument, and a tool result
    And a lockout signal then starts a lockout
    Then the logs, the terminal, and the state file do not hold "MARKER-7Q"

  @FR-056
  Scenario: No log line or state file holds the local API key or the management password
    Given the local API key is "K1" and the management password is "P1"
    When an API client sends requests with "K1", and the owner opens the management panel with "P1"
    And a lockout signal then starts a lockout
    Then the logs, the request lines of the terminal, and the state file do not hold "K1" or "P1"

  @FR-056
  Scenario: acp2api writes only the parsed error line of the stderr of the official runtime
    Given the stderr of the official runtime has the line "I1018 harness: MARKER-STDERR"
    And the stderr then has the error line with the code 503, the status "UNAVAILABLE", and the message "The service is currently unavailable."
    When an API client sends a valid request
    Then the log holds the code 503, the status "UNAVAILABLE", and the message "The service is currently unavailable."
    And the log does not hold "MARKER-STDERR"

  @FR-056
  Scenario Outline: acp2api replaces each email address in an error text with <REDACTED>
    Given the stderr of the official runtime has an error line with the message "Account <address> is not valid"
    When an API client sends a valid request
    Then the log holds "Account <REDACTED> is not valid"
    And the log does not hold "<address>"

    Examples:
      | address            |
      | name@example.com   |
      | name%40example.com |

  @FR-057
  Scenario: The logs keep a total size cap of 50 MB and delete the oldest log file first
    When acp2api writes 60 MB of log lines
    Then the logs take no more than 50 MB
    And the log file that acp2api deleted is the oldest log file

  @FR-057
  Scenario Outline: A start deletes each log file that is older than 30 days
    Given the log folder has a log file with a date <age> ago
    When the owner starts acp2api
    Then that log file is <result>

    Examples:
      | age     | result      |
      | 31 days | gone        |
      | 29 days | still there |
