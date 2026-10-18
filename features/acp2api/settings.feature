# language: en
Feature: Data folder and settings
  acp2api keeps its own files in the data folder.
  The configuration file holds the five settings of v1.
  These are the port, the concurrency limit, the local API key, the management password, and the path of agy_acp_server.
  In a quoted message, a value in angle brackets is a value that acp2api fills in, as the section "Message texts" of the PRD gives.

  @FR-003
  Scenario Outline: A first start puts the files of acp2api into the data folder of the platform
    Given a computer with <platform> and no data folder of acp2api
    When the owner starts acp2api with no option
    Then the configuration file "config.toml" and the logs are in the folder "<data folder>"

    Examples:
      | platform | data folder                            |
      | macOS    | ~/Library/Application Support/acp2api/ |
      | Windows  | %LOCALAPPDATA%\acp2api\                |

  @FR-003
  Scenario: The option --data-dir sets another data folder
    Given the folder "E" is empty
    When the owner starts acp2api with the option "--data-dir E"
    Then the configuration file "config.toml" and the logs are in the folder "E"
    And the data folder of the platform holds no file of acp2api

  @FR-004
  Scenario: A first start writes the default settings
    Given the data folder "D" is empty
    When the owner starts acp2api with the data folder "D"
    Then the configuration file has the port 8329 and the concurrency limit 2
    And the configuration file has no path of agy_acp_server

  @FR-004
  Scenario: The option --port sets the port for the start
    Given the configuration file has the port 8329
    When the owner starts acp2api with the option "--port 8400"
    Then acp2api answers requests on the port 8400
    And acp2api accepts no connection on the port 8329

  @FR-004
  Scenario: A setting that the owner edits while acp2api is stopped applies at the next start
    Given acp2api is stopped
    And the owner sets the port 8400 in the configuration file
    When the owner starts acp2api
    Then acp2api answers requests on the port 8400

  @FR-004
  Scenario: A change in the management panel goes into the configuration file with no restart
    Given acp2api runs with the concurrency limit 2
    And the owner has a panel session
    When the owner sets the concurrency limit 3 in the management panel
    Then the configuration file has the concurrency limit 3
    And the panel session of the owner is still valid

  @FR-004
  Scenario: Only the computer account of the owner can read the configuration file
    Given acp2api made the configuration file
    Then the file permissions of the configuration file give no other computer account read access

  @FR-004
  Scenario: A start with the port in use fails
    Given the configuration file has the port 8329
    And another program listens on the port 8329
    When the owner starts acp2api
    Then the start fails with the message "acp2api: the port 8329 is in use. Set another port in the configuration file or with --port."
    And acp2api listens on no other port

  @FR-004
  Scenario: A start with a setting that acp2api cannot use fails
    Given the configuration file has the concurrency limit 4
    When the owner starts acp2api
    Then the start fails with the message "acp2api: the setting <setting> in <path of the configuration file> has a value that acp2api cannot use."
    And the message names the setting of the concurrency limit and the path of the configuration file

  @FR-004
  Scenario: A start with no field of the management password makes a new management password
    Given acp2api is stopped
    And the owner removed the field of the management password from the configuration file
    When the owner starts acp2api
    Then the terminal shows a new random management password
    And the new management password opens the management panel
    And the earlier management password does not open the management panel
