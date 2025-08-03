@integration @configuration
Feature: Configuration validation
  As an admin
  I want the server to validate configuration files
  So that invalid configurations are rejected early
  
  # This feature tests the server's ability to detect and reject
  # invalid configuration files during startup

  Background:
    Given the server is not already running

  @negative @error-handling
  Scenario: Invalid configuration file is rejected
    # Test that malformed configuration files are properly rejected
    Given an invalid configuration file exists
    When I start the LNURL server with the configuration
    Then the server should fail to start
    And an error message should be displayed
    And the server should exit with a non-zero code