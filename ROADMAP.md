# Switchgear Roadmap

Roadmap entries are chronological. Chronology is determined by priority and engineering precursors.

## MILESTONE 1 : Production Readiness

Goal: move Switchgear from ALPHA status to RELEASE status.

#### ~~Backend Names~~ DONE

~~Lightning Node backends need a friendly name so admins can easily identify them.~~

#### Rate Limiting for LnUrl Endpoints

Mitigate overload and certain DOS attacks with rate limiting:

* max rate limit for each endpoint
* per-session (ip-based) rate limit per endpoint
* configurable individual max and session limits for each endpoint

#### ~~Paging for Offer GET Endpoint~~ DONE

~~GET endpoints for Discovery and Offer Services that return a list need paging. For REST: `?page={page}` query parameter. The CLI will need a --page parameter.~~

#### ~~PATCH Method for Discovery~~ DONE

~~Add a PATCH method to Discovery, so Lightning Node backends can be rapidly enabled or weight-adjusted without pushing an entire copy of the definition back to the service. Add an --enable and --weight to the CLI.~~

#### Separate Files for Server Logs

Each service log needs its own log file, rather combining into the main log. This makes administration simpler, and securely segregates user behavior reporting from system status.

#### Lightning Node Backend Status Endpoint

The health and enablement status of all attached Lightning Nodes must be made available to admins. The node status endpoint will drive a CLI status command as well.

#### ~~GitHub CI Pipeline~~ DONE

~~Test automation must be fully integrated with GitHub before releases can be cut with confidence.~~

#### OpenTelemetry Runtime Metrics

Switchgear must instrumented with runtime metrics so performance bottlenecks can be mitigated and resource constraints can be addressed by admins.

## MILESTONE 2 : Paying to static internet identifiers: LUD-16

Switchgear will support LNURL [static identifiers](https://github.com/lnurl/luds/blob/luds/16.md) (pay to me@domain.com), which are popular and convenient. The identifier database will be populated by local database, as well as integrated with [LDAP](https://en.wikipedia.org/wiki/Lightweight_Directory_Access_Protocol) for dynamic updates.


## MILESTONE 3 : Integrations : BTCPay Plugin

Integrate Switchgear with [BTCPay Plugin](https://docs.btcpayserver.org/Plugins/). Admins will be able to use Switchgear as an LNURL server with their existing system.

## MILESTONE 4 : Integrations : LNBits Extension

Integrate Switchgear with LNBits via API Bridge [extension](https://docs.lnbits.org/devs/extensions.html). Admins will be able to use Switchgear as an LNURL server with their existing system.

## MILESTONE 5 : Integrations : Start9 Service Package

Integrate Switchgear with [Start9 Service Package](https://docs.start9.com/0.3.5.x/developer-docs/packaging.html.) . Admins will be able to use Switchgear as an LNURL server with their existing system.

## MILESTONE 6 : Bolt12 Capabilities

Provide full Switchgear load balance feature set for Bolt12:

* generate Bolt12 Offers from existing Offer database or integrations
* answer Bolt12 Invoice Requests and Responses over Lightning P2P network
* balance and proxy invoice requests to Lighting Node backends over grpc client

See [LNDK](https://github.com/lndk-org/lndk) on how this works.

Switchgear Bolt12 Capabilities will also have the benefit of providing Bolt12 to any Lightning Node implementation, pushing the standard forward.
