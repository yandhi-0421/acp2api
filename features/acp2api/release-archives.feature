# language: en
Feature: Release archives
  Each release of acp2api gives one archive for each target platform on GitHub Releases.
  The owner unpacks the archive. The project has no installer.

  @FR-001 @issue-56 @wip
  Scenario: A release has one archive for each target platform
    Given a release of acp2api on GitHub Releases
    Then the release has one archive for each of these platforms:
      | platform      |
      | macOS arm64   |
      | macOS x64     |
      | Windows x64   |
      | Windows arm64 |
    And each archive holds one executable of acp2api

  @FR-001 @issue-56 @wip
  Scenario Outline: The executable of each archive starts on its platform
    Given the unpacked archive of a release for <platform>
    When the owner starts its executable on a computer with <platform>
    Then the terminal shows the address of the management panel

    Examples:
      | platform      |
      | macOS arm64   |
      | macOS x64     |
      | Windows x64   |
      | Windows arm64 |
