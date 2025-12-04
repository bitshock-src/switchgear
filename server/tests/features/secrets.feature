@integration @server-persistence
Feature: Server handles secrets files
  As an admin
  I want to restart the LNURL server and have it read database secrets for offers and backends
  So that the server can serve offers and invoices to payees after restarts

  Background:
    Given the payee has a CLN lightning node available
    And the server is not already running

  @persistence @secrets-validation @happy-path
  Scenario: Server startup succeeds with valid secrets file
    Given a valid configuration file exists with good secrets configuration
    When I start the LNURL server with the configuration
    Then the server should start successfully
    And all services should be listening on their configured ports
    When I send a SIGTERM signal to the server process
    Then the server should exit with code 0

  @persistence @secrets-validation @negative
  Scenario: Server startup fails with missing secrets file
    Given a valid configuration file exists with missing-file secrets configuration
    When I start the LNURL server with the configuration
    Then the server should fail to start
    And an error message should be displayed
    And the server should exit with a non-zero code

  @persistence @secrets-validation @negative
  Scenario: Server startup fails with invalid secrets file
    Given a valid configuration file exists with invalid-file secrets configuration
    When I start the LNURL server with the configuration
    Then the server should fail to start
    And an error message should be displayed
    And the server should exit with a non-zero code

  @persistence @secrets-validation @negative
  Scenario: Server startup fails with missing secret in file
    Given a valid configuration file exists with missing-secret secrets configuration
    When I start the LNURL server with the configuration
    Then the server should fail to start
    And an error message should be displayed
    And the server should exit with a non-zero code

      
