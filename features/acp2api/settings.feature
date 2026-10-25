# language: en
Feature: Data folder and settings
  acp2api keeps its own files in the data folder.
  The configuration file holds the five settings of v1.
  These are the port, the concurrency limit, the local API key, the management password, and the path of agy_acp_server.
  In a quoted message, a value in angle brackets is a value that acp2api fills in, as the section "Message texts" of the PRD gives.

  @FR-003 @issue-37
  Scenario Outline: A first start puts the files of acp2api into the data folder of the platform
    Given a computer with <platform> and no data folder of acp2api
    When the owner starts acp2api with no option
    Then the configuration file "config.toml" and the logs are in the folder "<data folder>"

    Examples:
      | platform | data folder                            |
      | macOS    | ~/Library/Application Support/acp2api/ |
      | Windows  | %LOCALAPPDATA%\\acp2api\\                |

  @FR-003 @issue-37
  Scenario: The option --data-dir sets another data folder
    Given the folder "E" is empty
    When the owner starts acp2api with the option "--data-dir E"
    Then the configuration file "config.toml" and the logs are in the folder "E"
    And the data folder of the platform holds no file of acp2api

  @FR-004 @issue-37
  Scenario: A first start writes the default settings
    Given the data folder "D" is empty
    And the platform maximum concurrency limit is 3
    When the owner starts acp2api with the data folder "D"
    Then the configuration file has the port 8329 and the concurrency limit 2
    And the configuration file has no path of agy_acp_server

  @FR-004 @issue-37
  Scenario: The option --port sets the port for the start
    Given the configuration file has the port 8329
    When the owner starts acp2api with the option "--port 8400"
    Then acp2api answers requests on the port 8400
    And acp2api accepts no connection on the port 8329

  @FR-004 @issue-37
  Scenario: A setting that the owner edits while acp2api is stopped applies at the next start
    Given acp2api is stopped
    And the owner sets the port 8400 in the configuration file
    When the owner starts acp2api
    Then acp2api answers requests on the port 8400

  @FR-004 @issue-55
  Scenario: A change in the management panel goes into the configuration file with no restart
    Given acp2api runs with the concurrency limit 2
    And the platform maximum concurrency limit is 3
    And the owner has a panel session
    When the owner sets the concurrency limit 3 in the management panel
    Then the configuration file has the concurrency limit 3
    And the panel session of the owner is still valid

  @FR-004 @issue-37
  Scenario: Only the computer account of the owner can read the configuration file
    Given acp2api made the configuration file
    Then the file permissions of the configuration file give no other computer account read access

  @FR-004 @issue-37
  Scenario: A start with the port in use fails
    Given the configuration file has the port 8329
    And another program listens on the port 8329
    When the owner starts acp2api
    Then the start fails with the message "acp2api: the port 8329 is in use. Set another port in the configuration file or with --port."
    And acp2api listens on no other port

  @FR-004 @issue-37
  Scenario: A start with a setting that acp2api cannot use fails
    Given the configuration file has the concurrency limit 4
    When the owner starts acp2api
    Then the start fails with the message "acp2api: the setting <setting> in <path of the configuration file> has a value that acp2api cannot use."
    And the message names the setting of the concurrency limit and the path of the configuration file

  @FR-004 @issue-37
  Scenario: An unknown key stops the start
    Given the configuration file has the unknown key "concurency_limit" with the value 2
    When the owner starts acp2api
    Then the start fails with the message "acp2api: the setting concurency_limit in <path of the configuration file> has a value that acp2api cannot use."
    And acp2api accepts no API request

  @FR-004 @issue-37
  Scenario Outline: A TOML syntax error stops the start and identifies its location
    Given the configuration file has these lines:
      """
      # Settings
      <invalid line>
      """
    When the owner starts acp2api
    Then the start fails with the message "acp2api: the setting <location> in <path of the configuration file> has a value that acp2api cannot use."
    And acp2api accepts no API request

    Examples:
      | invalid line | location  | path of the configuration file |
      | port = @     | port      | <path of the configuration file> |
      | = 8329       | at line 2 | <path of the configuration file> |

  @FR-004 @issue-37
  Scenario Outline: An absent port or concurrency limit takes its default
    Given acp2api is stopped
    And the platform maximum concurrency limit is 3
    And the configuration file has no field for the <setting>
    When the owner starts acp2api with no option that overrides that setting
    Then acp2api uses the <setting> <value>

    Examples:
      | setting           | value |
      | port              | 8329  |
      | concurrency limit | 2     |

  @FR-004 @FR-049 @issue-37
  Scenario Outline: The default concurrency limit respects the platform maximum
    Given the platform maximum concurrency limit is <maximum>
    And the configuration file has no concurrency limit
    When the owner starts acp2api
    Then the configuration file has the concurrency limit <default>

    Examples:
      | maximum | default |
      | 1       | 1       |
      | 2       | 2       |
      | 3       | 2       |

  @FR-004 @issue-37
  Scenario: The option --port overrides an absent port setting
    Given the configuration file has no field for the port
    When the owner starts acp2api with the option "--port 8400"
    Then acp2api answers requests on the port 8400
    And acp2api accepts no connection on the port 8329

  @FR-004 @issue-55
  Scenario: An absent local API key gives a new saved key
    Given acp2api is stopped
    And the owner removed the local API key "K1" from the configuration file
    And the configuration file still has the hash of the management password "P1"
    And an accepted runtime build is available for the credential check
    When the owner starts acp2api
    Then the terminal shows a new random local API key
    And the configuration file holds that new key
    And a request with that new key gets no HTTP 401
    And a request with the local API key "K1" gets HTTP 401
    And the terminal does not show "P1"

  @FR-049 @issue-60
  Scenario: A concurrency limit above the highest value of the platform stops the start
    Given the build of acp2api for this platform has the highest concurrency limit 2, as its release notes name
    And the configuration file has the concurrency limit 3
    When the owner starts acp2api
    Then the start fails with the message "acp2api: the setting <setting> in <path of the configuration file> has a value that acp2api cannot use."
    And the message names the setting of the concurrency limit and the path of the configuration file

  @FR-004 @issue-55
  Scenario: A start with no field of the management password makes a new management password
    Given acp2api is stopped
    And the owner removed the field of the management password from the configuration file
    When the owner starts acp2api
    Then the terminal shows a new random management password
    And the new management password opens the management panel
    And the earlier management password does not open the management panel
    And the configuration file holds only the hash of the new management password
