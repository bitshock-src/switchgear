@cli @discovery-manage
Feature: Discovery CLI management
  As an admin
  I want to manage backends via discovery CLI
  So that I can administer the discovery service

  # This feature tests the CLI commands for managing backends
  # via the discovery service, including create, read, update, and delete operations

  Background:
    Given the swgr CLI is available

  @discovery-new
  Scenario Outline: Generate <node_type> backend JSON
    When I run "swgr discovery new" for <node_type>
    Then the command should succeed
    And valid backend JSON should be output to stdout

    Examples:
      | node_type |
      | cln-grpc  |
      | lnd-grpc  |

  @discovery-new-with-output
  Scenario: Generate backend JSON with output file
    When I run "swgr discovery new" for lnd-grpc with output path
    Then the command should succeed
    And the backend JSON file should exist

  @discovery-post
  Scenario: Load a new backend
    Given the lnurl server is ready to start
    When I start the lnurl server with the configuration
    Then the server should start successfully
    Given a valid backend JSON exists
    When I run "swgr discovery post" with backend JSON
    Then the command should succeed

  @discovery-post-conflict
  Scenario: Post a duplicate backend returns conflict error
    Given the lnurl server is ready to start
    When I start the lnurl server with the configuration
    Then the server should start successfully
    Given a valid backend JSON exists
    When I run "swgr discovery post" with backend JSON
    Then the command should succeed
    When I run "swgr discovery post" with backend JSON
    Then the command should fail
    And a conflict message should be shown

  @discovery-ls
  Scenario: List all backends
    Given the lnurl server is ready to start
    When I start the lnurl server with the configuration
    Then the server should start successfully
    Given a valid backend JSON exists
    When I run "swgr discovery post" with backend JSON
    Then the command should succeed
    When I run "swgr discovery ls"
    Then the command should succeed
    And backend list should be output

  @discovery-get
  Scenario: Get a backend
    Given the lnurl server is ready to start
    When I start the lnurl server with the configuration
    Then the server should start successfully
    Given a valid backend JSON exists
    When I run "swgr discovery post" with backend JSON
    Then the command should succeed
    When I run "swgr discovery get" for backend address
    Then the command should succeed
    And backend details should be output

  @discovery-get-all
  Scenario: Get all backends
    Given the lnurl server is ready to start
    When I start the lnurl server with the configuration
    Then the server should start successfully
    Given a valid backend JSON exists
    When I run "swgr discovery post" with backend JSON
    Then the command should succeed
    When I run "swgr discovery get"
    Then the command should succeed
    And all backends should be output

  @discovery-put
  Scenario: Update a backend
    Given the lnurl server is ready to start
    When I start the lnurl server with the configuration
    Then the server should start successfully
    Given a valid backend JSON exists
    When I run "swgr discovery post" with backend JSON
    Then the command should succeed
    And updated backend JSON exists
    When I run "swgr discovery put" with backend address and JSON
    Then the command should succeed
    When I run "swgr discovery get" for backend address
    Then the command should succeed
    And the backend should contain the updated data

  @discovery-delete
  Scenario: Delete a backend
    Given the lnurl server is ready to start
    When I start the lnurl server with the configuration
    Then the server should start successfully
    Given a valid backend JSON exists
    When I run "swgr discovery post" with backend JSON
    Then the command should succeed
    When I run "swgr discovery delete" for backend address
    Then the command should succeed
    When I run "swgr discovery get" for backend address
    Then the command should fail
    And the backend should not be found

  @discovery-patch
  Scenario: Patch a backend
    Given the lnurl server is ready to start
    When I start the lnurl server with the configuration
    Then the server should start successfully
    Given a valid backend JSON exists
    When I run "swgr discovery post" with backend JSON
    Then the command should succeed
    And backend patch JSON exists
    When I run "swgr discovery patch" with backend address and patch JSON
    Then the command should succeed
    When I run "swgr discovery get" for backend address
    Then the command should succeed
    And the backend should contain the patched data

  @discovery-enable
  Scenario: Enable a backend
    Given the lnurl server is ready to start
    When I start the lnurl server with the configuration
    Then the server should start successfully
    Given a valid backend JSON exists
    When I run "swgr discovery post" with backend JSON
    Then the command should succeed
    When I run "swgr discovery disable" for backend address
    Then the command should succeed
    When I run "swgr discovery enable" for backend address
    Then the command should succeed
    When I run "swgr discovery get" for backend address
    Then the command should succeed
    And the backend should be enabled

  @discovery-disable
  Scenario: Disable a backend
    Given the lnurl server is ready to start
    When I start the lnurl server with the configuration
    Then the server should start successfully
    Given a valid backend JSON exists
    When I run "swgr discovery post" with backend JSON
    Then the command should succeed
    When I run "swgr discovery disable" for backend address
    Then the command should succeed
    When I run "swgr discovery get" for backend address
    Then the command should succeed
    And the backend should be disabled

  @discovery-get-error
  Scenario: Get a non-existent backend returns error
    Given the lnurl server is ready to start
    When I start the lnurl server with the configuration
    Then the server should start successfully
    When I run "swgr discovery get" for non-existent backend address
    Then the command should fail
    And the backend should not be found

  @discovery-patch-error
  Scenario: Patch a non-existent backend returns error
    Given the lnurl server is ready to start
    When I start the lnurl server with the configuration
    Then the server should start successfully
    And backend patch JSON exists
    When I run "swgr discovery patch" for non-existent backend address
    Then the command should fail
    And the backend should not be found

  @discovery-enable-error
  Scenario: Enable a non-existent backend returns error
    Given the lnurl server is ready to start
    When I start the lnurl server with the configuration
    Then the server should start successfully
    When I run "swgr discovery enable" for non-existent backend address
    Then the command should fail
    And the backend should not be found

  @discovery-disable-error
  Scenario: Disable a non-existent backend returns error
    Given the lnurl server is ready to start
    When I start the lnurl server with the configuration
    Then the server should start successfully
    When I run "swgr discovery disable" for non-existent backend address
    Then the command should fail
    And the backend should not be found

  @discovery-delete-error
  Scenario: Delete a non-existent backend returns error
    Given the lnurl server is ready to start
    When I start the lnurl server with the configuration
    Then the server should start successfully
    When I run "swgr discovery delete" for non-existent backend address
    Then the command should fail
    And the backend should not be found
