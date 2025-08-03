@integration @backend-management
Feature: Backend enable/disable functionality
  As an admin
  I want to use the backend API to disable and enable lightning node backends
  So that I can perform maintenance on lightning nodes without affecting other nodes

  # This feature tests the ability to dynamically enable and disable lightning node backends
  # through the discovery API, ensuring that invoice generation works correctly based on
  # backend availability while maintaining offer accessibility

  Background:
    Given the payee has access to both CLN and LND lightning nodes
    And all services should be listening on their configured ports
    And the payee has created an offer linked to both lightning nodes
    And both nodes are registered as separate backends
    And the system waits for backend readiness

  @backend-lifecycle @complete-workflow
  Scenario: Complete backend lifecycle management
    # Test the complete workflow: working → partial → failure → recovery
    Given the payer can generate invoices successfully
    When the admin disables the first backend
    Then the payer can still generate invoices
    When the admin disables the second backend
    Then the payer cannot generate invoices
    When the admin enables any backend
    Then the payer can again generate invoices