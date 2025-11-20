@cli @token
Feature: CLI token functionality
  As an admin
  I want to manage tokens via CLI
  So that I can secure service operations

  # This feature tests the CLI commands for managing tokens
  # for both discovery and offer services, including key generation,
  # token minting, and verification

  Background:
    Given the swgr CLI is available

  @token-key-generation
  Scenario Outline: Generate ECDSA key pair for <service> token
    When I run "swgr <service> token key" with public and private key output paths
    Then the command should succeed
    And the public key file should exist
    And the private key file should exist

    Examples:
      | service   |
      | discovery |
      | offer     |

  @token-mint
  Scenario Outline: Mint <service> token with existing key
    Given a valid ECDSA private key exists
    When I run "swgr <service> token mint" with key path and expiration
    Then the command should succeed
    And a valid token should be output to stdout

    Examples:
      | service   |
      | discovery |
      | offer     |

  @token-mint-with-output
  Scenario Outline: Mint <service> token with output file
    Given a valid ECDSA private key exists
    When I run "swgr <service> token mint" with key path, expiration, and output path
    Then the command should succeed
    And the token file should exist

    Examples:
      | service   |
      | discovery |
      | offer     |

  @token-mint-all
  Scenario Outline: Mint <service> token with new key
    When I run "swgr <service> token mint-all" with public path, private path, and expiration
    Then the command should succeed
    And the public key file should exist
    And the private key file should exist
    And a valid token should be output to stdout

    Examples:
      | service   |
      | discovery |
      | offer     |

  @token-mint-all-with-output
  Scenario Outline: Mint <service> token with new key and output file
    When I run "swgr <service> token mint-all" with public path, private path, expiration, and output path
    Then the command should succeed
    And the public key file should exist
    And the private key file should exist
    And the token file should exist

    Examples:
      | service   |
      | discovery |
      | offer     |

  @token-verify-from-stdin
  Scenario Outline: Verify <service> token from stdin
    Given a valid ECDSA public key exists
    And a valid <service> token exists
    When I run "swgr <service> token verify" with public key path and token via stdin
    Then the command should succeed
    And the verification output should be valid

    Examples:
      | service   |
      | discovery |
      | offer     |

  @token-verify-from-file
  Scenario Outline: Verify <service> token from file
    Given a valid ECDSA public key exists
    And a valid <service> token file exists
    When I run "swgr <service> token verify" with public key path and token file path
    Then the command should succeed
    And the verification output should be valid

    Examples:
      | service   |
      | discovery |
      | offer     |

  @token-verify-invalid
  Scenario Outline: Verify invalid <service> token
    Given a valid ECDSA public key exists
    And an invalid <service> token exists
    When I run "swgr <service> token verify" with public key path and invalid token
    Then the command should fail
    And an error message should be shown

    Examples:
      | service   |
      | discovery |
      | offer     |
