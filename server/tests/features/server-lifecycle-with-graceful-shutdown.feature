@integration @server-lifecycle
Feature: Server starts and shuts down cleanly with signal
  As an admin
  I want to start the LNURL server and shut it down with a signal
  So that the server exits cleanly without errors
  
  # This feature tests the complete lifecycle of the LNURL server process
  # including startup, health checks, signal handling, and graceful shutdown

  Background:
    Given the server is not already running

  @graceful-shutdown
  Scenario Outline: Start server and shutdown with <signal>
    # Test graceful shutdown using different signals
    Given a valid configuration file exists
    And the LNURL server is ready to start
    When I start the LNURL server with the configuration
    Then the server should start successfully
    And all services should be listening on their configured ports
    When I send a <signal> signal to the server process
    Then the server should begin graceful shutdown
    And the server should stop accepting new connections
    And the server should complete all pending requests
    And the server should exit with code 0
    And no error logs should be present

    @sigterm @sigint
    Examples:
      | signal  |
      | SIGTERM |
      | SIGINT  |