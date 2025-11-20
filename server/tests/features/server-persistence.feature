@integration @server-persistence
Feature: Server resumes state after restart
  As an admin
  I want to restart the LNURL server and have it remember offers and backends
  So that the server can continue to serve offers and invoices to payees after restarts

  Background:
    Given the payee has a CLN lightning node available
    And the server is not already running

  @persistence @backend-recovery @offer-recovery @full-lifecycle
  Scenario Outline: Complete persistence lifecycle across multiple server restarts with <backend_store>/<offer_store> storage
    Given a valid configuration file exists with <backend_store> backend storage and <offer_store> offer storage
    # First server instance: Create and persist data
    When I start the LNURL server with the configuration
    Then the server should start successfully
    And all services should be listening on their configured ports
    
    When the payee creates an offer for their lightning node
    And the payee registers their lightning node as a backend
    And the system waits for backend readiness
    
    When the payer requests the LNURL offer from the payee
    Then the offer should contain valid sendable amounts
    And the offer should contain valid metadata
    And the offer should provide a callback URL
    
    When the payer requests an invoice for 100 sats using the payee's callback URL
    Then the payer should receive a valid Lightning invoice
    And the invoice amount should be 100000 millisatoshis
    And the invoice description hash should match the metadata hash
    
    # Shutdown first instance
    When I send a SIGTERM signal to the server process
    Then the server should exit with code 0
    
    # Second server instance: Verify data persistence
    When I start the LNURL server with the configuration
    Then the server should start successfully
    And all services should be listening on their configured ports
    And the system waits for backend readiness
    
    # Test that persisted offer and backend still work
    When the payer requests the LNURL offer from the payee
    Then the offer should contain valid sendable amounts
    And the offer should contain valid metadata
    And the offer should provide a callback URL
    
    When the payer requests an invoice for 100 sats using the payee's callback URL
    Then the payer should receive a valid Lightning invoice
    And the invoice amount should be 100000 millisatoshis
    And the invoice description hash should match the metadata hash
    
    # Shutdown second instance
    When I send a SIGTERM signal to the server process
    Then the server should exit with code 0
    
    # Final cleanup
    When I send a SIGTERM signal to the server process
    Then the server should exit with code 0

    Examples: Current storage combinations
      | backend_store | offer_store |
      | sqlite        | sqlite      |
      


  @persistence @selective-cleanup @backend-only
  Scenario Outline: Backend data loss with offer persistence using <backend_store>/<offer_store> storage
    Given a valid configuration file exists with <backend_store> backend storage and <offer_store> offer storage
    
    # Create and persist data
    When I start the LNURL server with the configuration
    Then the server should start successfully
    When the payee creates an offer for their lightning node
    And the payee registers their lightning node as a backend
    And the system waits for backend readiness
    When the payer requests an invoice for 100 sats using the payee's callback URL
    Then the payer should receive a valid Lightning invoice
    When I send a SIGTERM signal to the server process
    
    # Delete only backend storage, keep offer storage
    When I delete the persistent <backend_store> backend storage files
    And I start the LNURL server with the configuration
    Then the server should start successfully
    And the system waits for backend readiness
    
    # Offer should exist but backend should be missing, causing invoice failure
    When the payer requests the LNURL offer from the payee
    Then the offer should contain valid sendable amounts
    But when the payer requests an invoice for 100 sats using the payee's callback URL
    Then the invoice request should fail with an "internal server error" status
    
    When I send a SIGTERM signal to the server process
    Then the server should exit with code 0

    Examples: Current storage combinations
      | backend_store | offer_store |
      | sqlite        | sqlite      |
      
