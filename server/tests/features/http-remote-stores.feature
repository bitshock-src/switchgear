@integration @http-remote-stores
Feature: Http Remote Stores
  As an admin
  I want to use remote stores for Offers and Discovery Backends
  So that I can better manage data

  # This feature tests the ability to use HTTP remote stores for both offers and discovery backends.
  # It requires two LNURL servers:
  # 1. First server with only offers and discovery services using memory stores
  # 2. Second server with only lnurl service using HTTP stores configured to access the first server
  
  Background:
    Given the payee has a CLN lightning node available
    And the server is not already running

  @http-stores @remote-data-access @multi-server
  Scenario: Complete HTTP remote stores workflow with distributed services
    # Setup first server with offers and discovery services using memory stores
    Given a server 1 configuration with memory stores exists
    When I start server 1 with offers and discovery services
    Then server 1 should have offers and discovery services listening
    
    # Setup second server with only lnurl service using HTTP stores
    Given a server 2 configuration with HTTP stores pointing to server 1 exists
    When I start server 2 with only lnurl service
    Then server 2 should have only lnurl service listening
    
    # Create offer and backend on server 1 (data storage server)
    When the single payee creates an offer for their lightning node
    And the single payee registers their lightning node as a backend
    And the system waits for backend readiness
    
    # Test LNURL Pay flow through server 2 (using HTTP remote stores)
    When the payer requests the LNURL offer from the payee
    Then the payee offer should contain valid sendable amounts
    And the payee offer should contain valid metadata
    And the payee offer should provide a callback URL
    
    When the payer requests an invoice for 100 sats using the payee's callback URL
    Then the payer should receive a valid Lightning invoice
    And the invoice amount should be 100000 millisatoshis
    And the invoice description hash should match the metadata hash
    
    # Stop servers and validate logs
    When I stop all servers
    Then server 1 logs should contain offer creation requests
    And server 1 logs should contain backend registration requests
    And server 1 logs should contain health check requests for offers and discovery services
    And server 1 logs should contain HTTP requests from server 2 for offers and discovery
    And server 2 logs should contain offer retrieval requests via HTTP stores
    And server 2 logs should contain invoice generation requests
    And server 2 logs should contain health check requests for lnurl service