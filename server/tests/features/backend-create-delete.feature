@integration @backend-management
Feature: Backend create/delete functionality
  As an admin
  I want to use the backend API to create and delete lightning node backends
  So that I can dynamically manage lightning nodes without affecting other nodes

  # This feature tests the ability to dynamically create and delete lightning node backends
  # through the discovery API, ensuring that invoice generation works correctly based on
  # backend availability while maintaining offer accessibility

  Background:
    Given the payee has access to both CLN and LND lightning nodes
    And all services should be listening on their configured ports
    And the payee has created an offer linked to both lightning nodes
    And both nodes are ready to be registered as backends

  @backend-lifecycle @complete-workflow
  Scenario: Complete backend lifecycle management
    # Test the complete workflow: working → partial → failure → recovery
    Given both backends are created and the payer can generate invoices successfully
    And the system waits for backend readiness
    When the admin deletes the first backend
    Then the payer can still generate invoices
    When the admin deletes the second backend
    Then the payer cannot generate invoices
    When the admin creates any backend again
    And the system waits for backend readiness
    Then the payer can again generate invoices