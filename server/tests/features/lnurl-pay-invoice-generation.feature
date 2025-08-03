@integration @lnurl-pay
Feature: LNURL Pay invoice generation
  As a payer
  I want to request an invoice from a payee's LNURL server
  So that I can open it with my lightning wallet and send payment

  # This feature tests the LNURL Pay protocol flow where a payee sets up 
  # offers and lightning nodes, and payers request invoices for payment
  # Note: This feature assumes the server is already running with backends available

  Background:
    Given the payee has lightning node backends available
    And all services should be listening on their configured ports

  @lightning-backend @happy-path
  Scenario Outline: Payer requests invoice from single payee's <backend_type> lightning offer using <protocol>
    # Test LNURL Pay flow where payee creates offer and payer requests invoice
    Given a valid configuration file exists for <protocol>
    And the single payee has a <backend_type> lightning node available
    When the single payee creates an offer for their lightning node
    And the single payee registers their lightning node as a backend
    And the system waits for backend readiness
    When the payer requests the LNURL offer from the payee using <protocol>
    Then the offer should contain valid sendable amounts
    And the offer should contain valid metadata
    And the offer should provide a callback URL
    When the payer requests an invoice for 100 sats using the payee's callback URL with <protocol>
    Then the payer should receive a valid Lightning invoice
    And the invoice amount should be 100000 millisatoshis
    And the invoice description hash should match the metadata hash

    Examples:
      | backend_type | protocol |
      | CLN          | http     |
      | CLN          | https    |
      | LND          | http     |
      | LND          | https    |