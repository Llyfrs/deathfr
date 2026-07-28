---
name: torn-api
description: Use when working with the torn-api Rust crate (Torn.com v2 API bindings), making Torn API requests, handling Torn API errors, implementing custom executors or requests, or migrating the project's custom TornAPI wrapper to the torn-api crate. Covers ReqwestClient, Executor, BulkExecutor, scopes, models, IntoRequest, and all auto-generated types.
---

# torn-api Crate Reference

Auto-generated, async, typesafe Rust bindings for the [Torn v2 API](https://www.torn.com/swagger.php).
Crate: `torn-api` (latest 5.1.0). Docs: https://docs.rs/torn-api/latest/torn_api/

## Installation

```toml
[dependencies]
torn-api = "5.1"
```

Requires an async runtime (tokio or smol). Runtime-agnostic when `reqwest` feature is disabled.

## Feature Flags

| Feature    | Description |
|------------|-------------|
| `reqwest`  | Includes `ReqwestClient` executor (requires tokio) |
| `models`   | Generate response and parameter model definitions |
| `requests` | Generate request model definitions |
| `scopes`   | Generate scope objects grouping endpoints by category |
| `builder`  | Generate builders via `bon` for all request structs |
| `strum`    | Derive `EnumIs` and `EnumTryAs` for auto-generated enums |

## Crate Architecture

```
torn_api::
  executor::   - Executor, BulkExecutor traits + ReqwestClient
  models::     - Auto-generated response/request model structs + enums
  parameters:: - Auto-generated parameter types (ApiFrom, ApiLimit, ApiSort, etc.)
  request::    - IntoRequest trait, ApiRequest, ApiResponse structs
  scopes::     - Scope structs (UserScope, FactionScope, TornScope, etc.)
  ApiError     - Enum of all API error variants
  Error        - Top-level error enum (Parameter, Network, Parsing, Api)
  ParameterError - Error for invalid parameter values
```

## Core Traits

### Executor

Central trait for executing single API requests.

```rust
pub trait Executor: Sized {
    type Error: From<Error> + From<ApiError> + Send;

    fn execute<R: IntoRequest>(self, request: R)
        -> impl Future<Output = (R::Discriminant, Result<ApiResponse, Self::Error>)> + Send;

    fn fetch<R: IntoRequest>(self, request: R)
        -> impl Future<Output = Result<R::Response, Self::Error>> + Send;
}
```

Implemented for `&ReqwestClient` (with `reqwest` feature).

### BulkExecutor

Executes multiple requests concurrently, returning a `Stream`.

```rust
pub trait BulkExecutor: Sized {
    type Error: From<Error> + From<ApiError> + Send;

    fn execute<R: IntoRequest>(self, requests: impl IntoIterator<Item = R>)
        -> impl Stream<Item = (R::Discriminant, Result<ApiResponse, Self::Error>)> + Unpin;

    fn fetch_many<R: IntoRequest>(self, requests: impl IntoIterator<Item = R>)
        -> impl Stream<Item = (R::Discriminant, Result<R::Response, Self::Error>)> + Unpin;
}
```

### ExecutorExt (requires `scopes` feature)

Provides scope-based access on any `Executor`:

```rust
pub trait ExecutorExt: Executor + Sized {
    fn user(self) -> UserScope<Self>;
    fn faction(self) -> FactionScope<Self>;
    fn torn(self) -> TornScope<Self>;
    fn market(self) -> MarketScope<Self>;
    fn racing(self) -> RacingScope<Self>;
    fn forum(self) -> ForumScope<Self>;
    fn key(self) -> KeyScope<Self>;
}
```

### BulkExecutorExt (requires `scopes` feature)

Provides bulk scope access on any `BulkExecutor`:

```rust
pub trait BulkExecutorExt: BulkExecutor + Sized {
    fn user_bulk(self) -> BulkUserScope<Self>;
    fn faction_bulk(self) -> BulkFactionScope<Self>;
    fn torn_bulk(self) -> BulkTornScope<Self>;
    fn market_bulk(self) -> BulkMarketScope<Self>;
    fn racing_bulk(self) -> BulkRacingScope<Self>;
    fn forum_bulk(self) -> BulkForumScope<Self>;
    fn key_bulk(self) -> BulkKeyScope<Self>;
}
```

### IntoRequest

Trait for typed API requests. Implement this for custom/undocumented endpoints.

```rust
pub trait IntoRequest: Send {
    type Discriminant: Send + 'static;
    type Response: for<'de> Deserialize<'de> + Send;

    fn into_request(self) -> (Self::Discriminant, ApiRequest);
}
```

`ApiRequest` contains `path: String` and `parameters: Vec<...>`.

## Quickstart

```rust
use torn_api::executor::{ReqwestClient, ExecutorExt};

let client = ReqwestClient::new("YOUR_API_KEY");

// Scope-based access
let response = client.user().profile().await.unwrap();
let faction = client.faction().basic().await.unwrap();

// With parameters
use torn_api::models::RacingRaceTypeEnum;
let races = client.user().races(|r| r.cat(RacingRaceTypeEnum::Official)).await.unwrap();
```

## Custom Executor

Implement `Executor` for custom logic (e.g., multi-key rotation, custom HTTP client):

```rust
use torn_api::executor::Executor;
use torn_api::request::{IntoRequest, ApiResponse};
use torn_api::{Error, ApiError};

struct MyExecutor { /* ... */ }

impl Executor for MyExecutor {
    type Error = Error;

    async fn execute<R: IntoRequest>(self, request: R)
        -> (R::Discriminant, Result<ApiResponse, Self::Error>)
    {
        let (discriminant, api_request) = request.into_request();
        // Build HTTP request from api_request.path and api_request.parameters
        // Return (discriminant, result)
        todo!()
    }
}
```

## Custom Requests (Undocumented Endpoints)

For v1 endpoints not yet ported to v2:

```rust
use torn_api::request::{IntoRequest, ApiRequest};
use torn_api::models::UserId;
use serde::Deserialize;

#[derive(Deserialize)]
struct UserBasic {
    id: UserId,
    name: String,
    level: i32,
}

struct UserBasicRequest(UserId);

impl IntoRequest for UserBasicRequest {
    type Discriminant = UserId;
    type Response = UserBasic;

    fn into_request(self) -> (Self::Discriminant, ApiRequest) {
        let request = ApiRequest {
            path: format!("/user/{}/basic", self.0),
            parameters: Vec::default(),
        };
        (self.0, request)
    }
}

let client = ReqwestClient::new("YOUR_API_KEY");
let basic = client.fetch(UserBasicRequest(UserId(1))).await.unwrap();
```

## Error Handling

### ApiError Variants

| Variant | Meaning |
|---------|---------|
| `Unknown` | Unrecognized error |
| `KeyIsEmpty` | No API key provided |
| `IncorrectKey` | Invalid API key |
| `WrongType` | Wrong request type |
| `WrongFields` | Invalid fields |
| `TooManyRequest` | Rate limited |
| `IncorrectId` | Invalid entity ID |
| `IncorrectIdEntityRelation` | ID doesn't match entity type |
| `IpBlock` | IP blocked |
| `ApiDisabled` | API disabled |
| `KeyOwnerInFederalJail` | Key owner in federal jail |
| `KeyChange` | Key recently changed |
| `KeyRead` | Key read error |
| `TemporaryInactivity` | Account temporarily inactive |
| `DailyReadLimit` | Daily read limit reached |
| `TemporaryError` | Temporary server error |
| `InsufficientAccessLevel` | Insufficient key access level |
| `Backend` | Backend error |
| `Paused` | API paused |
| `NotMigratedCrimes` | Crimes not migrated |
| `RaceNotFinished` | Race still in progress |
| `IncorrectCategory` | Invalid category |
| `OnlyInV1` | Endpoint only in v1 |
| `OnlyInV2` | Endpoint only in v2 |
| `ClosedTemporarily` | Temporarily closed |
| `Other { code, message }` | Catch-all for new errors |

### Error Enum (top-level)

```rust
pub enum Error {
    Parameter(ParameterError),
    Network(reqwest::Error),
    Parsing(serde_json::Error),
    Api(ApiError),
}
```

## Key Model Types

- `UserId`, `FactionId`, `ItemId`, `PropertyId`, `RaceId`, `ForumId`, etc. - newtype wrappers for IDs
- `UserBasic`, `UserProfileResponse`, `UserBattleStatsResponse` - user data
- `FactionBasic`, `FactionMembersResponse`, `FactionChain` - faction data
- `Revive`, `ReviveSimplified`, `RevivesResponse`, `RevivesFullResponse` - revive data
- `Attack`, `AttackSimplified`, `AttackLog` - attack data
- `TornItem`, `TornItemDetails` - item definitions
- `RacingRaceDetails`, `Race` - racing data

## Scopes (Endpoints by Category)

- **UserScope**: profile, basic, battlestats, bars, revives, attacks, personalstats, etc.
- **FactionScope**: basic, members, attacks, revives, chains, crimes, territory, etc.
- **TornScope**: items, honors, medals, merits, education, crimes, properties, etc.
- **MarketScope**: itemmarket, properties, rentals, etc.
- **RacingScope**: races, tracks, records, cars, upgrades, etc.
- **ForumScope**: categories, threads, posts, etc.
- **KeyScope**: info, log, etc.

## Pagination Parameters

- `ApiFrom(timestamp)` - lower time bound
- `ApiTo(timestamp)` - upper time bound
- `ApiLimit(n)` / `ApiLimit100`, `ApiLimit1000`, etc. - result count limits
- `ApiOffset(n)` - pagination offset
- `ApiSort` / `ApiSortAsc` / `ApiSortDesc` - sort ordering
- `ApiFiltersUser` - incoming/outgoing filter
- `ApiTarget` - target filter


