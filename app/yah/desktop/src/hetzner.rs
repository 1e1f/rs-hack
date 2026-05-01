//! @arch:layer(kg_store)
//! @arch:role(bridge)
//!
//! Hetzner Cloud client. Originally R027-F6's read-only smoke test
//! for the API-keys credential path; R029 extends it with the
//! provision surface (SSH-key list/upload here, server-create coming
//! in T2). The token stays Rust-side — see
//! `.yah/arch/authored/settings-api-keys.md` for the threat-model rationale
//! (renderer only ever sees `has(provider)` plus parsed results).
//!
//! Wire DTOs mirror the subset of upstream Hetzner objects the UI
//! actually renders; extra fields are dropped at parse time so the JS
//! bundle isn't flooded with what it doesn't display.
//!
//! Endpoints in use:
//! * `GET /v1/servers` — <https://docs.hetzner.cloud/#servers-get-all-servers>
//! * `POST /v1/servers` — <https://docs.hetzner.cloud/#servers-create-a-server>
//! * `GET /v1/ssh_keys` / `POST /v1/ssh_keys` — <https://docs.hetzner.cloud/#ssh-keys>
//! * `GET /v1/server_types` — <https://docs.hetzner.cloud/#server-types>
//! * `GET /v1/locations` — <https://docs.hetzner.cloud/#locations>
//! * `GET /v1/images?type=system` — <https://docs.hetzner.cloud/#images>
//!
//! @yah:relay(R029, "Hetzner provision: SSH-key management + create-server flow on Infra tab")
//! @yah:status(review)
//! @yah:assignee(agent:claude)
//! @yah:parent(R013)
//! @yah:next("T1 SSH-key plumbing: ssh_key_list_local + ssh_key_generate (ed25519) Tauri commands, Hetzner upload/list (POST/GET /v1/ssh_keys), HetznerSshKey DTO")
//! @yah:next("T2 Hetzner create_server: POST /v1/servers Tauri command, HetznerCreateServerSpec wire type (name, server_type, location, image, ssh_key_ids[]), parse + return new HetznerServer to renderer")
//! @yah:next("T3 Infra sub-tabs: split current InfraTab into Servers and Provision sub-tabs; default to Provision when servers list is empty; ProvisionForm with type/location/image/ssh-key dropdowns and price hints")
//!
//! @yah:ticket(R029-T1, "SSH-key plumbing: local discover/generate (ed25519) + Hetzner upload/list")
//! @yah:status(review)
//! @yah:assignee(agent:claude)
//! @yah:parent(R029)
//! @yah:handoff("SSH-key plumbing landed end-to-end. New crate dep: ssh-key 0.6.7 with features=ed25519+getrandom (pure-rust, no openssl). New module app/tauri/src/ssh_keys.rs: list_local() scans ~/.ssh/*.pub via ssh_key::PublicKey::from_openssh (drops anything unparseable), generate(name) writes Algorithm::Ed25519 keypair under ~/.ssh/<name>{,.pub} with mode 0600 on Unix and rejects clobber via AlreadyExists. LocalSshKey DTO carries name/public_key_path/public_key/fingerprint(SHA256:)/algorithm/has_private; private bytes never leave the host. Hetzner module gained list_ssh_keys() + upload_ssh_key(name, public_key) (HetznerSshKey DTO mirrors upstream subset id/name/fingerprint/public_key/created); shared auth_client + check_status helpers replace the inline 401/non-2xx handling on list_servers. Four new #[tauri::command]s registered in lib.rs: ssh_key_list_local, ssh_key_generate, hetzner_list_ssh_keys, hetzner_upload_ssh_key. env adapter: types.ts gains HetznerSshKey + LocalSshKey; index.ts adds HetznerRpc.{listSshKeys, uploadSshKey} and a new SshRpc { listLocal, generate } on the Rpc trait; tauri.ts wires invokes; browser.ts returns [] for list calls and rejects loudly for generate/upload. cargo test -p yah-tauri --lib 15/15 green (incl. new ssh_keys::tests::generate_round_trips_through_list — uses a per-test temp HOME); bun run typecheck + bun run build green.")
//! @yah:verify("cargo build -p yah-tauri")
//! @yah:verify("cargo test -p yah-tauri --lib")
//! @yah:verify("cd yah-ui && bun run typecheck && bun run build")
//! @yah:gotcha("ssh-key 0.6 (not 0.7-rc): rust-version 1.65 stable, OsRng comes from ssh_key::rand_core. PrivateKey::random takes &mut OsRng. set_comment() must be called before to_openssh so the private file's comment line carries '<name>@yah'. LineEnding::LF — Windows operators get LF in their .ssh files, which OpenSSH on Windows accepts.")
//!
//! @yah:ticket(R029-T2, "Hetzner create_server: POST /v1/servers Tauri command + HetznerCreateServerSpec wire type")
//! @yah:status(review)
//! @yah:assignee(agent:claude)
//! @yah:parent(R029)
//! @yah:handoff("create_server end-to-end. New HetznerCreateServerSpec { name, server_type, location, image, ssh_keys: Vec<u64> } (Deserialize+Serialize, field names match upstream JSON one-to-one). create_server() POSTs /v1/servers, reuses auth_client + check_status — 422 bodies surface via HetznerError::Upstream verbatim so the form can render the upstream reason string. Parsed CreateServerResponse.server reuses RawServer → HetznerServer (existing DTO already covered the shape). New #[tauri::command] hetzner_create_server registered in lib.rs. env adapter: types.ts gains HetznerCreateServerSpec; index.ts adds HetznerRpc.createServer; tauri.ts wires invoke('hetzner_create_server', { spec }); browser.ts rejects loudly. cargo build/test green (15/15 lib tests, no behaviour change to existing tests).")
//! @yah:verify("cargo build -p yah-tauri")
//! @yah:verify("cargo test -p yah-tauri --lib")
//! @yah:verify("cd yah-ui && bun run typecheck && bun run build")
//!
//! @yah:ticket(R029-T3, "Infra sub-tabs: Servers + Provision; ProvisionForm with live catalogue + ssh-key dropdowns")
//! @yah:status(review)
//! @yah:assignee(agent:claude)
//! @yah:parent(R029)
//! @yah:handoff("InfraTab now mounts new InfraView (yah-ui/src/components/infra/InfraView.tsx) — Servers/Provision sub-tab strip; default-tab decision at mount via one-shot listServers (empty project → Provision); successful createServer bumps a refreshKey + flips back to Servers, which remounts HetznerServerList for a fresh fetch. ProvisionForm fetches the live Hetzner catalogue on mount (listServerTypes / listLocations / listImages run in parallel) — earlier hardcoded matrix was wrong across architecture lines (cx/cax EU-only, cpx everywhere) and the user reported it surfaced retired SKUs that 422'd at submit. Now: Location is the first dropdown; Type is filtered by `prices[].location` (price-ascending) so only types Hetzner actually builds in the chosen DC appear, with per-location €/mo (net + gross-incl-VAT) shown inline; Image is filtered by selectedType.architecture and deduped by name (Hetzner publishes one record per (name, arch) and POST /v1/servers auto-picks the right variant when matching by name). Initial defaults pick fsn1 (or first available), the cheapest x86 type there, and the highest-versioned debian/ubuntu/fedora image for that architecture. Effects re-pin invalid type/image when their parent flips. SSH-key picker unchanged: optgrouped <select> over Hetzner project keys + local keys (~/.ssh/*.pub, deduped against Hetzner by fingerprint, auto-uploaded JIT on submit), 'No key' option, '+ Generate new yah key…' sentinel reveals inline name + Generate+upload (refreshKeys re-selects by fingerprint). New Rust DTOs: HetznerServerType { prices: [{location, price_monthly_net, price_monthly_gross}], architecture, cpu_type, deprecated, … }, HetznerLocation, HetznerImage; three new Tauri commands hetzner_list_{server_types,locations,images} registered in lib.rs. env adapter: types.ts + index.ts + tauri.ts + browser.ts (browser stubs return [], real reject on createServer). cargo build green, cargo test -p yah-tauri --lib 13/13 green (no test changes), bun typecheck + bun build green.")
//! @yah:verify("cargo build -p yah-tauri")
//! @yah:verify("cargo test -p yah-tauri --lib")
//! @yah:verify("cd yah-ui && bun run typecheck && bun run build")
//! @yah:gotcha("Catalogue endpoints use per_page=50; current Hetzner catalogue (~30 server_types, ~6 locations, ~25 system images after filter) fits comfortably. If they double the SKU count we'll need a paged loop — `meta.pagination` carries the cursor.")
//!
//! @yah:ticket(R034-T3, "Identity authorize/deauthorize writes for Hetzner + GitHub")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P3)
//! @yah:parent(R034)
//! @arch:see(.yah/arch/authored/yah-identities.md)
//! @yah:handoff("Identity authorize/deauthorize writes (P3) landed. New surfaces in app/tauri/src/identities.rs: authorize_hetzner(id, name) looks up the identity, calls hetzner::upload_ssh_key (existing R029-T1 plumbing), builds Authorization::Hetzner with returned key_id_in_hetzner, persists via write_authorization (replace-same-provider + last_used_at bump). authorize_github(id, title) posts {title, key} to https://api.github.com/user/keys with the github PAT from api_keys::get, resolves account login via GET /user, persists Authorization::Github with returned key id. deauthorize_hetzner(id) finds the cached Authorization::Hetzner, calls hetzner::delete_ssh_key (new helper, DELETE /v1/ssh_keys/<id>, 404 maps to Ok(false) for idempotency), clears entry from authorized_at. deauthorize_github(id) DELETE /user/keys/<id>, 404 same idempotent treatment. Both deauthorize_* return Ok(false) when no Authorization for that provider exists (no-op success). New write_authorization helper centralises the one-Authorization-per-provider-per-identity invariant. Four new Tauri commands wired in lib.rs. Tests: 12/12 (3 new no-network tests covering replace-same-provider, deauth-when-missing, lookup error). cargo build green; cargo test 71/71 green; bun typecheck + build green.")
//! @yah:next("F4/F5 unblocked: env adapter rpc.identity { list, create, import, remove, probeAll, probeHetzner, probeGithub, authorizeHetzner, deauthorizeHetzner, authorizeGithub, deauthorizeGithub }; types.ts mirrors of Identity, Authorization, ProbeReport, etc.; tauri.ts invokes all 11; browser.ts mock data for component inspection.")
//! @yah:next("Rig-card 'fix' button (yah-identities.md UX section) is one rpc.identity.authorizeHetzner/authorizeGithub call against the top-ranked identity; UI refetches identityList after to pick up the new Authorization.")
//! @yah:next("Cleanup: probe writes have their own retain+push for same-variant replacement — fold onto write_authorization helper once F4/F5 settle. Behaviour identical; collapses the invariant to one site.")
//! @yah:verify("cargo build -p yah-tauri")
//! @yah:verify("cargo test -p yah-tauri --lib identities")
//! @yah:verify("cd yah-ui && bun run typecheck && bun run build")
//! @yah:gotcha("GitHub PAT scope: authorize_github / deauthorize_github need admin:public_key. probe_github only needs read:public_key. 403 body names the missing scope verbatim — surface in renderer.")
//! @yah:gotcha("Hetzner upload_ssh_key returns 422 on duplicate fingerprint/name. Surfaces as HetznerError::Upstream{422, body}; renderer can pattern-match to suggest 'already authorized' + probe-back. Decision tree belongs in F4/F5.")
//! @yah:assumes("GitHub DELETE /user/keys/<id> 204 on success, 404 when gone — confirmed in docs but not tested live. 404→Ok(true) mapping keeps registry clean across out-of-band GitHub mutations.")

use reqwest::header::AUTHORIZATION;
use serde::{Deserialize, Serialize};

use crate::api_keys;

const HETZNER_API_BASE: &str = "https://api.hetzner.cloud";
const HETZNER_PROVIDER: &str = "hetzner";

#[derive(Debug, thiserror::Error)]
pub enum HetznerError {
    #[error("no Hetzner token stored — set one in Settings → API Keys")]
    NoToken,
    #[error("Hetzner token rejected: 401 unauthorized")]
    Unauthorized,
    #[error("Hetzner API returned {status}: {body}")]
    Upstream { status: u16, body: String },
    #[error("keychain read failed: {0}")]
    Keychain(#[from] api_keys::ApiKeyError),
    #[error("HTTP transport error: {0}")]
    Http(#[from] reqwest::Error),
}

/// Renderer-facing server summary. Mirrors the subset of Hetzner's
/// upstream `Server` object the Infra tab cares about; new columns
/// land here as the UI grows.
#[derive(Debug, Clone, Serialize)]
pub struct HetznerServer {
    pub id: u64,
    pub name: String,
    pub status: String,
    pub server_type: String,
    pub location: String,
    pub ipv4: Option<String>,
    pub created: String,
}

#[derive(Debug, Deserialize)]
struct ServersResponse {
    servers: Vec<RawServer>,
}

#[derive(Debug, Deserialize)]
struct RawServer {
    id: u64,
    name: String,
    status: String,
    created: String,
    server_type: RawServerType,
    datacenter: RawDatacenter,
    public_net: RawPublicNet,
}

#[derive(Debug, Deserialize)]
struct RawServerType {
    name: String,
}

#[derive(Debug, Deserialize)]
struct RawDatacenter {
    location: RawLocation,
}

#[derive(Debug, Deserialize)]
struct RawLocation {
    name: String,
}

#[derive(Debug, Deserialize)]
struct RawPublicNet {
    ipv4: Option<RawIpv4>,
}

#[derive(Debug, Deserialize)]
struct RawIpv4 {
    ip: Option<String>,
}

impl From<RawServer> for HetznerServer {
    fn from(r: RawServer) -> Self {
        Self {
            id: r.id,
            name: r.name,
            status: r.status,
            server_type: r.server_type.name,
            location: r.datacenter.location.name,
            ipv4: r.public_net.ipv4.and_then(|v| v.ip),
            created: r.created,
        }
    }
}

/// Fetch every server in the operator's Hetzner project. The single
/// page returned by `/v1/servers` is fine for v1 — pagination kicks in
/// past 25 servers and we'll add it when an operator hits the limit.
pub async fn list_servers() -> Result<Vec<HetznerServer>, HetznerError> {
    let (client, token) = auth_client().await?;
    let resp = client
        .get(format!("{HETZNER_API_BASE}/v1/servers"))
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .await?;
    let resp = check_status(resp).await?;
    let parsed: ServersResponse = resp.json().await?;
    Ok(parsed
        .servers
        .into_iter()
        .map(HetznerServer::from)
        .collect())
}

// ---------- SSH keys ----------
//
// Hetzner stores SSH keys per-project; the `ssh_keys` array on the
// `POST /v1/servers` body wires the listed keys into the new server's
// `~/.ssh/authorized_keys` at first boot. R029-T1 surfaces the list +
// upload flow so the provision form can let the operator pick from
// existing keys or push a yah-managed one before creation.

/// Renderer-facing SSH-key entry. Mirrors Hetzner's upstream `SSHKey`
/// object. `public_key` is the OpenSSH single-line form
/// (`ssh-ed25519 AAAA…`).
#[derive(Debug, Clone, Serialize)]
pub struct HetznerSshKey {
    pub id: u64,
    pub name: String,
    pub fingerprint: String,
    pub public_key: String,
    pub created: String,
}

#[derive(Debug, Deserialize)]
struct SshKeysResponse {
    ssh_keys: Vec<RawSshKey>,
}

#[derive(Debug, Deserialize)]
struct SshKeyResponse {
    ssh_key: RawSshKey,
}

#[derive(Debug, Deserialize)]
struct RawSshKey {
    id: u64,
    name: String,
    fingerprint: String,
    public_key: String,
    created: String,
}

impl From<RawSshKey> for HetznerSshKey {
    fn from(r: RawSshKey) -> Self {
        Self {
            id: r.id,
            name: r.name,
            fingerprint: r.fingerprint,
            public_key: r.public_key,
            created: r.created,
        }
    }
}

#[derive(Debug, Serialize)]
struct UploadSshKeyBody<'a> {
    name: &'a str,
    public_key: &'a str,
}

async fn auth_client() -> Result<(reqwest::Client, String), HetznerError> {
    let token = api_keys::get(HETZNER_PROVIDER)?.ok_or(HetznerError::NoToken)?;
    Ok((reqwest::Client::new(), token))
}

/// Translate HTTP status into the right `HetznerError` variant before
/// trying to parse the body. Centralised so every endpoint handles
/// 401 / non-2xx the same way.
async fn check_status(resp: reqwest::Response) -> Result<reqwest::Response, HetznerError> {
    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(HetznerError::Unauthorized);
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(HetznerError::Upstream {
            status: status.as_u16(),
            body,
        });
    }
    Ok(resp)
}

pub async fn list_ssh_keys() -> Result<Vec<HetznerSshKey>, HetznerError> {
    let (client, token) = auth_client().await?;
    let resp = client
        .get(format!("{HETZNER_API_BASE}/v1/ssh_keys"))
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .await?;
    let resp = check_status(resp).await?;
    let parsed: SshKeysResponse = resp.json().await?;
    Ok(parsed
        .ssh_keys
        .into_iter()
        .map(HetznerSshKey::from)
        .collect())
}

/// Delete a key from the operator's Hetzner project. 404 resolves to
/// `Ok(false)` so the deauthorize flow stays idempotent — a re-run
/// after the key is already gone should resolve cleanly, not surface a
/// confusing 404 to the renderer.
pub async fn delete_ssh_key(key_id: u64) -> Result<bool, HetznerError> {
    let (client, token) = auth_client().await?;
    let resp = client
        .delete(format!("{HETZNER_API_BASE}/v1/ssh_keys/{key_id}"))
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .await?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(false);
    }
    let _ = check_status(resp).await?;
    Ok(true)
}

pub async fn upload_ssh_key(name: &str, public_key: &str) -> Result<HetznerSshKey, HetznerError> {
    let (client, token) = auth_client().await?;
    let body = UploadSshKeyBody { name, public_key };
    let resp = client
        .post(format!("{HETZNER_API_BASE}/v1/ssh_keys"))
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .json(&body)
        .send()
        .await?;
    let resp = check_status(resp).await?;
    let parsed: SshKeyResponse = resp.json().await?;
    Ok(parsed.ssh_key.into())
}

// ---------- Catalogue (server types, locations, images) ----------
//
// The provision form on the renderer fetches these on mount so its
// dropdowns mirror what the Hetzner project can actually build today.
// Hardcoded options would silently 422 when prices/SKUs churn (the
// `cx` line is EU-only, `cax` is ARM, `cpx` is everywhere — and this
// matrix has changed twice in 2025 alone). Pulling live keeps the
// form authoritative without per-region special-casing here.

/// Per-location price for a server type. Gross is incl-VAT; net is the
/// figure shown on the public hetzner.com pricing page. We surface both
/// so the renderer can pick the convention that matches local
/// expectations.
#[derive(Debug, Clone, Serialize)]
pub struct HetznerServerTypePrice {
    pub location: String,
    pub price_monthly_net: String,
    pub price_monthly_gross: String,
}

/// Renderer-facing server-type catalogue entry. `prices` is the source
/// of truth for "is this type buildable in location X?" — empty array
/// for a location means Hetzner won't accept it there.
#[derive(Debug, Clone, Serialize)]
pub struct HetznerServerType {
    pub id: u64,
    pub name: String,
    pub description: String,
    pub cores: u32,
    pub memory: f64,
    pub disk: u32,
    pub architecture: String,
    pub cpu_type: String,
    pub deprecated: bool,
    pub prices: Vec<HetznerServerTypePrice>,
}

#[derive(Debug, Deserialize)]
struct ServerTypesResponse {
    server_types: Vec<RawServerType2>,
}

#[derive(Debug, Deserialize)]
struct RawServerType2 {
    id: u64,
    name: String,
    description: String,
    cores: u32,
    memory: f64,
    disk: u32,
    architecture: String,
    cpu_type: String,
    #[serde(default)]
    deprecated: bool,
    #[serde(default)]
    prices: Vec<RawPrice>,
}

#[derive(Debug, Deserialize)]
struct RawPrice {
    location: String,
    price_monthly: RawPriceFigure,
}

#[derive(Debug, Deserialize)]
struct RawPriceFigure {
    net: String,
    gross: String,
}

impl From<RawServerType2> for HetznerServerType {
    fn from(r: RawServerType2) -> Self {
        Self {
            id: r.id,
            name: r.name,
            description: r.description,
            cores: r.cores,
            memory: r.memory,
            disk: r.disk,
            architecture: r.architecture,
            cpu_type: r.cpu_type,
            deprecated: r.deprecated,
            prices: r
                .prices
                .into_iter()
                .map(|p| HetznerServerTypePrice {
                    location: p.location,
                    price_monthly_net: p.price_monthly.net,
                    price_monthly_gross: p.price_monthly.gross,
                })
                .collect(),
        }
    }
}

pub async fn list_server_types() -> Result<Vec<HetznerServerType>, HetznerError> {
    let (client, token) = auth_client().await?;
    /* per_page=50 fits the entire catalogue (~30 entries) so we skip
    pagination for now. If Hetzner doubles the SKU count we'll add
    a paged loop here — the response carries `meta.pagination`. */
    let resp = client
        .get(format!("{HETZNER_API_BASE}/v1/server_types?per_page=50"))
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .await?;
    let resp = check_status(resp).await?;
    let parsed: ServerTypesResponse = resp.json().await?;
    Ok(parsed
        .server_types
        .into_iter()
        .map(HetznerServerType::from)
        .collect())
}

#[derive(Debug, Clone, Serialize)]
pub struct HetznerLocation {
    pub id: u64,
    pub name: String,
    pub description: String,
    pub country: String,
    pub city: String,
    pub network_zone: String,
}

#[derive(Debug, Deserialize)]
struct LocationsResponse {
    locations: Vec<RawHetznerLocation>,
}

#[derive(Debug, Deserialize)]
struct RawHetznerLocation {
    id: u64,
    name: String,
    description: String,
    country: String,
    city: String,
    network_zone: String,
}

impl From<RawHetznerLocation> for HetznerLocation {
    fn from(r: RawHetznerLocation) -> Self {
        Self {
            id: r.id,
            name: r.name,
            description: r.description,
            country: r.country,
            city: r.city,
            network_zone: r.network_zone,
        }
    }
}

pub async fn list_locations() -> Result<Vec<HetznerLocation>, HetznerError> {
    let (client, token) = auth_client().await?;
    let resp = client
        .get(format!("{HETZNER_API_BASE}/v1/locations"))
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .await?;
    let resp = check_status(resp).await?;
    let parsed: LocationsResponse = resp.json().await?;
    Ok(parsed
        .locations
        .into_iter()
        .map(HetznerLocation::from)
        .collect())
}

/// Catalogue image, narrowed to system images (no snapshots / backups).
/// Hetzner publishes one record per (name, architecture) combo; the
/// renderer dedupes by name when populating the picker because
/// `POST /v1/servers` matches by name and auto-picks the variant for
/// the chosen server type's architecture.
#[derive(Debug, Clone, Serialize)]
pub struct HetznerImage {
    pub id: u64,
    pub name: String,
    pub description: String,
    pub os_flavor: String,
    pub os_version: Option<String>,
    pub architecture: String,
    pub deprecated: bool,
}

#[derive(Debug, Deserialize)]
struct ImagesResponse {
    images: Vec<RawHetznerImage>,
}

#[derive(Debug, Deserialize)]
struct RawHetznerImage {
    id: u64,
    /* `name` is null for snapshots/backups; we filter to type=system
    upstream so name is always present in our subset. */
    name: Option<String>,
    description: String,
    os_flavor: String,
    os_version: Option<String>,
    architecture: String,
    #[serde(default)]
    deprecated: Option<String>,
}

impl HetznerImage {
    fn from_raw(r: RawHetznerImage) -> Option<Self> {
        Some(Self {
            id: r.id,
            name: r.name?,
            description: r.description,
            os_flavor: r.os_flavor,
            os_version: r.os_version,
            architecture: r.architecture,
            deprecated: r.deprecated.is_some(),
        })
    }
}

pub async fn list_images() -> Result<Vec<HetznerImage>, HetznerError> {
    let (client, token) = auth_client().await?;
    /* type=system filters out snapshots/backups; status=available drops
    images that are still being prepared after a recent OS bump. */
    let resp = client
        .get(format!(
            "{HETZNER_API_BASE}/v1/images?type=system&status=available&per_page=50"
        ))
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .send()
        .await?;
    let resp = check_status(resp).await?;
    let parsed: ImagesResponse = resp.json().await?;
    Ok(parsed
        .images
        .into_iter()
        .filter_map(HetznerImage::from_raw)
        .collect())
}

// ---------- Server create ----------
//
// `POST /v1/servers` is the provision entry point. Hetzner accepts a
// rich body (volumes, networks, firewalls, user_data, …); v1 only
// surfaces the four fields the Provision form actually collects, plus
// an `ssh_keys` array of project-scoped key ids so the new server boots
// with `authorized_keys` pre-populated. Extra fields land here as the
// form grows.

/// Renderer-supplied spec for `POST /v1/servers`. Field names match the
/// upstream API to keep the JSON body shape one-to-one with the docs.
/// `ssh_keys` carries Hetzner key ids (returned by `list_ssh_keys` /
/// `upload_ssh_key`) — string-form keys (name lookup) aren't exposed
/// because the renderer always has the id at picker time.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HetznerCreateServerSpec {
    pub name: String,
    pub server_type: String,
    pub location: String,
    pub image: String,
    #[serde(default)]
    pub ssh_keys: Vec<u64>,
}

#[derive(Debug, Deserialize)]
struct CreateServerResponse {
    server: RawServer,
}

/// Provision a new server in the operator's Hetzner project. Returns
/// the parsed `HetznerServer` so the caller can update the list view
/// without a follow-up `list_servers` round-trip. Hetzner returns 422
/// for invalid spec fields (unknown server_type, image, location,
/// duplicate name) — the body is forwarded verbatim via
/// [`HetznerError::Upstream`] so the form can surface the upstream
/// reason string.
pub async fn create_server(spec: &HetznerCreateServerSpec) -> Result<HetznerServer, HetznerError> {
    let (client, token) = auth_client().await?;
    let resp = client
        .post(format!("{HETZNER_API_BASE}/v1/servers"))
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .json(spec)
        .send()
        .await?;
    let resp = check_status(resp).await?;
    let parsed: CreateServerResponse = resp.json().await?;
    Ok(parsed.server.into())
}

// ---------- Tauri commands ----------

#[tauri::command]
pub async fn hetzner_list_servers() -> Result<Vec<HetznerServer>, String> {
    list_servers().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn hetzner_list_ssh_keys() -> Result<Vec<HetznerSshKey>, String> {
    list_ssh_keys().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn hetzner_upload_ssh_key(
    name: String,
    public_key: String,
) -> Result<HetznerSshKey, String> {
    upload_ssh_key(&name, &public_key)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn hetzner_create_server(spec: HetznerCreateServerSpec) -> Result<HetznerServer, String> {
    create_server(&spec).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn hetzner_list_server_types() -> Result<Vec<HetznerServerType>, String> {
    list_server_types().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn hetzner_list_locations() -> Result<Vec<HetznerLocation>, String> {
    list_locations().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn hetzner_list_images() -> Result<Vec<HetznerImage>, String> {
    list_images().await.map_err(|e| e.to_string())
}
