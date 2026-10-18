# language: en
Feature: Builds of the official runtime
  acp2api searches for the official runtime, and it never downloads it.
  acp2api starts each runtime process only from its own copy of an accepted version in the data folder.
  A compatibility check on the computer of the owner can make another build an accepted version.

  @FR-004 @FR-042 @issue-58 @wip
  Scenario Outline: A start finds a build in each place that acp2api searches
    Given acp2api has no copy of the official runtime
    And the configuration file has no path of agy_acp_server
    And the only build of the official runtime is in <place>
    When the owner starts acp2api
    Then the result of the search lists the build in <place>

    Examples:
      | place                                 |
      | the folder of the acp2api executable  |
      | a folder of PATH                      |
      | the install folder of Zed             |
      | the install folder of a JetBrains IDE |

  @FR-042 @issue-58 @wip
  Scenario: A path that the owner gives finds the build
    Given acp2api found no build of the official runtime
    When the owner sets the path of agy_acp_server to a build in the folder "F" in the management panel
    Then the result of the search lists the build in the folder "F"

  @FR-042 @issue-58 @wip
  Scenario: A new search finds a build that the owner added after the start
    Given acp2api found no build of the official runtime
    And the owner then puts a build of an accepted version into the install folder of Zed
    When the owner starts a new search in the management panel
    Then the result of the search lists the build in the install folder of Zed

  @FR-042 @issue-58 @wip
  Scenario: The search ignores a folder without localharness_external
    Given acp2api has no copy of the official runtime
    And the install folder of Zed holds agy_acp_server of an accepted version and no localharness_external
    When the owner starts acp2api
    Then the result of the search does not list the install folder of Zed
    And acp2api is in the state "runtime unavailable"

  @FR-042 @issue-58 @wip
  Scenario: A path without localharness_external keeps the earlier path
    Given the path of agy_acp_server points to the folder "F1"
    When the owner sets the path of agy_acp_server to the folder "F2", which has no localharness_external, in the management panel
    Then the management panel shows "This folder has no localharness_external, so acp2api did not use this path."
    And the path of agy_acp_server still points to the folder "F1"

  @FR-043 @issue-58 @wip
  Scenario: With no copy, acp2api copies the found build with the highest version on the list
    Given acp2api has no copy of the official runtime
    And acp2api finds the builds 1.4.0 and 1.5.0, and both are on the list of the release
    When the owner starts acp2api
    Then the data folder holds one copy of the official runtime, of the build 1.5.0
    And the runtime process runs from that copy

  @FR-043 @issue-58 @wip
  Scenario: A new build with a higher version shows new build found and does not change the build in use
    Given the runtime process runs from the copy of the build 1.4.0
    And the owner puts the accepted build 1.5.0 into the install folder of Zed
    When acp2api searches again
    Then the management panel shows "New build found: 1.5.0"
    And the build in use is 1.4.0

  @FR-043 @issue-58 @wip
  Scenario: An older found build shows only in the result of the search
    Given the runtime process runs from the copy of the build 1.5.0
    And the install folder of Zed holds the accepted build 1.4.0
    When acp2api searches again
    Then the result of the search lists the build 1.4.0
    And the management panel shows no "New build found"

  @FR-043 @issue-58 @wip
  Scenario: The owner changes to another accepted build in the management panel
    Given the runtime process runs from the copy of the build 1.4.0
    And acp2api found the accepted build 1.5.0
    When the owner changes the build to 1.5.0 in the management panel
    Then acp2api replaces the runtime process with a runtime process of the build 1.5.0
    And the data folder holds one copy of the official runtime, of the build 1.5.0

  @FR-043 @issue-58 @wip
  Scenario: A build that is not on the list gets no copy without a compatibility check
    Given acp2api has no copy of the official runtime
    And the only found build is 1.6.0, which is not on the list of the release
    When the owner starts acp2api
    Then the data folder holds no copy of the official runtime
    And acp2api is in the state "runtime unavailable"

  @FR-043 @issue-58 @wip
  Scenario Outline: A changed byte in the copy stops the start
    Given the copy of the official runtime in the data folder has one changed byte
    And <other builds>
    When the owner starts acp2api
    Then no runtime process starts from the changed copy
    And <result>

    Examples:
      | other builds                                             | result                                                                                                          |
      | the install folder of Zed holds the accepted build 1.4.0 | acp2api copies the build 1.4.0 from the install folder of Zed, and the runtime process runs from the new copy     |
      | no other place holds a build of the official runtime     | acp2api is in the state "runtime unavailable" with the reason "no build that acp2api found is an accepted version" |

  @FR-043 @issue-58 @wip
  Scenario: A build for another CPU architecture is not accepted
    Given acp2api for macOS arm64 runs
    And it has no copy of an accepted version and no usable runtime process
    And the only found build is a build for macOS x64 whose hashes are on the list of the release
    When acp2api searches again
    Then that build is not an accepted version
    And acp2api is in the state "runtime unavailable"

  @FR-044 @issue-62 @wip
  Scenario: The management panel shows the cost and the unchecked risks before a compatibility check
    Given acp2api found the build 1.6.0, which is not an accepted version
    When the owner opens the compatibility check for 1.6.0 in the management panel
    Then the management panel shows the cost of 3 to 4 model requests through the Google account of the owner
    And the management panel shows these risks that the check does not check:
      | risk                                       |
      | the stderr lines of the lockout signals    |
      | three prompt turns at the same time        |
      | the memory that decides N                  |

  @FR-044 @issue-62 @wip
  Scenario: A compatibility check runs as a replacement of the runtime process, and new requests wait in the queue
    Given the runtime process runs from the copy of the build 1.4.0
    When the owner starts the compatibility check for the build 1.6.0
    And an API client sends a request during the check
    Then the request waits in the queue until the end of the check

  @FR-044 @issue-62 @wip
  Scenario: A passed compatibility check makes the build an accepted version on this computer
    Given acp2api found the build 1.6.0, and the official runtime of that build gives each fact that acp2api needs
    When the owner runs the compatibility check for 1.6.0
    Then the check passes
    And the state file holds the two hashes and the version of the build 1.6.0
    And the management panel shows the build 1.6.0 with the label "Checked on this computer"
    And the management panel shows each build of the list of the release with the label "On the release list"

  @FR-044 @issue-62 @wip
  Scenario: A failed compatibility check starts the earlier accepted version again
    Given the runtime process runs from the copy of the build 1.4.0
    And the official runtime of the found build 1.6.0 runs a built-in tool in the check
    When the owner runs the compatibility check for 1.6.0
    Then the check fails
    And the runtime process runs from the build 1.4.0 again
    And the build 1.6.0 is not an accepted version
