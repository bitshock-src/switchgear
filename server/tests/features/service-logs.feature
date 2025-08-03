@integration @service-logs
Feature: Service logs functionality
  As an admin
  I want to see server logs
  So that I can troubleshoot and monitor services

  # This feature tests the logging functionality of the LNURL server
  # Following pattern: perform work -> stop service -> assert logs

  Background:
    Given a valid configuration file exists
    And the server is not already running

  @service-health-logs
  Scenario: Service health check logging
    # Test that health check requests are logged with expected patterns
    Given a valid configuration file exists
    And the log level is set to "info" in the configuration
    And the single payee has a CLN lightning node available
    When I start the LNURL server with the configuration
    Then the server should start successfully
    And the lnurl service should be listening on the configured port
    And the discovery service should be listening on the configured port
    And the offers service should be listening on the configured port
    When I send a SIGTERM signal to the server process
    Then the server should exit with code 0
    And the server logs should contain health check requests for all services

  @service-operations-logs  
  Scenario: Service operation request logging
    # Test that service operations produce appropriate request logs
    Given a valid configuration file exists
    And the log level is set to "info" in the configuration
    And the single payee has a CLN lightning node available
    When I start the LNURL server with the configuration
    Then the server should start successfully
    And the lnurl service should be listening on the configured port
    And the discovery service should be listening on the configured port
    And the offers service should be listening on the configured port
    When the single payee creates an offer for their lightning node
    When the single payee registers their lightning node as a backend
    And the system waits for backend readiness
    When the payer requests the LNURL offer from the single payee
    When the payer requests an invoice for 100 sats using the single payee's callback URL
    When I send a SIGTERM signal to the server process
    Then the server should exit with code 0
    And the server logs should contain backend registration requests
    And the server logs should contain offer retrieval requests
    And the server logs should contain invoice generation requests

  @error-logging
  Scenario: Error conditions are properly logged
    # Test that errors are logged appropriately
    Given a valid configuration file exists
    And the log level is set to "warn" in the configuration
    When I start the LNURL server with the configuration
    Then the server should start successfully
    And the lnurl service should be listening on the configured port
    When I make a request to a non-existent endpoint
    When I request an invoice for a non-existent offer
    When I try to register an invalid backend
    When I send a SIGTERM signal to the server process
    Then the server should exit with code 0
    And the server logs should contain 404 error responses
    And the server logs should contain invalid offer error responses
    And the server logs should contain invalid backend registration errors