@integration @service-enablement
Feature: Service enablement functionality
  As an admin
  I want to selectively enable/disable services when starting the LNURL server
  So that I can run only the services I need for specific deployment scenarios

  # This feature tests the ability to selectively start services based on
  # enablement flags, allowing for flexible deployment configurations

  Background:
    Given the server is not already running

  @single-service @lnurl-only
  Scenario: Start only LNURL service
    # Test starting only the LNURL service with minimal configuration
    Given a configuration file exists with only lnurl service defined
    When I start the LNURL server with enablement flag "lnurl"
    Then the server should start successfully
    And the lnurl service should be listening on the configured port
    And the discovery service should not be listening on the configured port
    And the offers service should not be listening on the configured port
    When I send a SIGTERM signal to the server process
    Then the server should exit with code 0
    And no error logs should be present

  @single-service @backend-only
  Scenario: Start only backend (discovery) service  
    # Test starting only the backend/discovery service with minimal configuration
    Given a configuration file exists with only discovery service defined
    When I start the LNURL server with enablement flag "backend"
    Then the server should start successfully
    And the lnurl service should not be listening on the configured port
    And the discovery service should be listening on the configured port
    And the offers service should not be listening on the configured port
    When I send a SIGTERM signal to the server process
    Then the server should exit with code 0
    And no error logs should be present

  @single-service @offer-only
  Scenario: Start only offers service
    # Test starting only the offers service with minimal configuration
    Given a configuration file exists with only offers service defined
    When I start the LNURL server with enablement flag "offer"
    Then the server should start successfully
    And the lnurl service should not be listening on the configured port
    And the discovery service should not be listening on the configured port
    And the offers service should be listening on the configured port
    When I send a SIGTERM signal to the server process
    Then the server should exit with code 0
    And no error logs should be present
