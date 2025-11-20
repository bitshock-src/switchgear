@cli @offer-manage
Feature: Offer CLI management
  As an admin
  I want to manage offers via offer CLI
  So that I can administer the offer service

  # This feature tests the CLI commands for managing offers
  # via the offer service, including create, read, update, and delete operations

  Background:
    Given the swgr CLI is available

  @offer-new
  Scenario: Generate offer JSON
    When I run "swgr offer new"
    Then the command should succeed
    And valid offer JSON should be output to stdout

  @offer-new-with-output
  Scenario: Generate offer JSON with output file
    When I run "swgr offer new" with output path
    Then the command should succeed
    And the offer JSON file should exist

  @offer-post
  Scenario: Load a new offer
    Given the lnurl server is ready to start
    When I start the lnurl server with the configuration
    Then the server should start successfully
    Given a valid offer JSON exists
    When I run "swgr offer post" with offer JSON
    Then the command should succeed

  @offer-get
  Scenario: Get an offer
    Given the lnurl server is ready to start
    When I start the lnurl server with the configuration
    Then the server should start successfully
    Given a valid offer JSON exists
    When I run "swgr offer post" with offer JSON
    Then the command should succeed
    When I run "swgr offer get" for offer ID
    Then the command should succeed
    And offer details should be output

  @offer-get-all
  Scenario: Get all offers
    Given the lnurl server is ready to start
    When I start the lnurl server with the configuration
    Then the server should start successfully
    Given a valid offer JSON exists
    When I run "swgr offer post" with offer JSON
    Then the command should succeed
    When I run "swgr offer get"
    Then the command should succeed
    And all offers should be output

  @offer-put
  Scenario: Update an offer
    Given the lnurl server is ready to start
    When I start the lnurl server with the configuration
    Then the server should start successfully
    Given a valid offer JSON exists
    When I run "swgr offer post" with offer JSON
    Then the command should succeed
    And updated offer JSON exists
    When I run "swgr offer put" with offer ID and JSON
    Then the command should succeed
    When I run "swgr offer get" for offer ID
    Then the command should succeed
    And the offer should contain the updated data

  @offer-delete
  Scenario: Delete an offer
    Given the lnurl server is ready to start
    When I start the lnurl server with the configuration
    Then the server should start successfully
    Given a valid offer JSON exists
    When I run "swgr offer post" with offer JSON
    Then the command should succeed
    When I run "swgr offer delete" for offer ID
    Then the command should succeed
    When I run "swgr offer get" for offer ID
    Then the command should fail
    And the offer should not be found

  @offer-metadata-new
  Scenario: Generate offer metadata JSON
    When I run "swgr offer metadata new"
    Then the command should succeed
    And valid offer metadata JSON should be output to stdout

  @offer-metadata-new-with-output
  Scenario: Generate offer metadata JSON with output file
    When I run "swgr offer metadata new" with output path
    Then the command should succeed
    And the offer metadata JSON file should exist

  @offer-metadata-post
  Scenario: Load new offer metadata
    Given the lnurl server is ready to start
    When I start the lnurl server with the configuration
    Then the server should start successfully
    Given a valid offer metadata JSON exists
    When I run "swgr offer metadata post" with metadata JSON
    Then the command should succeed

  @offer-metadata-get
  Scenario: Get offer metadata
    Given the lnurl server is ready to start
    When I start the lnurl server with the configuration
    Then the server should start successfully
    Given a valid offer metadata JSON exists
    When I run "swgr offer metadata post" with metadata JSON
    Then the command should succeed
    When I run "swgr offer metadata get" for metadata ID
    Then the command should succeed
    And offer metadata details should be output

  @offer-metadata-get-all
  Scenario: Get all offer metadata
    Given the lnurl server is ready to start
    When I start the lnurl server with the configuration
    Then the server should start successfully
    Given a valid offer metadata JSON exists
    When I run "swgr offer metadata post" with metadata JSON
    Then the command should succeed
    When I run "swgr offer metadata get"
    Then the command should succeed
    And all offer metadata should be output

  @offer-metadata-put
  Scenario: Update offer metadata
    Given the lnurl server is ready to start
    When I start the lnurl server with the configuration
    Then the server should start successfully
    Given a valid offer metadata JSON exists
    When I run "swgr offer metadata post" with metadata JSON
    Then the command should succeed
    And updated offer metadata JSON exists
    When I run "swgr offer metadata put" with metadata ID and JSON
    Then the command should succeed
    When I run "swgr offer metadata get" for metadata ID
    Then the command should succeed
    And the offer metadata should contain the updated data

  @offer-metadata-delete
  Scenario: Delete offer metadata
    Given the lnurl server is ready to start
    When I start the lnurl server with the configuration
    Then the server should start successfully
    Given a valid offer metadata JSON exists
    When I run "swgr offer metadata post" with metadata JSON
    Then the command should succeed
    When I run "swgr offer metadata delete" for metadata ID
    Then the command should succeed
    When I run "swgr offer metadata get" for metadata ID
    Then the command should fail
    And the offer metadata should not be found

  @offer-post-invalid-metadata
  Scenario: Attempt to post offer with non-existent metadata
    Given the lnurl server is ready to start
    When I start the lnurl server with the configuration
    Then the server should start successfully
    Given an offer JSON with non-existent metadata ID exists
    When I run "swgr offer post" with offer JSON
    Then the command should fail
    And a user error message should be shown

  @offer-metadata-delete-referenced
  Scenario: Attempt to delete metadata referenced by offer
    Given the lnurl server is ready to start
    When I start the lnurl server with the configuration
    Then the server should start successfully
    Given a valid offer JSON exists
    When I run "swgr offer post" with offer JSON
    Then the command should succeed
    When I run "swgr offer metadata delete" for metadata ID
    Then the command should fail
    And a user error message should be shown

  @offer-get-error
  Scenario: Get a non-existent offer returns error
    Given the lnurl server is ready to start
    When I start the lnurl server with the configuration
    Then the server should start successfully
    When I run "swgr offer get" for non-existent offer ID
    Then the command should fail
    And the offer should not be found

  @offer-delete-error
  Scenario: Delete a non-existent offer returns error
    Given the lnurl server is ready to start
    When I start the lnurl server with the configuration
    Then the server should start successfully
    When I run "swgr offer delete" for non-existent offer ID
    Then the command should fail
    And the offer should not be found

  @offer-post-conflict
  Scenario: Post a duplicate offer returns conflict error
    Given the lnurl server is ready to start
    When I start the lnurl server with the configuration
    Then the server should start successfully
    Given a valid offer JSON exists
    When I run "swgr offer post" with offer JSON
    Then the command should succeed
    When I run "swgr offer post" with offer JSON
    Then the command should fail
    And a conflict message should be shown

  @offer-metadata-get-error
  Scenario: Get a non-existent offer metadata returns error
    Given the lnurl server is ready to start
    When I start the lnurl server with the configuration
    Then the server should start successfully
    When I run "swgr offer metadata get" for non-existent metadata ID
    Then the command should fail
    And the offer metadata should not be found

  @offer-metadata-delete-error
  Scenario: Delete a non-existent offer metadata returns error
    Given the lnurl server is ready to start
    When I start the lnurl server with the configuration
    Then the server should start successfully
    When I run "swgr offer metadata delete" for non-existent metadata ID
    Then the command should fail
    And the offer metadata should not be found

  @offer-metadata-post-conflict
  Scenario: Post a duplicate offer metadata returns conflict error
    Given the lnurl server is ready to start
    When I start the lnurl server with the configuration
    Then the server should start successfully
    Given a valid offer metadata JSON exists
    When I run "swgr offer metadata post" with metadata JSON
    Then the command should succeed
    When I run "swgr offer metadata post" with metadata JSON
    Then the command should fail
    And a conflict message should be shown
