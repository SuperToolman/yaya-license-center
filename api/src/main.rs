mod application_protocol;
mod application_market_reset;
mod application_market;
mod application_commerce;
use application_market::*;
mod licensing;
mod skill_environment;
mod employee_knowledge;
mod website_auth;
mod ops;
mod staff_sessions;
mod user_management;
mod platform_versions;
mod platform_delivery;
mod local_docker;
mod customer_deletion;
use skill_environment::EnvironmentVariable;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    io::{Cursor, Read, Write},
    net::SocketAddr,
    path::{Component, Path as FilePath, PathBuf},
    sync::Arc,
};

use argon2::Argon2;
use axum::{
    Json, Router,
    body::{Body, Bytes, to_bytes},
    extract::{ConnectInfo, DefaultBodyLimit, Multipart, Path, Query, State},
    http::{
        HeaderMap, HeaderValue, Method, Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE, SET_COOKIE},
    },
    middleware::{self, Next},
    response::Response,
    routing::{delete, get, post},
};
use chrono::{DateTime, Datelike, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use password_hash::{PasswordHasher, SaltString};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use uuid::Uuid;
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

#[derive(Clone)]
struct AppState {
    signing_key: Arc<EncodingKey>,
    verification_key: Arc<DecodingKey>,
    database_path: PathBuf,
    sessions: Arc<RwLock<HashMap<String, AuthenticatedUser>>>,
    ops: Arc<ops::OpsState>,
}

#[derive(Clone)]
struct AuthenticatedUser {
    user_id: String,
    role: String,
    organization_id: String,
}

const FIXED_ROLES: &[&str] = &[
    "platform_admin",
    "platform_operator",
    "platform_finance",
    "provider_admin",
    "provider_sales",
];

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UserView {
    user_id: String,
    username: String,
    display_name: String,
    role: String,
    organization_id: String,
    status: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateUserRequest {
    username: String,
    password: String,
    display_name: String,
    role: String,
    organization_id: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AuditLogRecord {
    id: i64,
    created_at: i64,
    level: String,
    source: String,
    endpoint_title: String,
    message: String,
    ip_address: Option<String>,
    method: Option<String>,
    path: Option<String>,
    status_code: Option<u16>,
    duration_ms: Option<u64>,
    user_agent: Option<String>,
    response_body: Option<String>,
    customer_id: Option<String>,
    customer_name: String,
}

struct AuditLogCustomer {
    customer_id: String,
    customer_name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuditLogQuery {
    limit: Option<u32>,
    before: Option<i64>,
    start: Option<i64>,
    end: Option<i64>,
    level: Option<String>,
    customer_id: Option<String>,
    search: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
struct IssueLicenseRequest {
    order_id: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AiEmployeeEntitlement {
    id: String,
    title: String,
    expires_at: i64,
    #[serde(default)]
    template_version: String,
    #[serde(default)]
    skills: Vec<OperationSkillSnapshot>,
    #[serde(default)]
    system_prompt: String,
    #[serde(default)]
    allowed_tools: Vec<String>,
    #[serde(default)]
    application_ids: Vec<String>,
    #[serde(default, skip_serializing_if="Option::is_none")]
    knowledge: Option<employee_knowledge::KnowledgeManifest>,
}

/// A signed, customer-visible plugin selection. Secrets are intentionally not
/// part of this artifact or the license payload.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginPackageEntitlement {
    id: String,
    version: String,
    sha256: String,
    module: String,
    manifest: serde_json::Value,
    expires_at: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IssueLicenseResponse {
    license: String,
    license_id: String,
    order_id: Option<String>,
    expires_at: i64,
    module_expires_at: HashMap<String, i64>,
    module_titles: HashMap<String, String>,
    ai_employees: Vec<AiEmployeeEntitlement>,
    platform_status: String,
    module_statuses: HashMap<String, String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LicenseRecord {
    license: String,
    license_id: String,
    subject: String,
    #[serde(default)]
    customer_name_snapshot: String,
    #[serde(default)]
    order_id: Option<String>,
    #[serde(default = "default_linked_status")]
    linkage_status: String,
    modules: Vec<String>,
    issued_at: i64,
    expires_at: i64,
    #[serde(default)]
    module_expires_at: HashMap<String, i64>,
    #[serde(default)]
    ai_employees: Vec<AiEmployeeEntitlement>,
    #[serde(default = "default_unactivated_status")]
    platform_status: String,
    #[serde(default)]
    module_statuses: HashMap<String, String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LicenseView {
    license: String,
    license_id: String,
    subject: String,
    customer_name_snapshot: String,
    order_id: Option<String>,
    linkage_status: String,
    modules: Vec<String>,
    issued_at: i64,
    expires_at: i64,
    module_expires_at: HashMap<String, i64>,
    module_titles: HashMap<String, String>,
    ai_employees: Vec<AiEmployeeEntitlement>,
    ai_employee_statuses: HashMap<String, String>,
    platform_status: String,
    module_statuses: HashMap<String, String>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LicenseClaims {
    license_id: String,
    subject: String,
    #[serde(default)]
    customer_name: String,
    #[serde(default)]
    order_id: String,
    #[serde(default = "default_deployment_type")]
    deployment_type: String,
    modules: Vec<String>,
    iat: usize,
    exp: usize,
    #[serde(default)]
    module_expires_at: HashMap<String, i64>,
    #[serde(default)]
    module_titles: HashMap<String, String>,
    #[serde(default)]
    ai_employees: Vec<AiEmployeeEntitlement>,
    #[serde(default)]
    plugin_packages: Vec<PluginPackageEntitlement>,
    iss: String,
    aud: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LicenseStatus {
    valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    latest_license: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    latest_license_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    latest_issued_at: Option<i64>,
}

impl LicenseStatus {
    fn simple(valid: bool) -> Self {
        Self {
            valid,
            latest_license: None,
            latest_license_id: None,
            latest_issued_at: None,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CustomerRecord {
    customer_id: String,
    name: String,
    contact_name: String,
    contact_phone: String,
    contact_email: String,
    status: String,
    notes: String,
    created_at: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateCustomerRequest {
    name: String,
    #[serde(default)]
    contact_name: String,
    #[serde(default)]
    contact_phone: String,
    #[serde(default)]
    contact_email: String,
    #[serde(default)]
    notes: String,
}

#[derive(Deserialize)]
struct StatusRequest {
    status: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct OrderLineItem {
    kind: String,
    reference_id: Option<String>,
    title: String,
    quantity: u32,
    unit_price_cents: i64,
    entitlement_days: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateOrderRequest {
    customer_id: String,
    items: Vec<OrderLineItem>,
    due_at: Option<DateTime<Utc>>,
    #[serde(default = "default_deployment_type")]
    deployment_type: String,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    provider_id: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderRecord {
    provider_id: String,
    name: String,
    contact_name: String,
    contact_phone: String,
    status: String,
    created_at: i64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveProviderRequest {
    name: String,
    #[serde(default)]
    contact_name: String,
    #[serde(default)]
    contact_phone: String,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CommissionRule {
    rule_id: String,
    product_type: String,
    rate_basis_points: i64,
    status: String,
    effective_at: i64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveCommissionRuleRequest {
    product_type: String,
    rate_basis_points: i64,
    effective_at: Option<i64>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CommissionEntry {
    entry_id: String,
    order_id: String,
    provider_id: String,
    product_type: String,
    received_amount_cents: i64,
    rate_basis_points: i64,
    commission_amount_cents: i64,
    status: String,
    created_at: i64,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SettlementBatch {
    batch_id: String,
    provider_id: String,
    total_amount_cents: i64,
    entry_count: i64,
    status: String,
    settled_at: i64,
    notes: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateSettlementBatchRequest {
    provider_id: String,
    #[serde(default)]
    notes: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct OrderRecord {
    payment_status:String,fulfillment_status:String,
    order_id: String,
    order_no: String,
    customer_id: String,
    customer_name: String,
    items: Vec<OrderLineItem>,
    total_amount_cents: i64,
    received_amount_cents: i64,
    status: String,
    created_at: i64,
    due_at: Option<i64>,
    deployment_type: String,
    paid_at: Option<i64>,
    license_id: Option<String>,
    notes: String,
    provider_id: Option<String>,
}

fn default_deployment_type() -> String {
    "saas".to_string()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecordPaymentRequest {
    amount_cents: i64,
    method: String,
    #[serde(default)]
    reference: String,
    #[serde(default)]
    notes: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TransactionRecord {
    transaction_id: String,
    order_id: String,
    order_no: String,
    customer_id: String,
    customer_name: String,
    amount_cents: i64,
    method: String,
    reference: String,
    notes: String,
    occurred_at: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FinanceSummary {
    total_received_cents: i64,
    month_received_cents: i64,
    outstanding_cents: i64,
    paid_order_count: i64,
    pending_order_count: i64,
    active_customer_count: i64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiEmployeeProduct {
    id: String,
    title: String,
    description: String,
    category: String,
    price_cents: i64,
    billing_cycle: String,
    version: String,
    skill_ids: Vec<String>,
    system_prompt: String,
    allow_network: bool,
    allowed_tools: Vec<String>,
    application_ids: Vec<String>,
    avatar_url: Option<String>,
    status: String,
    created_at: i64,
    updated_at: i64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiEmployeeMarketProduct {
    id: String,
    title: String,
    description: String,
    category: String,
    price_cents: i64,
    billing_cycle: String,
    version: String,
    avatar_url: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AiEmployeeDeletionBlocker {
    license_id: String,
    customer_name: String,
    expires_at: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AiEmployeeDeletionEligibility {
    can_delete: bool,
    blockers: Vec<AiEmployeeDeletionBlocker>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillDeletionBlocker {
    employee_id: String,
    employee_title: String,
    employee_status: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillDeletionEligibility {
    can_delete: bool,
    blockers: Vec<SkillDeletionBlocker>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApplicationReleaseSubmission {
    submission_id: String,
    app_id: String,
    app_name: String,
    #[serde(default = "default_application_version")]
    version: String,
    status: String,
    submitted_by: String,
    #[serde(default)]
    applicant_subject: String,
    submitted_at: i64,
    snapshot: serde_json::Value,
    #[serde(default)] product_id: Option<String>,
    #[serde(default)] fingerprint: String,
    #[serde(default)] review_reason: String,
}

fn default_application_version() -> String {
    "1.0.0".to_string()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReviewApplicationSubmissionRequest {
    approved: bool,
    #[serde(default)]
    reason: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GrantApplicationEntitlementRequest {
    subject: String,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    expires_at: Option<i64>,
}

fn application_package_root() -> PathBuf {
    std::env::var_os("YAYA_APPLICATION_PACKAGES_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("runtime/application-packages"))
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MarketPurchaseResponse {
    order_id: String,
    order_no: String,
    license_id: String,
    status: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
struct CreateAiEmployeeProductRequest {
    #[serde(default)]
    id: String,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    category: String,
    price_cents: i64,
    billing_cycle: String,
    #[serde(default)]
    skill_ids: Vec<String>,
    #[serde(default)]
    system_prompt: String,
    #[serde(default)]
    allow_network: bool,
    #[serde(default)]
    allowed_tools: Vec<String>,
    #[serde(default)]
    application_ids: Vec<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct OperationSkill {
    id: String,
    title: String,
    package_name: String,
    source: String,
    version: String,
    package_path: String,
    description: String,
    instructions: String,
    status: String,
    created_at: i64,
    updated_at: i64,
    environment_variables: Vec<EnvironmentVariable>,
    environment_values: BTreeMap<String, String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OperationSkillSnapshot {
    id: String,
    title: String,
    #[serde(default)]
    package_name: String,
    #[serde(default)]
    source: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    package_path: String,
    description: String,
    instructions: String,
    #[serde(default)]
    environment_values: BTreeMap<String, String>,
}

impl From<OperationSkill> for OperationSkillSnapshot {
    fn from(skill: OperationSkill) -> Self {
        Self {
            id: skill.id,
            title: skill.title,
            package_name: skill.package_name,
            source: skill.source,
            version: skill.version,
            package_path: skill.package_path,
            description: skill.description,
            instructions: skill.instructions,
            environment_values: skill.environment_values,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
struct SaveOperationSkillRequest {
    #[serde(default)]
    id: String,
    title: String,
    #[serde(default)]
    source: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    instructions: String,
    #[serde(default)]
    environment_values: Option<BTreeMap<String, String>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlatformTool {
    id: &'static str,
    title: &'static str,
    description: &'static str,
    category: &'static str,
    group: &'static str,
    risk_level: &'static str,
}

fn default_template_version() -> String {
    "1.0.0".to_string()
}

fn generated_resource_id(prefix: &str) -> String {
    format!("{prefix}-{}", Uuid::new_v4())
}

#[derive(Serialize)]
struct SessionStatus {
    authenticated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    user: Option<UserView>,
}

#[derive(Debug, Serialize)]
struct Envelope<T: Serialize> {
    code: i32,
    message: String,
    data: Option<T>,
}

#[tokio::main]
async fn main() {
    if std::env::args().nth(1).as_deref() == Some("--file-agent") {
        ops::files_agent::run().await.expect("run local file agent");
        return;
    }
    if let Some(arg)=std::env::args().nth(1).filter(|s|s=="--market-reset-preview"||s=="--market-reset") {
        let path=std::env::var_os("YAYA_OPERATION_CENTER_DB_PATH").or_else(||std::env::var_os("LICENSE_CENTER_DB_PATH")).map(PathBuf::from).unwrap_or_else(||PathBuf::from(".yaya-operation-center.sqlite3"));
        let confirmation=if arg=="--market-reset"{Some(std::env::args().nth(2).expect("需要预览生成的确认凭据"))}else{None};
        let result=application_market_reset::command(&path,confirmation.as_deref()).expect("market reset");
        println!("{}",serde_json::to_string_pretty(&result).unwrap());return;
    }
    let private_key = required_env("LICENSE_SIGNING_PRIVATE_KEY_PEM");
    let public_key = required_env("LICENSE_SIGNING_PUBLIC_KEY_PEM");
    let initial_admin_username = std::env::var("YAYA_OPERATION_CENTER_INITIAL_ADMIN_USERNAME")
        .unwrap_or_else(|_| "admin".to_string());
    let initial_admin_password = std::env::var("YAYA_OPERATION_CENTER_INITIAL_ADMIN_PASSWORD").ok();
    let legacy_revoked_path = std::env::var_os("YAYA_OPERATION_CENTER_REVOKED_PATH")
        .or_else(|| std::env::var_os("LICENSE_CENTER_REVOKED_PATH"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".license-center-revocations.json"));
    let legacy_licenses_path = std::env::var_os("YAYA_OPERATION_CENTER_LICENSES_PATH")
        .or_else(|| std::env::var_os("LICENSE_CENTER_LICENSES_PATH"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".license-center-licenses.json"));
    let database_path = std::env::var_os("YAYA_OPERATION_CENTER_DB_PATH")
        .or_else(|| std::env::var_os("LICENSE_CENTER_DB_PATH"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".yaya-operation-center.sqlite3"));
    initialize_database(
        &database_path,
        &legacy_licenses_path,
        &legacy_revoked_path,
        &initial_admin_username,
        initial_admin_password.as_deref(),
    )
    .expect("failed to initialize license database");
    website_auth::initialize(&Connection::open(&database_path).expect("open website auth database"))
        .expect("initialize website customer authentication");
    ops::initialize(&database_path).expect("initialize staff sessions and operations database");
    platform_versions::initialize(&database_path).expect("initialize platform package database");
    platform_delivery::initialize(&database_path).expect("initialize platform delivery database");
    platform_delivery::recover(&database_path).expect("recover interrupted local Docker deployments");
    let state = AppState {
        signing_key: Arc::new(
            EncodingKey::from_rsa_pem(private_key.as_bytes())
                .expect("LICENSE_SIGNING_PRIVATE_KEY_PEM must be a valid RSA PEM"),
        ),
        verification_key: Arc::new(
            DecodingKey::from_rsa_pem(public_key.as_bytes())
                .expect("LICENSE_SIGNING_PUBLIC_KEY_PEM must be a valid RSA PEM"),
        ),
        database_path,
        sessions: Arc::new(RwLock::new(HashMap::new())),
        ops: Arc::new(ops::OpsState::from_env().expect("valid operations configuration")),
    };
    platform_delivery::runtime_state::start(state.database_path.clone());
    ops::telemetry::start(state.database_path.clone());
    ops::terminal::recover(state.database_path.clone());
    let app = Router::new()
        .merge(website_auth::router())
        .merge(ops::router(state.clone()))
        .merge(platform_versions::router())
        .merge(platform_delivery::router())
        .route(
            "/healthz",
            get(|| async { Json(success("ok", LicenseStatus::simple(true))) }),
        )
        .route("/api/licenses", post(issue_license))
        .merge(licensing::router())
        .route("/api/licenses", get(list_licenses))
        .route("/api/customers", get(list_customers).post(create_customer))
        .route(
            "/api/customers/{customer_id}",
            post(update_customer).delete(customer_deletion::delete_customer),
        )
        .route(
            "/api/customers/{customer_id}/deletion-eligibility",
            get(customer_deletion::get_eligibility),
        )
        .route(
            "/api/customers/{customer_id}/status",
            post(update_customer_status),
        )
        .route("/api/orders", get(list_orders).post(create_order))
        .route("/api/orders/{order_id}", axum::routing::put(update_order))
        .route("/api/orders/{order_id}/payment", post(record_payment))
        .route("/api/orders/{order_id}/cancel", post(cancel_order))
        .route("/api/finance/summary", get(finance_summary))
        .route("/api/transactions", get(list_transactions))
        .route(
            "/api/ai-employees",
            get(list_ai_employee_products).post(create_ai_employee_product),
        )
        .route(
            "/api/ai-employees/{employee_id}/avatar",
            get(get_ai_employee_avatar).put(upload_ai_employee_avatar),
        )
        .route(
            "/api/market/ai-employees",
            get(list_public_ai_employee_products),
        )
        .route(
            "/api/market/ai-employees/{employee_id}/package",
            get(get_owned_ai_employee_package),
        )
        .route(
            "/api/market/ai-employees/{employee_id}/skills/{skill_id}/archive",
            get(download_owned_ai_employee_skill_archive),
        )
        .route(
            "/api/market/ai-employees/{employee_id}/test-purchase",
            post(test_purchase_ai_employee),
        )
        .route(
            "/api/application-submissions",
            get(list_application_submissions).post(receive_application_submission),
        )
        .route(
            "/api/application-submissions/{submission_id}/review",
            post(review_application_submission),
        )
        .route(
            "/api/application-products/{product_id}/listing",
            post(set_application_listing),
        )
        .route(
            "/api/application-products/{product_id}/releases",
            get(list_application_product_releases),
        )
        .route("/api/application-products", get(list_application_products))
        .route("/api/application-products/{product_id}", delete(delete_application_product))
        .route("/api/application-products/import", post(import_application_package))
        .route(
            "/api/application-products/{product_id}/entitlements",
            get(list_application_entitlements).post(grant_application_entitlement),
        )
        .route(
            "/api/application-products/{product_id}/entitlements/{subject}",
            delete(revoke_application_entitlement),
        )
        .route("/api/market/application-submissions", get(publisher_application_submissions))
        .route("/api/market/applications/{product_id}/publication", get(application_publication_info))
        .route("/api/application-submissions/{submission_id}/status", get(application_submission_status))
        .route("/api/market/applications/{product_id}/commercial", get(application_commerce::list_commercial).post(application_commerce::submit_commercial))
        .route("/api/application-products/{product_id}/commercial", get(application_commerce::list_commercial).post(application_commerce::submit_commercial))
        .route("/api/application-commercial/{id}/review", post(application_commerce::review_commercial))
        .route("/api/application-commerce", get(application_commerce::overview))
        .route("/api/application-commerce/{order_id}/refund", post(application_commerce::refund))
        .route("/api/application-commerce/settlements", post(application_commerce::settle))
        .route("/api/market/application-orders", get(application_commerce::my_orders))
        .route("/api/market/application-orders/{order_id}/simulate-payment", post(application_commerce::simulate_payment))
        .route("/api/market/application-orders/{order_id}/action", post(application_commerce::order_action))
        .route("/api/market/application-revenues", get(application_commerce::publisher_revenues))
        .route("/api/market/applications/{product_id}/checkout", post(application_commerce::checkout))
        .route("/api/market/applications", get(list_public_applications))
        .route(
            "/api/market/applications/{product_id}",
            get(get_public_application),
        )
        .route(
            "/api/market/applications/{product_id}/download-ticket",
            post(create_application_download_ticket),
        )
        .route(
            "/api/market/downloads/{ticket}",
            get(download_application_package),
        )
        .route(
            "/api/ai-employees/{employee_id}/status",
            post(update_ai_employee_product_status),
        )
        .route(
            "/api/ai-employees/{employee_id}/deletion-eligibility",
            get(get_ai_employee_deletion_eligibility),
        )
        .route(
            "/api/ai-employees/{employee_id}",
            delete(delete_ai_employee_product),
        )
        .route("/api/ai-employees/{employee_id}/knowledge",get(employee_knowledge::list).post(employee_knowledge::upload).layer(DefaultBodyLimit::max(8*1024*1024)))
        .route("/api/ai-employees/{employee_id}/knowledge/{document_id}",delete(employee_knowledge::remove))
        .route("/api/market/ai-employees/{employee_id}/knowledge/{document_id}",get(employee_knowledge::download))
        .route(
            "/api/skills",
            get(list_operation_skills).post(save_operation_skill),
        )
        .route("/api/skills/import", post(import_operation_skill))
        .route("/api/platform-tools", get(list_platform_tools))
        .route(
            "/api/skills/{skill_id}/status",
            post(update_operation_skill_status),
        )
        .route(
            "/api/skills/{skill_id}/deletion-eligibility",
            get(get_operation_skill_deletion_eligibility),
        )
        .route(
            "/api/skills/{skill_id}/archive",
            post(update_operation_skill_archive),
        )
        .route("/api/skills/{skill_id}", delete(delete_operation_skill))
        .route("/api/session", get(session_status).post(create_session).delete(close_session))
        .route("/api/users", get(list_users).post(create_user))
        .route("/api/users/{user_id}", axum::routing::put(user_management::update).delete(user_management::remove))
        .route("/api/users/{user_id}/status", post(user_management::set_status))
        .route("/api/providers", get(list_providers).post(create_provider))
        .route(
            "/api/commission-rules",
            get(list_commission_rules).post(save_commission_rule),
        )
        .route("/api/commissions", get(list_commissions))
        .route(
            "/api/commissions/{entry_id}/reverse",
            post(reverse_commission),
        )
        .route(
            "/api/settlement-batches",
            get(list_settlement_batches).post(create_settlement_batch),
        )
        .route("/api/licenses/{license_id}/revoke", post(revoke_license))
        .route("/api/licenses/{license_id}/destroy", post(destroy_license))
        .route(
            "/api/licenses/{license_id}/activate",
            post(activate_license),
        )
        .route("/api/licenses/{license_id}/status", get(license_status))
        .route("/api/logs", get(list_audit_logs))
        .with_state(state.clone())
        .layer(DefaultBodyLimit::max(66 * 1024 * 1024))
        .layer(middleware::from_fn_with_state(state.clone(), audit_request))
        .layer(TraceLayer::new_for_http())
        .layer(
            CorsLayer::new()
                .allow_origin([
                    HeaderValue::from_static("http://127.0.0.1:8778"),
                    HeaderValue::from_static("http://localhost:8778"),
                ])
                .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
                .allow_headers([AUTHORIZATION, CONTENT_TYPE])
                .allow_credentials(true),
        );
    let host = std::env::var("YAYA_OPERATION_CENTER_HOST")
        .or_else(|_| std::env::var("LICENSE_CENTER_HOST"))
        .unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = std::env::var("YAYA_OPERATION_CENTER_PORT")
        .or_else(|_| std::env::var("LICENSE_CENTER_PORT"))
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(8779);
    let address: SocketAddr = format!("{host}:{port}")
        .parse()
        .expect("invalid YAYA_OPERATION_CENTER_HOST or YAYA_OPERATION_CENTER_PORT");
    println!("Yaya Operation Center listening on http://{address}");
    axum::serve(
        tokio::net::TcpListener::bind(address)
            .await
            .expect("failed to bind Yaya Operation Center"),
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .expect("Yaya Operation Center stopped unexpectedly");
}

async fn audit_request(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let started = std::time::Instant::now();
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let skip_audit = is_operation_frontend_request(request.headers(), &path);
    let user_agent = request
        .headers()
        .get("user-agent")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let response = next.run(request).await;
    if skip_audit {
        return response;
    }
    let status_code = response.status().as_u16();
    let response_is_json = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/json"));
    let level = if status_code >= 500 {
        "error"
    } else if status_code >= 400 {
        "warning"
    } else if method == Method::GET {
        "debug"
    } else {
        "info"
    };
    let method_text = method.to_string();
    let message = format!("{method_text} {path} -> {status_code}");
    let mut record = AuditLogRecord {
        id: 0,
        created_at: Utc::now().timestamp(),
        level: level.to_string(),
        source: "request".to_string(),
        endpoint_title: endpoint_title(&path, &method).to_string(),
        message,
        ip_address: Some(peer.ip().to_string()),
        method: Some(method_text),
        path: Some(path),
        status_code: Some(status_code),
        duration_ms: Some(started.elapsed().as_millis().min(u64::MAX as u128) as u64),
        user_agent,
        response_body: None,
        customer_id: None,
        customer_name: "无客户".to_string(),
    };
    let response = if response_is_json {
        let (parts, body) = response.into_parts();
        match to_bytes(body, usize::MAX).await {
            Ok(bytes) => {
                record.response_body = sanitize_response_body(&bytes);
                Response::from_parts(parts, Body::from(bytes))
            }
            Err(_) => Response::from_parts(parts, Body::empty()),
        }
    } else {
        response
    };
    let database_path = state.database_path.clone();
    let _ = tokio::task::spawn_blocking(move || insert_audit_log(&database_path, &record)).await;
    response
}

fn is_operation_frontend_request(headers: &HeaderMap, path: &str) -> bool {
    if path == "/api/logs" {
        return true;
    }
    let configured_origin = std::env::var("YAYA_OPERATION_CENTER_WEB_ORIGIN")
        .ok()
        .filter(|value| !value.trim().is_empty());
    ["origin", "referer"]
        .iter()
        .filter_map(|name| headers.get(*name))
        .filter_map(|value| value.to_str().ok())
        .any(|value| {
            configured_origin
                .as_deref()
                .is_some_and(|origin| value.starts_with(origin))
                || value.starts_with("http://127.0.0.1:8778")
                || value.starts_with("http://localhost:8778")
        })
}

fn endpoint_title(path: &str, method: &Method) -> &'static str {
    match path {
        "/healthz" => "服务健康检查",
        "/api/session" => "管理员会话",
        "/api/platform-tools" => "平台工具列表",
        "/api/logs" => "运营审计日志",
        "/api/customers" => {
            if *method == Method::GET {
                "客户列表"
            } else {
                "创建客户"
            }
        }
        "/api/orders" => {
            if *method == Method::GET {
                "订单列表"
            } else {
                "创建订单"
            }
        }
        "/api/licenses" => {
            if *method == Method::GET {
                "许可证列表"
            } else {
                "签发许可证"
            }
        }
        "/api/finance/summary" => "财务汇总",
        "/api/transactions" => "回款流水",
        "/api/ai-employees" => "AI 员工商品",
        "/api/market/ai-employees" => "AI 员工市场目录",
        "/api/skills" => "Skills 管理",
        "/api/skills/import" => "导入 Skill",
        _ if path.starts_with("/api/licenses/") && path.ends_with("/status") => "许可证状态校验",
        _ if path.starts_with("/api/licenses/") && path.ends_with("/activate") => "许可证激活",
        _ if path.starts_with("/api/licenses/") && path.ends_with("/destroy") => "许可证销毁",
        _ if path.starts_with("/api/orders/") && path.ends_with("/payment") => "登记订单回款",
        _ if path.starts_with("/api/orders/") && path.ends_with("/cancel") => "取消订单",
        _ if path.starts_with("/api/orders/") => "编辑订单",
        _ if path.starts_with("/api/customers/") && path.ends_with("/deletion-eligibility") => "客户删除条件检查",
        _ if path.starts_with("/api/customers/") && *method == Method::DELETE => "删除客户",
        _ if path.starts_with("/api/customers/") => "客户资料",
        _ if path.starts_with("/api/ai-employees/") => "AI 员工商品状态",
        _ if path.starts_with("/api/market/ai-employees/") && path.ends_with("/test-purchase") => {
            "AI 员工测试购买"
        }
        _ if path.starts_with("/api/skills/") => "Skill 状态",
        _ => "运营接口",
    }
}

fn sanitize_response_body(bytes: &[u8]) -> Option<String> {
    let mut value = serde_json::from_slice::<serde_json::Value>(bytes).ok()?;
    redact_json_value(&mut value);
    let text = serde_json::to_string_pretty(&value).ok()?;
    Some(if text.chars().count() > 32_768 {
        format!(
            "{}\n... [响应已截断]",
            text.chars().take(32_768).collect::<String>()
        )
    } else {
        text
    })
}

fn redact_json_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                let key = key.to_ascii_lowercase();
                if [
                    "license",
                    "latestlicense",
                    "authorization",
                    "cookie",
                    "admintoken",
                    "password",
                    "apikey",
                    "secret",
                    "token",
                    "sessiontoken",
                    "registrationtoken",
                    "environmentvalues",
                ]
                .contains(&key.as_str())
                {
                    *child = serde_json::Value::String("[REDACTED]".to_string());
                } else {
                    redact_json_value(child);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                redact_json_value(item);
            }
        }
        _ => {}
    }
}

async fn list_audit_logs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AuditLogQuery>,
) -> Result<Json<Envelope<Vec<AuditLogRecord>>>, (StatusCode, Json<Envelope<()>>)> {
    require_admin(&headers, &state).await?;
    if query.start.zip(query.end).is_some_and(|(start, end)| start >= end) {
        return Err(error(StatusCode::BAD_REQUEST, "开始日期必须早于结束日期"));
    }
    if query.level.as_deref().is_some_and(|level| !["debug", "info", "warning", "error"].contains(&level)) {
        return Err(error(StatusCode::BAD_REQUEST, "日志级别不合法"));
    }
    let mut logs = list_audit_log_records(&state.database_path, &query)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "日志读取失败"))?;
    for log in &mut logs {
        if log.endpoint_title == "运营接口" {
            if let (Some(path), Some(method)) = (&log.path, &log.method) {
                if let Ok(method) = Method::from_bytes(method.as_bytes()) {
                    log.endpoint_title = endpoint_title(path, &method).to_string();
                }
            }
        }
    }
    Ok(Json(success("日志已读取", logs)))
}

async fn issue_license(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<IssueLicenseRequest>,
) -> Result<Json<Envelope<IssueLicenseResponse>>, (StatusCode, Json<Envelope<()>>)> {
    let actor = licensing::operator(&headers, &state).await?;
    platform_versions::check_origin(&headers)?;
    let order_id = request.order_id.trim();
    if order_id.is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "orderId 不能为空"));
    }
    let mut connection = ops::db::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "订单读取失败"))?;
    let catalog_connection = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|_| error(StatusCode::CONFLICT,"客户授权正在更新，请稍后重试"))?;
    let order = get_order(&catalog_connection, order_id)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "订单读取失败"))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "订单不存在"))?;
    if let Some(id) = order.license_id.as_deref() {
        let existing = get_license_record(&state.database_path,id).map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"许可证读取失败"))?.ok_or_else(||error(StatusCode::CONFLICT,"订单授权记录不完整"))?;
        return Ok(Json(success("该订单已经完成授权交付",IssueLicenseResponse{license:existing.license,license_id:existing.license_id,order_id:existing.order_id,expires_at:existing.expires_at,module_expires_at:existing.module_expires_at,module_titles:HashMap::new(),ai_employees:existing.ai_employees,platform_status:existing.platform_status,module_statuses:existing.module_statuses})));
    }
    if order.status != "paid" {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "只有已回款订单可以签发许可证",
        ));
    }
    if order.license_id.is_some() {
        return Err(error(StatusCode::CONFLICT, "该订单已经签发许可证"));
    }

    licensing::validate_items(&order.items)?;
    let active:bool=catalog_connection.query_row("SELECT EXISTS(SELECT 1 FROM customers WHERE customer_id=? AND status='active')",[&order.customer_id],|r|r.get(0)).map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"客户读取失败"))?;
    if !active {return Err(error(StatusCode::CONFLICT,"停用客户不可交付授权"));}
    let now = Utc::now().timestamp() as usize;
    let issued_at = Utc::now();
    let previous = licensing::latest(&catalog_connection,&order.customer_id).map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"客户已有权益读取失败"))?;
    let previous_claims = previous.as_ref().map(|record| {
        let mut validation=Validation::new(Algorithm::RS256);
        validation.validate_exp=false;
        validation.set_issuer(&["yaya-operation-center","yaya-license-center"]);
        validation.set_audience(&["yaya-low-code"]);
        decode::<LicenseClaims>(&record.license,&state.verification_key,&validation).map(|v|v.claims)
    }).transpose().map_err(|_|error(StatusCode::CONFLICT,"客户原授权签名无法验证，不能自动累计权益"))?;
    if let Some(claims)=&previous_claims {
        if claims.deployment_type != order.deployment_type {return Err(error(StatusCode::CONFLICT,"增购和续费须使用客户原授权的部署方式"));}
    }
    let mut modules = previous_claims.as_ref().map(|c|c.modules.clone()).unwrap_or_default();
    let mut module_titles = previous_claims.as_ref().map(|c|c.module_titles.clone()).unwrap_or_default();
    let mut module_expires_at = previous_claims.as_ref().map(|c|c.module_expires_at.clone()).unwrap_or_default();
    // Older signatures omitted explicit module expirations; materialize their original cap.
    if let Some(claims)=&previous_claims {for module in &claims.modules {if module!="platform" {module_expires_at.entry(module.clone()).or_insert(claims.exp as i64);}}}
    let mut platform_expires_at = previous_claims.as_ref().map(|c|c.exp as i64);
    let mut ordered_ai_employees: HashMap<String, (String, i64)> = HashMap::new();
    for item in &order.items {
        if !["platform", "module", "ai_employee"].contains(&item.kind.as_str()) {
            continue;
        }
        let days = item
            .entitlement_days
            .filter(|days| *days > 0)
            .ok_or_else(|| {
                error(
                    StatusCode::BAD_REQUEST,
                    format!("订单商品“{}”缺少有效的授权天数", item.title),
                )
            })?;
        if days>36500 {return Err(error(StatusCode::BAD_REQUEST,"授权天数不得超过 36500 天"));}

        match item.kind.as_str() {
            "platform" => {
                platform_expires_at = Some(licensing::extend(platform_expires_at.unwrap_or(0),issued_at.timestamp(),days)?);
                module_titles.insert("platform".to_string(), item.title.trim().to_string());
            }
            "module" => {
                let module = item.reference_id.as_deref().map(str::trim).unwrap_or("");
                if module.is_empty() {
                    return Err(error(StatusCode::BAD_REQUEST, "订单模块缺少 referenceId"));
                }
                modules.push(module.to_string());
                module_titles.insert(module.to_string(), item.title.trim().to_string());
                let expiry=licensing::extend(module_expires_at.get(module).copied().unwrap_or(0),issued_at.timestamp(),days)?;
                module_expires_at.insert(module.to_string(),expiry);
            }
            "ai_employee" => {
                let employee_id = item.reference_id.as_deref().map(str::trim).unwrap_or("");
                if employee_id.is_empty() {
                    return Err(error(
                        StatusCode::BAD_REQUEST,
                        "订单 AI 员工缺少 referenceId",
                    ));
                }
                let prior=ordered_ai_employees.get(employee_id).map(|(_,e)|*e).or_else(||previous_claims.as_ref().and_then(|c|c.ai_employees.iter().find(|e|e.id==employee_id).map(|e|e.expires_at))).unwrap_or(0);
                ordered_ai_employees.insert(employee_id.to_string(),(item.title.trim().to_string(),licensing::extend(prior,issued_at.timestamp(),days)?));
            }
            _ => {}
        }
    }
    let expires_at = platform_expires_at
        .ok_or_else(|| error(StatusCode::BAD_REQUEST, "首次授权需购买低代码平台；已有授权可单独增购或续费"))?;
    if expires_at<=issued_at.timestamp(){return Err(error(StatusCode::BAD_REQUEST,"平台授权已过期，请在订单中续费平台后再交付增购权益"));}
    modules.push("platform".to_string());
    modules.sort();
    modules.dedup();

    let initial_module_statuses = modules
        .iter()
        .map(|module| (module.clone(), "unactivated".to_string()))
        .collect::<HashMap<_, _>>();
    let mut ai_employees_by_id: HashMap<String,AiEmployeeEntitlement> = previous_claims.as_ref().map(|c|c.ai_employees.iter().cloned().map(|e|(e.id.clone(),e)).collect()).unwrap_or_default();
    for (id, (title, entitlement_expires_at)) in ordered_ai_employees {
        let product = get_ai_employee_product(&catalog_connection, &id)
            .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "AI 员工模板读取失败"))?
            .ok_or_else(|| {
                error(
                    StatusCode::BAD_REQUEST,
                    format!("订单中的 AI 员工不存在：{id}"),
                )
            })?;
        let (template_version, skills, system_prompt) = {
            let mut all_skill_ids = product.skill_ids.clone();
            all_skill_ids.sort();
            all_skill_ids.dedup();
            let mut skill_snapshots = Vec::new();
            for skill_id in all_skill_ids {
                if let Some(skill) =
                    get_operation_skill(&catalog_connection, &skill_id).map_err(|_| {
                        error(StatusCode::INTERNAL_SERVER_ERROR, "AI 员工 Skill 读取失败")
                    })?
                {
                    skill_snapshots.push(OperationSkillSnapshot::from(skill));
                }
            }
            (
                product.version,
                skill_snapshots,
                product.system_prompt.clone(),
            )
        };
        ai_employees_by_id.insert(
            id.clone(),
            AiEmployeeEntitlement {
                id,
                title,
                expires_at: entitlement_expires_at,
                template_version,
                skills,
                system_prompt,
                allowed_tools: product.allowed_tools.clone(),
                application_ids: product.application_ids.clone(),
                knowledge: None,
            },
        );
    }
    let mut ai_employees = ai_employees_by_id.into_values().collect::<Vec<_>>();
    ai_employees.sort_by(|left, right| left.title.cmp(&right.title));
    let license_id = format!("lic_{}", Uuid::new_v4().simple());
    let claims = LicenseClaims {
        license_id: license_id.clone(),
        subject: order.customer_id.clone(),
        customer_name: order.customer_name.clone(),
        order_id: order.order_id.clone(),
        deployment_type: order.deployment_type.clone(),
        modules: modules.clone(),
        iat: now,
        exp: expires_at as usize,
        module_expires_at: module_expires_at.clone(),
        module_titles: module_titles.clone(),
        ai_employees: ai_employees.clone(),
        plugin_packages: previous_claims.as_ref().map(|c|c.plugin_packages.clone()).unwrap_or_default(),
        iss: "yaya-operation-center".to_string(),
        aud: "yaya-low-code".to_string(),
    };
    let license = encode(&Header::new(Algorithm::RS256), &claims, &state.signing_key)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "许可证签发失败"))?;
    let response = IssueLicenseResponse {
        license: license.clone(),
        license_id: license_id.clone(),
        order_id: Some(order.order_id.clone()),
        expires_at: claims.exp as i64,
        module_expires_at: module_expires_at.clone(),
        module_titles,
        ai_employees: ai_employees.clone(),
        platform_status: "unactivated".to_string(),
        module_statuses: initial_module_statuses.clone(),
    };
    let record = LicenseRecord {
        license,
        license_id,
        subject: claims.subject.clone(),
        customer_name_snapshot: order.customer_name,
        order_id: Some(order.order_id),
        linkage_status: "linked".to_string(),
        modules: claims.modules.clone(),
        issued_at: now as i64,
        expires_at: claims.exp as i64,
        module_expires_at,
        ai_employees,
        platform_status: "unactivated".to_string(),
        module_statuses: initial_module_statuses,
    };
    insert_license_into_connection(&catalog_connection,&record).map_err(|_|error(StatusCode::CONFLICT,"该订单已经签发或授权保存失败"))?;
    licensing::register(&catalog_connection,&record,&actor.user_id).map_err(|_|error(StatusCode::CONFLICT,"客户授权版本保存失败"))?;
    let updated=catalog_connection.execute("UPDATE orders SET status='fulfilled',license_id=? WHERE order_id=? AND status='paid' AND license_id IS NULL",params![record.license_id,record.order_id]).map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"订单履约保存失败"))?;
    if updated!=1 {return Err(error(StatusCode::CONFLICT,"订单状态已变化"));}
    catalog_connection.commit().map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "许可证和订单状态保存失败",
        )
    })?;
    Ok(Json(success("许可证已签发", response)))
}

async fn list_licenses(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Envelope<Vec<LicenseView>>>, (StatusCode, Json<Envelope<()>>)> {
    require_admin(&headers, &state).await?;
    let licenses = list_license_records(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "许可证列表读取失败"))?;
    Ok(Json(success(
        "许可证列表已读取",
        licenses
            .iter()
            .map(|license| license_view(license))
            .collect(),
    )))
}

async fn list_customers(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Envelope<Vec<CustomerRecord>>>, (StatusCode, Json<Envelope<()>>)> {
    require_admin(&headers, &state).await?;
    let connection = Connection::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "客户数据读取失败"))?;
    let mut statement = connection
        .prepare(
            "SELECT customer_id, name, contact_name, contact_phone, contact_email,
                    status, notes, created_at
             FROM customers ORDER BY created_at DESC",
        )
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "客户数据读取失败"))?;
    let customers = statement
        .query_map([], |row| {
            Ok(CustomerRecord {
                customer_id: row.get(0)?,
                name: row.get(1)?,
                contact_name: row.get(2)?,
                contact_phone: row.get(3)?,
                contact_email: row.get(4)?,
                status: row.get(5)?,
                notes: row.get(6)?,
                created_at: row.get(7)?,
            })
        })
        .and_then(Iterator::collect)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "客户数据读取失败"))?;
    Ok(Json(success("客户列表已读取", customers)))
}

async fn create_customer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateCustomerRequest>,
) -> Result<Json<Envelope<CustomerRecord>>, (StatusCode, Json<Envelope<()>>)> {
    require_admin(&headers, &state).await?;
    let name = request.name.trim();
    if name.is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "客户名称不能为空"));
    }
    let phone = request.contact_phone.trim();
    if phone.is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "联系电话（手机号）不能为空"));
    }
    let customer = CustomerRecord {
        customer_id: format!("cus_{}", Uuid::new_v4().simple()),
        name: name.to_string(),
        contact_name: request.contact_name.trim().to_string(),
        contact_phone: phone.to_string(),
        contact_email: request.contact_email.trim().to_string(),
        status: "active".to_string(),
        notes: request.notes.trim().to_string(),
        created_at: Utc::now().timestamp(),
    };
    let mut connection = Connection::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "客户保存失败"))?;
    // Serialize the duplicate check and insert without changing legacy customer data.
    let transaction = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "客户保存失败"))?;
    validate_new_customer(&transaction, name, phone)?;
    transaction
        .execute(
            "INSERT INTO customers (
                customer_id, name, contact_name, contact_phone, contact_email,
                status, notes, created_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                customer.customer_id,
                customer.name,
                customer.contact_name,
                customer.contact_phone,
                customer.contact_email,
                customer.status,
                customer.notes,
                customer.created_at,
            ],
        )
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "客户保存失败"))?;
    transaction.commit()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "客户保存失败"))?;
    Ok(Json(success("客户已创建", customer)))
}

fn validate_new_customer(
    connection: &Connection,
    name: &str,
    phone: &str,
) -> Result<(), (StatusCode, Json<Envelope<()>>)> {
    let name = name.trim();
    let phone = phone.trim();
    if name.is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "客户名称不能为空"));
    }
    if phone.is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "联系电话（手机号）不能为空"));
    }
    let (name_exists, phone_exists): (bool, bool) = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM customers WHERE TRIM(name) = ?),
                EXISTS(SELECT 1 FROM customers WHERE TRIM(contact_phone) = ?)",
        params![name, phone],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "客户重复校验失败"))?;
    if name_exists {
        return Err(error(StatusCode::CONFLICT, "客户名称已存在，请勿重复创建"));
    }
    if phone_exists {
        return Err(error(StatusCode::CONFLICT, "联系电话（手机号）已存在，请勿重复创建"));
    }
    Ok(())
}

async fn update_customer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(customer_id): Path<String>,
    Json(request): Json<CreateCustomerRequest>,
) -> Result<Json<Envelope<CustomerRecord>>, (StatusCode, Json<Envelope<()>>)> {
    require_admin(&headers, &state).await?;
    let name = request.name.trim();
    if name.is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "客户名称不能为空"));
    }
    let connection = Connection::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "客户资料更新失败"))?;
    let updated = connection
        .execute(
            "UPDATE customers SET
                name = ?, contact_name = ?, contact_phone = ?, contact_email = ?, notes = ?
             WHERE customer_id = ?",
            params![
                name,
                request.contact_name.trim(),
                request.contact_phone.trim(),
                request.contact_email.trim(),
                request.notes.trim(),
                customer_id,
            ],
        )
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "客户资料更新失败"))?;
    if updated == 0 {
        return Err(error(StatusCode::NOT_FOUND, "客户不存在"));
    }
    let customer = get_customer(&connection, &customer_id)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "客户数据读取失败"))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "客户不存在"))?;
    Ok(Json(success("客户资料已更新", customer)))
}

async fn update_customer_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(customer_id): Path<String>,
    Json(request): Json<StatusRequest>,
) -> Result<Json<Envelope<CustomerRecord>>, (StatusCode, Json<Envelope<()>>)> {
    require_admin(&headers, &state).await?;
    if !["active", "inactive"].contains(&request.status.as_str()) {
        return Err(error(StatusCode::BAD_REQUEST, "客户状态无效"));
    }
    let connection = Connection::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "客户状态更新失败"))?;
    let updated = connection
        .execute(
            "UPDATE customers SET status = ? WHERE customer_id = ?",
            params![request.status, customer_id],
        )
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "客户状态更新失败"))?;
    if updated == 0 {
        return Err(error(StatusCode::NOT_FOUND, "客户不存在"));
    }
    let customer = get_customer(&connection, &customer_id)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "客户数据读取失败"))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "客户不存在"))?;
    Ok(Json(success("客户状态已更新", customer)))
}

async fn list_orders(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Envelope<Vec<OrderRecord>>>, (StatusCode, Json<Envelope<()>>)> {
    let user = require_authenticated(&headers, &state).await?;
    let provider_id = ["provider_admin", "provider_sales"]
        .contains(&user.role.as_str())
        .then_some(user.organization_id.as_str());
    let orders = load_orders(&state.database_path, provider_id)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "订单列表读取失败"))?;
    Ok(Json(success("订单列表已读取", orders)))
}

async fn list_providers(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Envelope<Vec<ProviderRecord>>>, (StatusCode, Json<Envelope<()>>)> {
    require_admin(&headers, &state).await?;
    let connection = Connection::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "服务商读取失败"))?;
    let mut statement = connection.prepare("SELECT provider_id, name, contact_name, contact_phone, status, created_at FROM providers ORDER BY created_at DESC").map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "服务商读取失败"))?;
    let records = statement
        .query_map([], |r| {
            Ok(ProviderRecord {
                provider_id: r.get(0)?,
                name: r.get(1)?,
                contact_name: r.get(2)?,
                contact_phone: r.get(3)?,
                status: r.get(4)?,
                created_at: r.get(5)?,
            })
        })
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "服务商读取失败"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "服务商读取失败"))?;
    Ok(Json(success("服务商已读取", records)))
}

async fn create_provider(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<SaveProviderRequest>,
) -> Result<Json<Envelope<ProviderRecord>>, (StatusCode, Json<Envelope<()>>)> {
    require_platform_admin(&headers, &state).await?;
    if request.name.trim().is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "服务商名称不能为空"));
    }
    let record = ProviderRecord {
        provider_id: generated_resource_id("prv"),
        name: request.name.trim().to_string(),
        contact_name: request.contact_name.trim().to_string(),
        contact_phone: request.contact_phone.trim().to_string(),
        status: "active".to_string(),
        created_at: Utc::now().timestamp(),
    };
    Connection::open(&state.database_path).map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "服务商保存失败"))?.execute("INSERT INTO providers (provider_id,name,contact_name,contact_phone,status,created_at) VALUES (?,?,?,?,?,?)", params![record.provider_id,record.name,record.contact_name,record.contact_phone,record.status,record.created_at]).map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "服务商保存失败"))?;
    Ok(Json(success("服务商已创建", record)))
}

async fn list_commission_rules(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Envelope<Vec<CommissionRule>>>, (StatusCode, Json<Envelope<()>>)> {
    require_admin(&headers, &state).await?;
    let connection = Connection::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "提成配置读取失败"))?;
    let mut statement=connection.prepare("SELECT rule_id,product_type,rate_basis_points,status,effective_at FROM commission_rules ORDER BY effective_at DESC").map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR,"提成配置读取失败"))?;
    let records = statement
        .query_map([], |r| {
            Ok(CommissionRule {
                rule_id: r.get(0)?,
                product_type: r.get(1)?,
                rate_basis_points: r.get(2)?,
                status: r.get(3)?,
                effective_at: r.get(4)?,
            })
        })
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "提成配置读取失败"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "提成配置读取失败"))?;
    Ok(Json(success("提成配置已读取", records)))
}

async fn save_commission_rule(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<SaveCommissionRuleRequest>,
) -> Result<Json<Envelope<CommissionRule>>, (StatusCode, Json<Envelope<()>>)> {
    require_platform_admin(&headers, &state).await?;
    if request.product_type.trim().is_empty() || !(0..=10_000).contains(&request.rate_basis_points)
    {
        return Err(error(StatusCode::BAD_REQUEST, "产品类型或返点比例无效"));
    }
    let record = CommissionRule {
        rule_id: generated_resource_id("crl"),
        product_type: request.product_type.trim().to_string(),
        rate_basis_points: request.rate_basis_points,
        status: "active".to_string(),
        effective_at: request
            .effective_at
            .unwrap_or_else(|| Utc::now().timestamp()),
    };
    Connection::open(&state.database_path).map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"提成配置保存失败"))?.execute("INSERT INTO commission_rules (rule_id,product_type,rate_basis_points,status,effective_at) VALUES (?,?,?,?,?)",params![record.rule_id,record.product_type,record.rate_basis_points,record.status,record.effective_at]).map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"提成配置保存失败"))?;
    Ok(Json(success("提成配置已保存", record)))
}

async fn list_commissions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Envelope<Vec<CommissionEntry>>>, (StatusCode, Json<Envelope<()>>)> {
    let user = require_authenticated(&headers, &state).await?;
    let connection = Connection::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "返利台账读取失败"))?;
    let provider_scope = ["provider_admin", "provider_sales"].contains(&user.role.as_str());
    let sql = if provider_scope {
        "SELECT entry_id,order_id,provider_id,product_type,received_amount_cents,rate_basis_points,commission_amount_cents,status,created_at FROM commission_entries WHERE provider_id = ? ORDER BY created_at DESC"
    } else {
        "SELECT entry_id,order_id,provider_id,product_type,received_amount_cents,rate_basis_points,commission_amount_cents,status,created_at FROM commission_entries ORDER BY created_at DESC"
    };
    let mut statement = connection
        .prepare(sql)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "返利台账读取失败"))?;
    let map_row = |r: &rusqlite::Row<'_>| {
        Ok(CommissionEntry {
            entry_id: r.get(0)?,
            order_id: r.get(1)?,
            provider_id: r.get(2)?,
            product_type: r.get(3)?,
            received_amount_cents: r.get(4)?,
            rate_basis_points: r.get(5)?,
            commission_amount_cents: r.get(6)?,
            status: r.get(7)?,
            created_at: r.get(8)?,
        })
    };
    let records = if provider_scope {
        statement.query_map([user.organization_id], map_row)
    } else {
        statement.query_map([], map_row)
    }
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "返利台账读取失败"))?
    .collect::<Result<Vec<_>, _>>()
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "返利台账读取失败"))?;
    Ok(Json(success("返利台账已读取", records)))
}

async fn create_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateOrderRequest>,
) -> Result<Json<Envelope<OrderRecord>>, (StatusCode, Json<Envelope<()>>)> {
    require_admin(&headers, &state).await?;
    if !["saas", "local"].contains(&request.deployment_type.as_str()) {
        return Err(error(StatusCode::BAD_REQUEST, "平台类型无效"));
    }
    licensing::validate_items(&request.items)?;
    if request.items.is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "订单至少需要一个商品项"));
    }
    if request.items.iter().any(|item| {
        item.title.trim().is_empty()
            || item.quantity == 0
            || item.unit_price_cents < 0
            || !["platform", "module", "ai_employee", "service"].contains(&item.kind.as_str())
    }) {
        return Err(error(StatusCode::BAD_REQUEST, "订单商品项不完整或类型无效"));
    }
    let total_amount_cents = request.items.iter().try_fold(0_i64, |total, item| {
        total.checked_add(item.unit_price_cents.checked_mul(item.quantity.into())?)
    });
    let Some(total_amount_cents) = total_amount_cents.filter(|amount| *amount > 0) else {
        return Err(error(StatusCode::BAD_REQUEST, "订单金额必须大于 0"));
    };
    let connection = Connection::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "订单保存失败"))?;
    let customer = get_customer(&connection, request.customer_id.trim())
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "客户数据读取失败"))?
        .ok_or_else(|| error(StatusCode::BAD_REQUEST, "客户不存在"))?;
    if customer.status != "active" {
        return Err(error(StatusCode::BAD_REQUEST, "停用客户不能创建订单"));
    }
    if let Some(provider_id) = request.provider_id.as_deref() {
        let active: Option<String> = connection
            .query_row(
                "SELECT status FROM providers WHERE provider_id = ?",
                [provider_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "服务商读取失败"))?;
        if active.as_deref() != Some("active") {
            return Err(error(StatusCode::BAD_REQUEST, "接单服务商不存在或已停用"));
        }
    }
    let now = Utc::now().timestamp();
    let order_id = format!("ord_{}", Uuid::new_v4().simple());
    let order_no = format!(
        "YY{}-{}",
        Utc::now().format("%Y%m%d"),
        &Uuid::new_v4().simple().to_string()[..6].to_uppercase()
    );
    connection
        .execute(
            "INSERT INTO orders (
                order_id, order_no, customer_id, items_json, total_amount_cents,
                status, created_at, due_at, deployment_type, notes, provider_id
             ) VALUES (?, ?, ?, ?, ?, 'pending_payment', ?, ?, ?, ?, ?)",
            params![
                order_id,
                order_no,
                customer.customer_id,
                serde_json::to_string(&request.items).expect("order items are serializable"),
                total_amount_cents,
                now,
                request.due_at.map(|value| value.timestamp()),
                request.deployment_type,
                request.notes.trim(),
                request.provider_id,
            ],
        )
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "订单保存失败"))?;
    let order = get_order(&connection, &order_id)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "订单读取失败"))?
        .ok_or_else(|| error(StatusCode::INTERNAL_SERVER_ERROR, "订单保存失败"))?;
    Ok(Json(success("订单已创建", order)))
}

async fn list_settlement_batches(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Envelope<Vec<SettlementBatch>>>, (StatusCode, Json<Envelope<()>>)> {
    let user = require_authenticated(&headers, &state).await?;
    let scoped = ["provider_admin", "provider_sales"].contains(&user.role.as_str());
    let connection = Connection::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "结算批次读取失败"))?;
    let sql = if scoped {
        "SELECT batch_id,provider_id,total_amount_cents,entry_count,status,settled_at,notes FROM settlement_batches WHERE provider_id = ? ORDER BY settled_at DESC"
    } else {
        "SELECT batch_id,provider_id,total_amount_cents,entry_count,status,settled_at,notes FROM settlement_batches ORDER BY settled_at DESC"
    };
    let mut statement = connection
        .prepare(sql)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "结算批次读取失败"))?;
    let map = |r: &rusqlite::Row<'_>| {
        Ok(SettlementBatch {
            batch_id: r.get(0)?,
            provider_id: r.get(1)?,
            total_amount_cents: r.get(2)?,
            entry_count: r.get(3)?,
            status: r.get(4)?,
            settled_at: r.get(5)?,
            notes: r.get(6)?,
        })
    };
    let batches = if scoped {
        statement.query_map([user.organization_id], map)
    } else {
        statement.query_map([], map)
    }
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "结算批次读取失败"))?
    .collect::<Result<Vec<_>, _>>()
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "结算批次读取失败"))?;
    Ok(Json(success("结算批次已读取", batches)))
}

async fn create_settlement_batch(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateSettlementBatchRequest>,
) -> Result<Json<Envelope<SettlementBatch>>, (StatusCode, Json<Envelope<()>>)> {
    require_platform_admin(&headers, &state).await?;
    let mut connection = Connection::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "结算批次保存失败"))?;
    let transaction = connection
        .transaction()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "结算批次保存失败"))?;
    let (total, count):(i64,i64)=transaction.query_row("SELECT COALESCE(SUM(commission_amount_cents),0),COUNT(*) FROM commission_entries WHERE provider_id = ? AND status = 'pending_settlement'",[request.provider_id.as_str()],|r|Ok((r.get(0)?,r.get(1)?))).map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"返利台账读取失败"))?;
    if count == 0 {
        return Err(error(StatusCode::BAD_REQUEST, "该服务商没有待结算返利"));
    }
    let batch = SettlementBatch {
        batch_id: generated_resource_id("stl"),
        provider_id: request.provider_id,
        total_amount_cents: total,
        entry_count: count,
        status: "settled".to_string(),
        settled_at: Utc::now().timestamp(),
        notes: request.notes.trim().to_string(),
    };
    transaction.execute("INSERT INTO settlement_batches (batch_id,provider_id,total_amount_cents,entry_count,status,settled_at,notes) VALUES (?,?,?,?,?,?,?)",params![batch.batch_id,batch.provider_id,batch.total_amount_cents,batch.entry_count,batch.status,batch.settled_at,batch.notes]).map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"结算批次保存失败"))?;
    transaction.execute("UPDATE commission_entries SET status = 'settled', settlement_batch_id = ? WHERE provider_id = ? AND status = 'pending_settlement'",params![batch.batch_id,batch.provider_id]).map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"返利台账更新失败"))?;
    transaction
        .commit()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "结算批次保存失败"))?;
    Ok(Json(success("返利已结算", batch)))
}

async fn reverse_commission(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(entry_id): Path<String>,
) -> Result<Json<Envelope<CommissionEntry>>, (StatusCode, Json<Envelope<()>>)> {
    require_platform_admin(&headers, &state).await?;
    let mut connection = Connection::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "返利冲正失败"))?;
    let transaction = connection
        .transaction()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "返利冲正失败"))?;
    let original: CommissionEntry=transaction.query_row("SELECT entry_id,order_id,provider_id,product_type,received_amount_cents,rate_basis_points,commission_amount_cents,status,created_at FROM commission_entries WHERE entry_id = ?",[entry_id.as_str()],|r|Ok(CommissionEntry{entry_id:r.get(0)?,order_id:r.get(1)?,provider_id:r.get(2)?,product_type:r.get(3)?,received_amount_cents:r.get(4)?,rate_basis_points:r.get(5)?,commission_amount_cents:r.get(6)?,status:r.get(7)?,created_at:r.get(8)?})).optional().map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"返利台账读取失败"))?.ok_or_else(||error(StatusCode::NOT_FOUND,"返利记录不存在"))?;
    if original.status != "pending_settlement" {
        return Err(error(StatusCode::BAD_REQUEST, "仅待结算返利可冲正"));
    }
    let reversal = CommissionEntry {
        entry_id: generated_resource_id("cme"),
        order_id: original.order_id.clone(),
        provider_id: original.provider_id.clone(),
        product_type: format!("{}-退款冲正", original.product_type),
        received_amount_cents: -original.received_amount_cents,
        rate_basis_points: original.rate_basis_points,
        commission_amount_cents: -original.commission_amount_cents,
        status: "reversed".to_string(),
        created_at: Utc::now().timestamp(),
    };
    transaction.execute("INSERT INTO commission_entries (entry_id,order_id,provider_id,product_type,received_amount_cents,rate_basis_points,commission_amount_cents,status,created_at) VALUES (?,?,?,?,?,?,?,?,?)",params![reversal.entry_id,reversal.order_id,reversal.provider_id,reversal.product_type,reversal.received_amount_cents,reversal.rate_basis_points,reversal.commission_amount_cents,reversal.status,reversal.created_at]).map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"返利冲正失败"))?;
    transaction
        .execute(
            "UPDATE commission_entries SET status = 'reversed' WHERE entry_id = ?",
            [entry_id.as_str()],
        )
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "返利冲正失败"))?;
    transaction
        .commit()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "返利冲正失败"))?;
    Ok(Json(success("返利已冲正", reversal)))
}

async fn update_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(order_id): Path<String>,
    Json(request): Json<CreateOrderRequest>,
) -> Result<Json<Envelope<OrderRecord>>, (StatusCode, Json<Envelope<()>>)> {
    require_admin(&headers, &state).await?;
    if Connection::open(&state.database_path).map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"订单读取失败"))?
        .query_row("SELECT EXISTS(SELECT 1 FROM application_order_details WHERE order_id=?)",[&order_id],|r|r.get::<_,bool>(0)).map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"订单读取失败"))? {
        return Err(error(StatusCode::BAD_REQUEST,"应用订单请使用应用交易管理操作，不能通过通用订单接口修改"));
    }

    licensing::validate_items(&request.items)?;
    if !["saas", "local"].contains(&request.deployment_type.as_str()) || request.items.is_empty() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "订单信息不完整或平台类型无效",
        ));
    }
    if request.items.iter().any(|item| {
        item.title.trim().is_empty()
            || item.quantity == 0
            || item.unit_price_cents < 0
            || !["platform", "module", "ai_employee", "service"].contains(&item.kind.as_str())
    }) {
        return Err(error(StatusCode::BAD_REQUEST, "订单商品项不完整或类型无效"));
    }
    let total = request
        .items
        .iter()
        .try_fold(0_i64, |total, item| {
            total.checked_add(item.unit_price_cents.checked_mul(item.quantity.into())?)
        })
        .filter(|amount| *amount > 0)
        .ok_or_else(|| error(StatusCode::BAD_REQUEST, "订单金额必须大于 0"))?;
    let connection = Connection::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "订单保存失败"))?;
    let customer = get_customer(&connection, request.customer_id.trim())
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "客户数据读取失败"))?
        .ok_or_else(|| error(StatusCode::BAD_REQUEST, "客户不存在"))?;
    if customer.status != "active" {
        return Err(error(StatusCode::BAD_REQUEST, "停用客户不能关联订单"));
    }
    let status: String = connection
        .query_row(
            "SELECT status FROM orders WHERE order_id = ?",
            [&order_id],
            |row| row.get(0),
        )
        .map_err(|_| error(StatusCode::NOT_FOUND, "订单不存在"))?;
    if status != "pending_payment" {
        return Err(error(StatusCode::BAD_REQUEST, "只有未回款订单可以编辑"));
    }
    connection.execute("UPDATE orders SET customer_id = ?, items_json = ?, total_amount_cents = ?, due_at = ?, deployment_type = ?, notes = ? WHERE order_id = ?", params![customer.customer_id, serde_json::to_string(&request.items).expect("order items are serializable"), total, request.due_at.map(|value| value.timestamp()), request.deployment_type, request.notes.trim(), order_id]).map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "订单保存失败"))?;
    let order = get_order(&connection, &order_id)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "订单读取失败"))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "订单不存在"))?;
    Ok(Json(success("订单已更新", order)))
}

async fn record_payment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(order_id): Path<String>,
    Json(request): Json<RecordPaymentRequest>,
) -> Result<Json<Envelope<OrderRecord>>, (StatusCode, Json<Envelope<()>>)> {
    require_admin(&headers, &state).await?;
    if Connection::open(&state.database_path).map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"订单读取失败"))?
        .query_row("SELECT EXISTS(SELECT 1 FROM application_order_details WHERE order_id=?)",[&order_id],|r|r.get::<_,bool>(0)).map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"订单读取失败"))? {
        return Err(error(StatusCode::BAD_REQUEST,"应用订单请使用应用交易管理操作，不能通过通用订单接口修改"));
    }

    if request.amount_cents <= 0 || request.method.trim().is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "回款金额和收款方式不能为空"));
    }
    let mut connection = Connection::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "回款记录保存失败"))?;
    let transaction = connection
        .transaction()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "回款记录保存失败"))?;
    let (customer_id, total_amount_cents, status, provider_id, items_json): (String, i64, String, Option<String>, String) = transaction
        .query_row(
            "SELECT customer_id, total_amount_cents, status, provider_id, items_json FROM orders WHERE order_id = ?",
            [&order_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .optional()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "订单读取失败"))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "订单不存在"))?;
    if ["cancelled", "fulfilled"].contains(&status.as_str()) {
        return Err(error(StatusCode::BAD_REQUEST, "当前订单状态不能登记回款"));
    }
    let received_before: i64 = transaction
        .query_row(
            "SELECT COALESCE(SUM(amount_cents), 0) FROM transactions WHERE order_id = ?",
            [&order_id],
            |row| row.get(0),
        )
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "回款汇总失败"))?;
    if received_before + request.amount_cents > total_amount_cents {
        return Err(error(StatusCode::BAD_REQUEST, "本次回款超过订单待收金额"));
    }
    let now = Utc::now().timestamp();
    transaction
        .execute(
            "INSERT INTO transactions (
                transaction_id, order_id, customer_id, amount_cents, method,
                reference, notes, occurred_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                format!("txn_{}", Uuid::new_v4().simple()),
                order_id,
                customer_id,
                request.amount_cents,
                request.method.trim(),
                request.reference.trim(),
                request.notes.trim(),
                now,
            ],
        )
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "回款记录保存失败"))?;
    if let Some(provider_id) = provider_id {
        let items: Vec<OrderLineItem> = serde_json::from_str(&items_json).unwrap_or_default();
        for item in items {
            let product_type = match item.kind.as_str() {
                "platform" => "低代码平台",
                "ai_employee" => "AI员工",
                "module" => "通信",
                _ => "服务",
            };
            let rule: Option<(i64,)> = transaction.query_row("SELECT rate_basis_points FROM commission_rules WHERE product_type = ? AND status = 'active' AND effective_at <= ? ORDER BY effective_at DESC LIMIT 1", params![product_type, now], |row| Ok((row.get(0)?,))).optional().map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR,"提成规则读取失败"))?;
            if let Some((rate,)) = rule {
                let item_amount = item.unit_price_cents * i64::from(item.quantity);
                let received = item_amount * request.amount_cents / total_amount_cents;
                let commission = received * rate / 10_000;
                transaction.execute("INSERT INTO commission_entries (entry_id,order_id,provider_id,product_type,received_amount_cents,rate_basis_points,commission_amount_cents,status,created_at) VALUES (?,?,?,?,?,?,?,'pending_settlement',?)",params![generated_resource_id("cme"),order_id,provider_id,product_type,received,rate,commission,now]).map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"返利台账保存失败"))?;
            }
        }
    }
    if received_before + request.amount_cents == total_amount_cents {
        transaction
            .execute(
                "UPDATE orders SET status = 'paid', paid_at = ? WHERE order_id = ?",
                params![now, order_id],
            )
            .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "订单状态更新失败"))?;
    }
    transaction
        .commit()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "回款记录保存失败"))?;
    let order = get_order(&connection, &order_id)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "订单读取失败"))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "订单不存在"))?;
    Ok(Json(success("回款已登记", order)))
}

async fn cancel_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(order_id): Path<String>,
) -> Result<Json<Envelope<OrderRecord>>, (StatusCode, Json<Envelope<()>>)> {
    require_admin(&headers, &state).await?;
    if Connection::open(&state.database_path).map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"订单读取失败"))?
        .query_row("SELECT EXISTS(SELECT 1 FROM application_order_details WHERE order_id=?)",[&order_id],|r|r.get::<_,bool>(0)).map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"订单读取失败"))? {
        return Err(error(StatusCode::BAD_REQUEST,"应用订单请使用应用交易管理操作，不能通过通用订单接口修改"));
    }

    let connection = Connection::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "订单取消失败"))?;
    let updated = connection
        .execute(
            "UPDATE orders SET status = 'cancelled'
             WHERE order_id = ? AND status = 'pending_payment'
               AND NOT EXISTS (SELECT 1 FROM transactions WHERE order_id = ?)",
            params![order_id, order_id],
        )
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "订单取消失败"))?;
    if updated == 0 {
        return Err(error(StatusCode::BAD_REQUEST, "只有未回款订单可以取消"));
    }
    let order = get_order(&connection, &order_id)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "订单读取失败"))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "订单不存在"))?;
    Ok(Json(success("订单已取消", order)))
}

async fn finance_summary(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Envelope<FinanceSummary>>, (StatusCode, Json<Envelope<()>>)> {
    require_admin(&headers, &state).await?;
    let connection = Connection::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "财务汇总读取失败"))?;
    let month_start = Utc::now()
        .date_naive()
        .with_day(1)
        .and_then(|date| date.and_hms_opt(0, 0, 0))
        .map(|value| value.and_utc().timestamp())
        .unwrap_or(0);
    let total_received_cents = scalar_i64(
        &connection,
        "SELECT COALESCE(SUM(amount_cents), 0) FROM transactions",
        [],
    )?;
    let month_received_cents = connection
        .query_row(
            "SELECT COALESCE(SUM(amount_cents), 0) FROM transactions WHERE occurred_at >= ?",
            [month_start],
            |row| row.get(0),
        )
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "财务汇总读取失败"))?;
    let outstanding_cents = scalar_i64(
        &connection,
        "SELECT COALESCE(SUM(MAX(o.total_amount_cents - COALESCE(p.received, 0), 0)), 0)
         FROM orders o
         LEFT JOIN (SELECT order_id, SUM(amount_cents) received FROM transactions GROUP BY order_id) p
           ON p.order_id = o.order_id
         WHERE o.status NOT IN ('cancelled', 'fulfilled')",
        [],
    )?;
    let summary = FinanceSummary {
        total_received_cents,
        month_received_cents,
        outstanding_cents,
        paid_order_count: scalar_i64(
            &connection,
            "SELECT COUNT(*) FROM orders WHERE status IN ('paid', 'fulfilled')",
            [],
        )?,
        pending_order_count: scalar_i64(
            &connection,
            "SELECT COUNT(*) FROM orders WHERE status = 'pending_payment'",
            [],
        )?,
        active_customer_count: scalar_i64(
            &connection,
            "SELECT COUNT(*) FROM customers WHERE status = 'active'",
            [],
        )?,
    };
    Ok(Json(success("财务汇总已读取", summary)))
}

async fn list_transactions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Envelope<Vec<TransactionRecord>>>, (StatusCode, Json<Envelope<()>>)> {
    require_admin(&headers, &state).await?;
    let connection = Connection::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "回款流水读取失败"))?;
    let mut statement = connection
        .prepare(
            "SELECT t.transaction_id, t.order_id, o.order_no, t.customer_id, c.name,
                    t.amount_cents, t.method, t.reference, t.notes, t.occurred_at
             FROM transactions t
             JOIN orders o ON o.order_id = t.order_id
             JOIN customers c ON c.customer_id = t.customer_id
             ORDER BY t.occurred_at DESC",
        )
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "回款流水读取失败"))?;
    let records = statement
        .query_map([], |row| {
            Ok(TransactionRecord {
                transaction_id: row.get(0)?,
                order_id: row.get(1)?,
                order_no: row.get(2)?,
                customer_id: row.get(3)?,
                customer_name: row.get(4)?,
                amount_cents: row.get(5)?,
                method: row.get(6)?,
                reference: row.get(7)?,
                notes: row.get(8)?,
                occurred_at: row.get(9)?,
            })
        })
        .and_then(Iterator::collect)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "回款流水读取失败"))?;
    Ok(Json(success("回款流水已读取", records)))
}

async fn list_ai_employee_products(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Envelope<Vec<AiEmployeeProduct>>>, (StatusCode, Json<Envelope<()>>)> {
    require_admin(&headers, &state).await?;
    let connection = Connection::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "AI 员工目录读取失败"))?;
    let products = load_ai_employee_products(&connection)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "AI 员工目录读取失败"))?;
    Ok(Json(success("AI 员工目录已读取", products)))
}

async fn list_public_ai_employee_products(
    State(state): State<AppState>,
) -> Result<Json<Envelope<Vec<AiEmployeeMarketProduct>>>, (StatusCode, Json<Envelope<()>>)> {
    let connection = Connection::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "AI 员工市场读取失败"))?;
    let products = load_ai_employee_products(&connection)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "AI 员工市场读取失败"))?
        .into_iter()
        .filter(|product| product.status == "active")
        .map(|product| AiEmployeeMarketProduct {
            id: product.id.clone(),
            title: product.title,
            description: product.description,
            category: product.category,
            price_cents: product.price_cents,
            billing_cycle: product.billing_cycle,
            version: product.version,
            avatar_url: product
                .avatar_url
                .map(|_| format!("/api/ai-employees/{}/avatar", product.id)),
        })
        .collect();
    Ok(Json(success("AI 员工市场已读取", products)))
}

async fn get_owned_ai_employee_package(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(employee_id): Path<String>,
) -> Result<Json<Envelope<AiEmployeeEntitlement>>, (StatusCode, Json<Envelope<()>>)> {
    let token =
        bearer_token(&headers).ok_or_else(|| error(StatusCode::UNAUTHORIZED, "缺少平台许可证"))?;
    let claims = verify_license(token, &state)
        .map_err(|_| error(StatusCode::UNAUTHORIZED, "平台许可证无效或已过期"))?;
    let entitlement = claims
        .ai_employees
        .iter()
        .find(|item| item.id == employee_id && item.expires_at > Utc::now().timestamp())
        .ok_or_else(|| error(StatusCode::FORBIDDEN, "当前许可证未包含该 AI 员工"))?;
    let connection = Connection::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "AI 员工安装包读取失败"))?;
    let product = get_ai_employee_product(&connection, employee_id.trim())
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "AI 员工商品读取失败"))?
        .filter(|product| product.status == "active")
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "AI 员工商品不存在或未上架"))?;
    let mut skill_ids = product.skill_ids.clone();
    skill_ids.sort();
    skill_ids.dedup();
    let mut skills = Vec::new();
    for skill_id in skill_ids {
        if let Some(skill) = get_operation_skill(&connection, &skill_id)
            .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "AI 员工 Skill 读取失败"))?
        {
            skills.push(OperationSkillSnapshot::from(skill));
        }
    }
    let knowledge=Some(employee_knowledge::manifest(&connection, &product.id, &product.version).map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR,"员工知识读取失败"))?);
    let system_prompt = product.system_prompt.clone();
    Ok(Json(success(
        "AI 员工安装包已读取",
        AiEmployeeEntitlement {
            id: product.id,
            title: product.title,
            expires_at: entitlement.expires_at,
            template_version: product.version,
            skills,
            system_prompt,
            allowed_tools: product.allowed_tools,
            knowledge,
            application_ids: product.application_ids,
        },
    )))
}

async fn download_owned_ai_employee_skill_archive(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((employee_id, skill_id)): Path<(String, String)>,
) -> Result<Response, (StatusCode, Json<Envelope<()>>)> {
    let token =
        bearer_token(&headers).ok_or_else(|| error(StatusCode::UNAUTHORIZED, "缺少平台许可证"))?;
    let claims = verify_license(token, &state)
        .map_err(|_| error(StatusCode::UNAUTHORIZED, "平台许可证无效或已过期"))?;
    let entitlement = claims
        .ai_employees
        .iter()
        .find(|item| item.id == employee_id && item.expires_at > Utc::now().timestamp())
        .ok_or_else(|| error(StatusCode::FORBIDDEN, "当前许可证未包含该 AI 员工"))?;
    let connection = Connection::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "AI 员工 Skill 读取失败"))?;
    let product = get_ai_employee_product(&connection, employee_id.trim())
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "AI 员工商品读取失败"))?
        .filter(|product| product.status == "active")
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "AI 员工商品不存在或未上架"))?;
    let skill_ids = product.skill_ids.clone();
    if !skill_ids.iter().any(|id| id == &skill_id) {
        return Err(error(StatusCode::FORBIDDEN, "该 Skill 不属于当前 AI 员工"));
    }
    let skill = get_operation_skill(&connection, skill_id.trim())
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "AI 员工 Skill 读取失败"))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "AI 员工 Skill 不存在"))?;
    let archive = archive_operation_skill_package(&skill)
        .map_err(|message| error(StatusCode::BAD_REQUEST, message))?;
    let filename = format!("{}-{}.zip", entitlement.id, skill.package_name);
    let mut response = Response::new(Body::from(archive));
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("application/zip"));
    let disposition = format!("attachment; filename=\"{}\"", filename.replace('"', ""));
    response.headers_mut().insert(
        axum::http::header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&disposition)
            .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Skill 文件名无效"))?,
    );
    Ok(response)
}

async fn test_purchase_ai_employee(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(employee_id): Path<String>,
) -> Result<Json<Envelope<MarketPurchaseResponse>>, (StatusCode, Json<Envelope<()>>)> {
    let token =
        bearer_token(&headers).ok_or_else(|| error(StatusCode::UNAUTHORIZED, "缺少平台许可证"))?;
    let current_claims = verify_license(token, &state)
        .map_err(|_| error(StatusCode::UNAUTHORIZED, "平台许可证无效或已过期"))?;
    let mut connection = Connection::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "测试订单创建失败"))?;
    let current_record = get_license_record(&state.database_path, &current_claims.license_id)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "许可证状态读取失败"))?
        .filter(|record| {
            record.subject == current_claims.subject
                && record.linkage_status != "legacy_unlinked"
                && record.platform_status != "destroyed"
        })
        .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "平台许可证已失效"))?;
    let customer = get_customer(&connection, &current_claims.subject)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "客户数据读取失败"))?
        .filter(|customer| customer.status == "active")
        .ok_or_else(|| error(StatusCode::FORBIDDEN, "许可证主体对应的客户不存在或已停用"))?;
    let product = get_ai_employee_product(&connection, employee_id.trim())
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "AI 员工商品读取失败"))?
        .filter(|product| product.status == "active")
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "AI 员工商品不存在或未上架"))?;
    if product.price_cents <= 0 {
        return Err(error(StatusCode::BAD_REQUEST, "测试购买要求商品价格大于 0"));
    }

    let now = Utc::now().timestamp();
    let latest_record = licensing::latest(&connection,&current_claims.subject).map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"客户授权读取失败"))?.unwrap_or(current_record);
    let latest_claims = verify_license(&latest_record.license, &state)
        .map_err(|_| error(StatusCode::CONFLICT, "客户最新许可证无效，无法生成累计权益"))?;
    let purchase_days = match product.billing_cycle.as_str() {
        "month" => 30,
        "year" => 365,
        "one_time" => 3650,
        _ => return Err(error(StatusCode::BAD_REQUEST, "AI 员工计费周期无效")),
    };
    let items = vec![OrderLineItem {
        kind:"ai_employee".into(),reference_id:Some(product.id.clone()),title:product.title.clone(),quantity:1,
        unit_price_cents:product.price_cents,entitlement_days:Some(purchase_days),
    }];

    let order_id = format!("ord_{}", Uuid::new_v4().simple());
    let order_no = format!(
        "YY{}-{}",
        Utc::now().format("%Y%m%d"),
        &Uuid::new_v4().simple().to_string()[..6].to_uppercase()
    );
    let transaction_id = format!("txn_{}", Uuid::new_v4().simple());
    let payment_reference = format!("TEST-{}", Uuid::new_v4().simple());
    {
        let transaction = connection
            .transaction()
            .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "测试订单创建失败"))?;
        transaction
            .execute(
                "INSERT INTO orders (
                order_id, order_no, customer_id, items_json, total_amount_cents,
                status, created_at, due_at, deployment_type, paid_at, notes
             ) VALUES (?, ?, ?, ?, ?, 'paid', ?, ?, ?, ?, ?)",
                params![
                    order_id,
                    order_no,
                    customer.customer_id,
                    serde_json::to_string(&items).expect("order items are serializable"),
                    product.price_cents,
                    now,
                    now,
                    latest_claims.deployment_type,
                    now,
                    format!("AI 员工市场测试支付：{}", product.title),
                ],
            )
            .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "测试订单创建失败"))?;
        transaction
            .execute(
                "INSERT INTO transactions (
                transaction_id, order_id, customer_id, amount_cents, method,
                reference, notes, occurred_at
             ) VALUES (?, ?, ?, ?, 'test', ?, 'AI 员工市场模拟支付', ?)",
                params![
                    transaction_id,
                    order_id,
                    customer.customer_id,
                    product.price_cents,
                    payment_reference,
                    now,
                ],
            )
            .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "测试回款登记失败"))?;
        transaction
            .commit()
            .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "测试订单创建失败"))?;
    }
    drop(connection);

    let internal_session_id = Uuid::new_v4().simple().to_string();
    state.sessions.write().await.insert(
        internal_session_id.clone(),
        AuthenticatedUser {
            user_id: "system-test-purchase".to_string(),
            role: "platform_admin".to_string(),
            organization_id: "platform".to_string(),
        },
    );
    let mut admin_headers = HeaderMap::new();
    admin_headers.insert(
        "cookie",
        HeaderValue::from_str(&format!("license_center_session={internal_session_id}"))
            .expect("session cookie is valid"),
    );
    if let Some(origin)=headers.get("origin") {admin_headers.insert("origin",origin.clone());}
    let Json(issued) = issue_license(
        State(state.clone()),
        admin_headers,
        Json(IssueLicenseRequest {
            order_id: order_id.clone(),
        }),
    )
    .await?;
    let license = issued
        .data
        .ok_or_else(|| error(StatusCode::INTERNAL_SERVER_ERROR, "测试许可证签发失败"))?;
    state.sessions.write().await.remove(&internal_session_id);
    Ok(Json(success(
        "测试支付、回款和许可证签发已完成",
        MarketPurchaseResponse {
            order_id,
            order_no,
            license_id: license.license_id,
            status: "fulfilled".to_string(),
        },
    )))
}

async fn create_ai_employee_product(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateAiEmployeeProductRequest>,
) -> Result<Json<Envelope<AiEmployeeProduct>>, (StatusCode, Json<Envelope<()>>)> {
    require_admin(&headers, &state).await?;
    let id = if request.id.trim().is_empty() {
        generated_resource_id("ai-employee")
    } else {
        request.id.trim().to_string()
    };
    let title = request.title.trim();
    if title.is_empty() || request.price_cents < 0 {
        return Err(error(StatusCode::BAD_REQUEST, "AI 员工 title 或价格无效"));
    }
    if !["month", "year", "one_time"].contains(&request.billing_cycle.as_str()) {
        return Err(error(StatusCode::BAD_REQUEST, "计费周期无效"));
    }
    let now = Utc::now().timestamp();
    let connection = Connection::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "AI 员工保存失败"))?;
    let existing_product = get_ai_employee_product(&connection, &id)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "AI 员工读取失败"))?;
    let effective_version = existing_product
        .as_ref()
        .map(|product| next_patch_version(&product.version))
        .unwrap_or_else(default_template_version);
    let mut skill_ids = request.skill_ids;
    skill_ids.sort();
    skill_ids.dedup();
    validate_active_skills(&connection, &skill_ids)?;
    connection
        .execute(
            "INSERT INTO ai_employee_products (
                id, title, description, category, price_cents, billing_cycle,
                version, skill_ids_json, system_prompt, allow_network,
                allowed_tools_json, application_ids_json, status, created_at, updated_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'active', ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                description = excluded.description,
                category = excluded.category,
                price_cents = excluded.price_cents,
                billing_cycle = excluded.billing_cycle,
                version = excluded.version,
                skill_ids_json = excluded.skill_ids_json,
                system_prompt = excluded.system_prompt,
                allow_network = excluded.allow_network,
                allowed_tools_json = excluded.allowed_tools_json,
                application_ids_json = excluded.application_ids_json,
                updated_at = excluded.updated_at",
            params![
                &id,
                title,
                request.description.trim(),
                if request.category.trim().is_empty() {
                    "通用"
                } else {
                    request.category.trim()
                },
                request.price_cents,
                request.billing_cycle,
                effective_version,
                serde_json::to_string(&skill_ids).unwrap_or_else(|_| "[]".to_string()),
                request.system_prompt.trim(),
                request.allow_network,
                serde_json::to_string(&request.allowed_tools).unwrap_or_else(|_| "[]".to_string()),
                serde_json::to_string(&request.application_ids)
                    .unwrap_or_else(|_| "[]".to_string()),
                now,
                now,
            ],
        )
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "AI 员工保存失败"))?;
    let product = get_ai_employee_product(&connection, &id)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "AI 员工读取失败"))?
        .ok_or_else(|| error(StatusCode::INTERNAL_SERVER_ERROR, "AI 员工保存失败"))?;
    Ok(Json(success("AI 员工商品已保存", product)))
}

fn ai_employee_avatar_dir(database_path: &PathBuf) -> PathBuf {
    database_path
        .parent()
        .unwrap_or_else(|| FilePath::new("."))
        .join("avatars")
        .join("ai-employees")
}

async fn get_ai_employee_avatar(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(employee_id): Path<String>,
) -> Result<Response, (StatusCode, Json<Envelope<()>>)> {
    let _ = headers;
    let connection = Connection::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "头像读取失败"))?;
    let product = get_ai_employee_product(&connection, &employee_id)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "头像读取失败"))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "AI 员工不存在"))?;
    let Some(url) = product.avatar_url else {
        return Err(error(StatusCode::NOT_FOUND, "头像不存在"));
    };
    let bytes = tokio::fs::read(ai_employee_avatar_dir(&state.database_path).join(url))
        .await
        .map_err(|_| error(StatusCode::NOT_FOUND, "头像不存在"))?;
    Ok(Response::builder()
        .header("content-type", "image/webp")
        .body(Body::from(bytes))
        .unwrap())
}

async fn upload_ai_employee_avatar(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(employee_id): Path<String>,
    body: Bytes,
) -> Result<Json<Envelope<AiEmployeeProduct>>, (StatusCode, Json<Envelope<()>>)> {
    require_admin(&headers, &state).await?;
    if body.is_empty() || body.len() > 5 * 1024 * 1024 {
        return Err(error(StatusCode::BAD_REQUEST, "头像文件需小于 5MB"));
    }
    let connection = Connection::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "头像保存失败"))?;
    let product = get_ai_employee_product(&connection, &employee_id)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "头像读取失败"))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "AI 员工不存在"))?;
    let dir = ai_employee_avatar_dir(&state.database_path);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "头像保存失败"))?;
    let file_name = format!("{}-{}.webp", employee_id, Uuid::new_v4());
    tokio::fs::write(dir.join(&file_name), &body)
        .await
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "头像保存失败"))?;
    if let Some(previous) = product.avatar_url {
        let _ = tokio::fs::remove_file(dir.join(previous)).await;
    }
    connection
        .execute(
            "UPDATE ai_employee_products SET avatar_url = ?, updated_at = ? WHERE id = ?",
            params![file_name, Utc::now().timestamp(), employee_id],
        )
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "头像保存失败"))?;
    let updated = get_ai_employee_product(&connection, &employee_id)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "头像读取失败"))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "AI 员工不存在"))?;
    Ok(Json(success("头像已保存", updated)))
}

async fn update_ai_employee_product_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(employee_id): Path<String>,
    Json(request): Json<StatusRequest>,
) -> Result<Json<Envelope<AiEmployeeProduct>>, (StatusCode, Json<Envelope<()>>)> {
    require_admin(&headers, &state).await?;
    if !["active", "inactive"].contains(&request.status.as_str()) {
        return Err(error(StatusCode::BAD_REQUEST, "AI 员工状态无效"));
    }
    let connection = Connection::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "AI 员工状态更新失败"))?;
    let existing = get_ai_employee_product(&connection, &employee_id)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "AI 员工读取失败"))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "AI 员工不存在"))?;
    let now = Utc::now().timestamp();
    let updated = connection
        .execute(
            "UPDATE ai_employee_products SET status = ?, version = ?, updated_at = ? WHERE id = ?",
            params![
                request.status,
                next_patch_version(&existing.version),
                now,
                employee_id
            ],
        )
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "AI 员工状态更新失败"))?;
    if updated == 0 {
        return Err(error(StatusCode::NOT_FOUND, "AI 员工不存在"));
    }
    let product = get_ai_employee_product(&connection, &employee_id)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "AI 员工读取失败"))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "AI 员工不存在"))?;
    Ok(Json(success("AI 员工状态已更新", product)))
}

fn ai_employee_deletion_eligibility(
    database_path: &PathBuf,
    employee_id: &str,
) -> rusqlite::Result<AiEmployeeDeletionEligibility> {
    Ok(ai_employee_deletion_eligibility_from_licenses(
        list_license_records(database_path)?,
        employee_id,
        Utc::now().timestamp(),
    ))
}

fn ai_employee_deletion_eligibility_from_licenses(
    licenses: impl IntoIterator<Item = LicenseRecord>,
    employee_id: &str,
    now: i64,
) -> AiEmployeeDeletionEligibility {
    let mut blockers = licenses
        .into_iter()
        .filter(|license| license.platform_status != "destroyed" && license.expires_at > now)
        .filter_map(|license| {
            license
                .ai_employees
                .into_iter()
                .find(|employee| employee.id == employee_id && employee.expires_at > now)
                .map(|employee| AiEmployeeDeletionBlocker {
                    license_id: license.license_id,
                    customer_name: if license.customer_name_snapshot.trim().is_empty() {
                        license.subject
                    } else {
                        license.customer_name_snapshot
                    },
                    expires_at: employee.expires_at,
                })
        })
        .collect::<Vec<_>>();
    blockers.sort_by(|left, right| left.expires_at.cmp(&right.expires_at));
    AiEmployeeDeletionEligibility {
        can_delete: blockers.is_empty(),
        blockers,
    }
}

async fn get_ai_employee_deletion_eligibility(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(employee_id): Path<String>,
) -> Result<Json<Envelope<AiEmployeeDeletionEligibility>>, (StatusCode, Json<Envelope<()>>)> {
    require_admin(&headers, &state).await?;
    let connection = Connection::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "AI 员工删除检查失败"))?;
    if get_ai_employee_product(&connection, &employee_id)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "AI 员工读取失败"))?
        .is_none()
    {
        return Err(error(StatusCode::NOT_FOUND, "AI 员工不存在"));
    }
    let eligibility = ai_employee_deletion_eligibility(&state.database_path, &employee_id)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "AI 员工删除检查失败"))?;
    Ok(Json(success("AI 员工删除条件已检查", eligibility)))
}

async fn delete_ai_employee_product(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(employee_id): Path<String>,
) -> Result<Json<Envelope<AiEmployeeDeletionEligibility>>, (StatusCode, Json<Envelope<()>>)> {
    require_admin(&headers, &state).await?;
    let eligibility = ai_employee_deletion_eligibility(&state.database_path, &employee_id)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "AI 员工删除检查失败"))?;
    if !eligibility.can_delete {
        return Err(error(
            StatusCode::CONFLICT,
            "该 AI 员工仍存在有效客户授权，暂不能删除",
        ));
    }
    let connection = Connection::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "AI 员工删除失败"))?;
    let product = get_ai_employee_product(&connection, &employee_id)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "AI 员工读取失败"))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "AI 员工不存在"))?;
    connection
        .execute(
            "DELETE FROM ai_employee_products WHERE id = ?",
            [&employee_id],
        )
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "AI 员工删除失败"))?;
    if let Some(file_name) = product.avatar_url {
        let path = FilePath::new(&file_name);
        if path.file_name().and_then(|name| name.to_str()) == Some(file_name.as_str()) {
            let _ = tokio::fs::remove_file(ai_employee_avatar_dir(&state.database_path).join(path))
                .await;
        }
    }
    Ok(Json(success("AI 员工已删除", eligibility)))
}

async fn list_operation_skills(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Envelope<Vec<OperationSkill>>>, (StatusCode, Json<Envelope<()>>)> {
    require_admin(&headers, &state).await?;
    let connection = Connection::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Skills 读取失败"))?;
    let skills = load_operation_skills(&connection)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Skills 读取失败"))?;
    Ok(Json(success("Skills 已读取", skills)))
}

fn operation_skill_deletion_eligibility(
    connection: &Connection,
    skill_id: &str,
) -> rusqlite::Result<SkillDeletionEligibility> {
    let mut blockers = load_ai_employee_products(connection)?
        .into_iter()
        .filter(|employee| employee.skill_ids.iter().any(|id| id == skill_id))
        .map(|employee| SkillDeletionBlocker {
            employee_id: employee.id,
            employee_title: employee.title,
            employee_status: employee.status,
        })
        .collect::<Vec<_>>();
    blockers.sort_by(|left, right| left.employee_title.cmp(&right.employee_title));
    Ok(SkillDeletionEligibility {
        can_delete: blockers.is_empty(),
        blockers,
    })
}

async fn get_operation_skill_deletion_eligibility(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(skill_id): Path<String>,
) -> Result<Json<Envelope<SkillDeletionEligibility>>, (StatusCode, Json<Envelope<()>>)> {
    require_admin(&headers, &state).await?;
    let connection = Connection::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Skill 删除检查失败"))?;
    if get_operation_skill(&connection, skill_id.trim())
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Skill 读取失败"))?
        .is_none()
    {
        return Err(error(StatusCode::NOT_FOUND, "Skill 不存在"));
    }
    let eligibility = operation_skill_deletion_eligibility(&connection, skill_id.trim())
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Skill 删除检查失败"))?;
    Ok(Json(success("Skill 删除条件已检查", eligibility)))
}

async fn delete_operation_skill(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(skill_id): Path<String>,
) -> Result<Json<Envelope<SkillDeletionEligibility>>, (StatusCode, Json<Envelope<()>>)> {
    require_admin(&headers, &state).await?;
    let connection = Connection::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Skill 删除失败"))?;
    let skill = get_operation_skill(&connection, skill_id.trim())
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Skill 读取失败"))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "Skill 不存在"))?;
    let eligibility = operation_skill_deletion_eligibility(&connection, skill_id.trim())
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Skill 删除检查失败"))?;
    if !eligibility.can_delete {
        return Err(error(
            StatusCode::CONFLICT,
            "该 Skill 仍被 AI 员工引用，暂不能删除",
        ));
    }
    connection
        .execute(
            "DELETE FROM operation_skills WHERE id = ?",
            [skill_id.trim()],
        )
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Skill 删除失败"))?;
    let package_name = FilePath::new(&skill.package_name);
    if package_name.components().count() == 1
        && matches!(package_name.components().next(), Some(Component::Normal(_)))
    {
        let _ = fs::remove_dir_all(FilePath::new("runtime/skills").join(package_name));
    }
    Ok(Json(success("Skill 已删除", eligibility)))
}

async fn list_platform_tools(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Envelope<Vec<PlatformTool>>>, (StatusCode, Json<Envelope<()>>)> {
    require_admin(&headers, &state).await?;
    Ok(Json(success("平台工具已读取", platform_tool_catalog())))
}

async fn import_operation_skill(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<Envelope<OperationSkill>>, (StatusCode, Json<Envelope<()>>)> {
    require_admin(&headers, &state).await?;
    let field = multipart
        .next_field()
        .await
        .map_err(|_| error(StatusCode::BAD_REQUEST, "Skill ZIP 上传数据无效"))?
        .ok_or_else(|| error(StatusCode::BAD_REQUEST, "请选择一个 Skill ZIP 文件"))?;
    let file_name = field.file_name().unwrap_or("skill.zip").to_string();
    if !file_name.to_ascii_lowercase().ends_with(".zip") {
        return Err(error(StatusCode::BAD_REQUEST, "Skill 导入仅支持 .zip 文件"));
    }
    let bytes = field
        .bytes()
        .await
        .map_err(|_| error(StatusCode::BAD_REQUEST, "无法读取 Skill ZIP 文件"))?;
    let id = format!("skill-{}", Uuid::new_v4().simple());
    let base_name = file_name
        .strip_suffix(".zip")
        .or_else(|| file_name.strip_suffix(".ZIP"))
        .unwrap_or("skill");
    let short_id = id
        .trim_start_matches("skill-")
        .chars()
        .take(8)
        .collect::<String>();
    let package_name = format!("{}-{short_id}", normalize_package_name(base_name));
    let (instructions, package_path) = extract_skill_package(&package_name, &bytes)
        .map_err(|message| error(StatusCode::BAD_REQUEST, message))?;
    let now = Utc::now().timestamp();
    let connection = Connection::open(&state.database_path).map_err(|_| {
        let _ = fs::remove_dir_all(FilePath::new("runtime/skills").join(&package_name));
        error(StatusCode::INTERNAL_SERVER_ERROR, "Skill 导入失败")
    })?;
    connection
        .execute(
            "INSERT INTO operation_skills (
                id, title, package_name, source, version, package_path,
                description, instructions, allowed_tools_json,
                application_ids_json, status, created_at, updated_at
             ) VALUES (?, ?, ?, 'imported', '1.0.0', ?, '', ?, '[]', '[]', 'active', ?, ?)",
            params![
                id,
                base_name.trim(),
                &package_name,
                package_path,
                instructions,
                now,
                now
            ],
        )
        .map_err(|_| {
            let _ = fs::remove_dir_all(FilePath::new("runtime/skills").join(&package_name));
            error(StatusCode::INTERNAL_SERVER_ERROR, "Skill 导入失败")
        })?;
    let skill = get_operation_skill(&connection, &id)
        .map_err(|_| {
            let _ = fs::remove_dir_all(FilePath::new("runtime/skills").join(&package_name));
            error(StatusCode::INTERNAL_SERVER_ERROR, "Skill 读取失败")
        })?
        .ok_or_else(|| error(StatusCode::INTERNAL_SERVER_ERROR, "Skill 导入失败"))?;
    Ok(Json(success("Skill ZIP 已导入", skill)))
}

async fn save_operation_skill(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<SaveOperationSkillRequest>,
) -> Result<Json<Envelope<OperationSkill>>, (StatusCode, Json<Envelope<()>>)> {
    require_admin(&headers, &state).await?;
    let id = request.id.trim().to_string();
    if id.is_empty() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "Skill 必须通过 ZIP 导入创建",
        ));
    }
    let title = request.title.trim();
    if title.is_empty() || request.instructions.trim().is_empty() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "Skill 名称和执行说明不能为空",
        ));
    }
    let now = Utc::now().timestamp();
    let connection = Connection::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Skill 保存失败"))?;
    let existing_skill = get_operation_skill(&connection, &id)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Skill 读取失败"))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "Skill 不存在"))?;
    let definitions =
        skill_environment::discover(&existing_skill.package_path, &request.instructions);
    let environment_values = request
        .environment_values
        .unwrap_or_else(|| existing_skill.environment_values.clone());
    skill_environment::validate_values(&definitions, &environment_values)
        .map_err(|message| error(StatusCode::BAD_REQUEST, message))?;
    let environment_json = serde_json::to_string(&environment_values)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "环境变量保存失败"))?;
    let version = next_patch_version(&existing_skill.version);
    connection
        .execute(
            "INSERT INTO operation_skills (
            id, title, package_name, source, version, package_path,
            description, instructions, allowed_tools_json, application_ids_json,
            status, created_at, updated_at, environment_values_json
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'active', ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
            title = excluded.title,
            package_name = excluded.package_name,
            source = excluded.source,
            version = excluded.version,
            package_path = excluded.package_path,
            description = excluded.description,
            instructions = excluded.instructions,
            allowed_tools_json = excluded.allowed_tools_json,
            application_ids_json = excluded.application_ids_json,
            environment_values_json = excluded.environment_values_json,
            updated_at = excluded.updated_at",
            params![
                &id,
                title,
                existing_skill.package_name,
                if request.source.trim().is_empty() {
                    "operation-center"
                } else {
                    request.source.trim()
                },
                version,
                existing_skill.package_path,
                request.description.trim(),
                request.instructions.trim(),
                "[]",
                "[]",
                now,
                now,
                environment_json,
            ],
        )
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Skill 保存失败"))?;
    bump_employee_versions_for_skill(&connection, &id, now)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "AI 员工版本更新失败"))?;
    let skill = get_operation_skill(&connection, &id)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Skill 读取失败"))?
        .ok_or_else(|| error(StatusCode::INTERNAL_SERVER_ERROR, "Skill 保存失败"))?;
    Ok(Json(success("Skill 已保存", skill)))
}

async fn update_operation_skill_archive(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(skill_id): Path<String>,
    mut multipart: Multipart,
) -> Result<Json<Envelope<OperationSkill>>, (StatusCode, Json<Envelope<()>>)> {
    require_admin(&headers, &state).await?;
    let field = multipart
        .next_field()
        .await
        .map_err(|_| error(StatusCode::BAD_REQUEST, "Skill ZIP 上传数据无效"))?
        .ok_or_else(|| error(StatusCode::BAD_REQUEST, "请选择一个 Skill ZIP 文件"))?;
    let file_name = field.file_name().unwrap_or("skill.zip").to_string();
    if !file_name.to_ascii_lowercase().ends_with(".zip") {
        return Err(error(StatusCode::BAD_REQUEST, "Skill 更新仅支持 .zip 文件"));
    }
    let bytes = field
        .bytes()
        .await
        .map_err(|_| error(StatusCode::BAD_REQUEST, "无法读取 Skill ZIP 文件"))?;
    let connection = Connection::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Skill 更新失败"))?;
    let existing_skill = get_operation_skill(&connection, skill_id.trim())
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Skill 读取失败"))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "Skill 不存在"))?;
    let (instructions, package_path) = extract_skill_package(&existing_skill.package_name, &bytes)
        .map_err(|message| error(StatusCode::BAD_REQUEST, message))?;
    let now = Utc::now().timestamp();
    let version = next_patch_version(&existing_skill.version);
    connection
        .execute(
            "UPDATE operation_skills
             SET version = ?, package_path = ?, instructions = ?, updated_at = ?
             WHERE id = ?",
            params![version, package_path, instructions, now, skill_id.trim()],
        )
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Skill 更新失败"))?;
    bump_employee_versions_for_skill(&connection, skill_id.trim(), now)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "AI 员工版本更新失败"))?;
    let skill = get_operation_skill(&connection, skill_id.trim())
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Skill 读取失败"))?
        .ok_or_else(|| error(StatusCode::INTERNAL_SERVER_ERROR, "Skill 更新失败"))?;
    Ok(Json(success("Skill ZIP 已更新", skill)))
}

async fn update_operation_skill_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(skill_id): Path<String>,
    Json(request): Json<StatusRequest>,
) -> Result<Json<Envelope<OperationSkill>>, (StatusCode, Json<Envelope<()>>)> {
    require_admin(&headers, &state).await?;
    if !["active", "inactive"].contains(&request.status.as_str()) {
        return Err(error(StatusCode::BAD_REQUEST, "Skill 状态无效"));
    }
    let connection = Connection::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Skill 状态更新失败"))?;
    let existing = get_operation_skill(&connection, &skill_id)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Skill 读取失败"))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "Skill 不存在"))?;
    let updated = connection
        .execute(
            "UPDATE operation_skills SET status = ?, version = ?, updated_at = ? WHERE id = ?",
            params![
                request.status,
                next_patch_version(&existing.version),
                Utc::now().timestamp(),
                skill_id
            ],
        )
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Skill 状态更新失败"))?;
    if updated == 0 {
        return Err(error(StatusCode::NOT_FOUND, "Skill 不存在"));
    }
    bump_employee_versions_for_skill(&connection, &skill_id, Utc::now().timestamp())
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "AI 员工版本更新失败"))?;
    let skill = get_operation_skill(&connection, &skill_id)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Skill 读取失败"))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "Skill 不存在"))?;
    Ok(Json(success("Skill 状态已更新", skill)))
}

async fn revoke_license(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(license_id): Path<String>,
) -> Result<Json<Envelope<LicenseStatus>>, (StatusCode, Json<Envelope<()>>)> {
    require_platform_admin(&headers, &state).await?;
    platform_versions::check_origin(&headers)?;
    destroy_license_by_id(&state, &license_id).await?;
    Ok(Json(success("许可证已吊销", LicenseStatus::simple(false))))
}

async fn destroy_license(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(license_id): Path<String>,
) -> Result<Json<Envelope<LicenseStatus>>, (StatusCode, Json<Envelope<()>>)> {
    require_platform_admin(&headers, &state).await?;
    platform_versions::check_origin(&headers)?;
    destroy_license_by_id(&state, &license_id).await?;
    Ok(Json(success("许可证已销毁", LicenseStatus::simple(false))))
}

async fn activate_license(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(license_id): Path<String>,
) -> Result<Json<Envelope<LicenseStatus>>, (StatusCode, Json<Envelope<()>>)> {
    let claims = bearer_token(&headers)
        .and_then(|token| verify_license(token, &state).ok())
        .filter(|claims| claims.license_id == license_id)
        .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "许可证无效或已过期"))?;
    let mut license = get_license_record(&state.database_path, &license_id)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "许可证状态读取失败"))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "许可证不存在"))?;
    if license.linkage_status == "legacy_unlinked" {
        return Err(error(
            StatusCode::FORBIDDEN,
            "历史无订单许可证仅支持只读查看",
        ));
    }
    if license.platform_status == "destroyed" {
        return Err(error(StatusCode::FORBIDDEN, "许可证已销毁"));
    }
    license.platform_status = "running".to_string();
    for module in &claims.modules {
        license.module_statuses.insert(
            module.clone(),
            if module_expiry(&license, module) > Utc::now().timestamp() {
                "running"
            } else {
                "expired"
            }
            .to_string(),
        );
    }
    if !activate_latest_license_record(&state.database_path, &license)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "许可证状态保存失败"))?
    {
        return Err(error(
            StatusCode::CONFLICT,
            "当前签名不是该授权主体的最新许可证，请更新后再激活",
        ));
    }
    Ok(Json(success("许可证已激活", LicenseStatus::simple(true))))
}

async fn license_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(license_id): Path<String>,
) -> Json<Envelope<LicenseStatus>> {
    let claims = bearer_token(&headers)
        .and_then(|token| verify_license(token, &state).ok())
        .filter(|claims| claims.license_id == license_id);
    let token_valid = claims.is_some();
    let valid = token_valid
        && get_license_record(&state.database_path, &license_id)
            .ok()
            .flatten()
            .is_some_and(|license| license.platform_status != "destroyed");
    let latest = if valid {claims.as_ref().and_then(|claims|ops::db::open(&state.database_path).ok().and_then(|db|licensing::latest(&db,&claims.subject).ok().flatten()).filter(|l|l.expires_at>Utc::now().timestamp()))} else {None};
    let mut status = LicenseStatus::simple(valid);
    if let Some(latest) = latest.filter(|license| license.license_id != license_id) {
        status.latest_license = Some(latest.license);
        status.latest_license_id = Some(latest.license_id);
        status.latest_issued_at = Some(latest.issued_at);
    }
    Json(success("许可证状态已读取", status))
}

async fn destroy_license_by_id(
    state: &AppState,
    license_id: &str,
) -> Result<(), (StatusCode, Json<Envelope<()>>)> {
    let mut license = get_license_record(&state.database_path, license_id)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "许可证状态读取失败"))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "许可证不存在"))?;
    if license.linkage_status == "legacy_unlinked" {
        return Err(error(
            StatusCode::FORBIDDEN,
            "历史无订单许可证仅支持只读查看",
        ));
    }
    license.platform_status = "destroyed".to_string();
    for module in &license.modules {
        license
            .module_statuses
            .insert(module.clone(), "destroyed".to_string());
    }
    let mut db=ops::db::open(&state.database_path).map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"授权撤销失败"))?;
    let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(|_|error(StatusCode::CONFLICT,"授权正在更新"))?;
    let auth:Option<String>=tx.query_row("SELECT authorization_id FROM customer_authorizations WHERE customer_id=? AND (latest_license_id=? OR active_license_id=?)",params![license.subject,license_id,license_id],|r|r.get(0)).optional().map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"授权读取失败"))?;
    if let Some(auth)=auth {
        tx.execute("UPDATE licenses SET platform_status='destroyed' WHERE license_id IN (SELECT license_id FROM authorization_versions WHERE authorization_id=?)",[&auth]).map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"授权撤销失败"))?;
        tx.execute("UPDATE authorization_versions SET state='revoked' WHERE authorization_id=?",[&auth]).map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"授权撤销失败"))?;
        tx.execute("UPDATE customer_authorizations SET active_license_id=NULL WHERE authorization_id=?",[auth]).map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"授权撤销失败"))?;
    } else {
        tx.execute("UPDATE licenses SET platform_status='destroyed' WHERE license_id=?",[license_id]).map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"许可证撤销失败"))?;
        tx.execute("UPDATE authorization_versions SET state='revoked' WHERE license_id=?",[license_id]).map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"许可证撤销失败"))?;
    }
    tx.commit().map_err(|_|error(StatusCode::INTERNAL_SERVER_ERROR,"授权撤销失败"))?;
    Ok(())
}

fn license_view(license: &LicenseRecord) -> LicenseView {
    let now = Utc::now().timestamp();
    let platform_status = if license.platform_status == "destroyed" {
        "destroyed".to_string()
    } else if license.expires_at <= now {
        "expired".to_string()
    } else {
        license.platform_status.clone()
    };
    let module_statuses = license
        .modules
        .iter()
        .map(|module| {
            let status = if module == "platform" {
                platform_status.clone()
            } else if platform_status == "destroyed" {
                "destroyed".to_string()
            } else if platform_status == "expired" || module_expiry(license, module) <= now {
                "expired".to_string()
            } else {
                license
                    .module_statuses
                    .get(module)
                    .cloned()
                    .unwrap_or_else(|| "unactivated".to_string())
            };
            (module.clone(), status)
        })
        .collect();
    let ai_employee_statuses = license
        .ai_employees
        .iter()
        .map(|entitlement| {
            let status = if platform_status == "destroyed" {
                "destroyed"
            } else if platform_status == "expired" || entitlement.expires_at <= now {
                "expired"
            } else if platform_status == "running" {
                "running"
            } else {
                "unactivated"
            };
            (entitlement.id.clone(), status.to_string())
        })
        .collect();
    LicenseView {
        license: license.license.clone(),
        license_id: license.license_id.clone(),
        subject: license.subject.clone(),
        customer_name_snapshot: license.customer_name_snapshot.clone(),
        order_id: license.order_id.clone(),
        linkage_status: license.linkage_status.clone(),
        modules: license.modules.clone(),
        issued_at: license.issued_at,
        expires_at: license.expires_at,
        module_expires_at: license.module_expires_at.clone(),
        module_titles: {
            let payload=license.license.split('.').nth(1).and_then(|v|base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD,v).ok()).and_then(|v|serde_json::from_slice::<serde_json::Value>(&v).ok());
            license.modules.iter().map(|module|(module.clone(),payload.as_ref().and_then(|v|v["moduleTitles"][module].as_str()).map(str::to_owned).unwrap_or_else(||module_title(module)))).collect()
        },
        ai_employees: license.ai_employees.clone(),
        ai_employee_statuses,
        platform_status,
        module_statuses,
    }
}

fn module_expiry(license: &LicenseRecord, module: &str) -> i64 {
    license
        .module_expires_at
        .get(module)
        .copied()
        .unwrap_or(license.expires_at).min(license.expires_at)
}

fn module_title(module: &str) -> String {
    match module {
        "platform" => "低代码平台".to_string(),
        "communication" => "通讯模型".to_string(),
        _ => module.to_string(),
    }
}

fn default_unactivated_status() -> String {
    "unactivated".to_string()
}

fn default_linked_status() -> String {
    "linked".to_string()
}

fn verify_license(token: &str, state: &AppState) -> Result<LicenseClaims, ()> {
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&["yaya-operation-center", "yaya-license-center"]);
    validation.set_audience(&["yaya-low-code"]);
    let claims=decode::<LicenseClaims>(token,&state.verification_key,&validation).map_err(|_|())?.claims;
    let db=ops::db::open(&state.database_path).map_err(|_|())?;
    let version:Option<String>=db.query_row("SELECT state FROM authorization_versions WHERE license_id=?",[&claims.license_id],|r|r.get(0)).optional().map_err(|_|())?;
    if version.as_deref().is_some_and(|s|["superseded","revoked"].contains(&s)) {return Err(());}
    Ok(claims)
}

async fn session_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Envelope<SessionStatus>>, (StatusCode, Json<Envelope<()>>)> {
    let user = require_authenticated(&headers, &state).await?;
    Ok(Json(success(
        "登录状态有效",
        SessionStatus {
            authenticated: true,
            user: Some(user_view(&state.database_path, &user.user_id)?),
        },
    )))
}

async fn create_session(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> Result<(HeaderMap, Json<Envelope<SessionStatus>>), (StatusCode, Json<Envelope<()>>)> {
    let (session_id,user_id)=ops::db::blocking(state.database_path.clone(),move |connection| {
        staff_sessions::login(connection,request,Utc::now().timestamp_millis())
    }).await.map_err(|message| error(if message == "账号或密码错误" {StatusCode::UNAUTHORIZED}else{StatusCode::SERVICE_UNAVAILABLE},message))?;
    let mut headers=HeaderMap::new();
    headers.insert(SET_COOKIE,HeaderValue::from_str(&format!(
        "license_center_session={session_id}; HttpOnly; Path=/; SameSite=Lax; Max-Age=604800{}",staff_cookie_secure_suffix()
    )).expect("generated session cookie is valid"));
    Ok((headers,Json(success("登录成功",SessionStatus{authenticated:true,user:Some(user_view(&state.database_path,&user_id)?)}))))
}

fn staff_cookie_secure_suffix() -> &'static str {
    let secure=std::env::var("YAYA_SESSION_SECURE_COOKIE").map(|v|v=="true" || v=="1").unwrap_or_else(|_| {
        std::env::var("YAYA_OPERATION_CENTER_WEB_ORIGIN").is_ok_and(|v|v.starts_with("https://"))
    });
    if secure {"; Secure"} else {""}
}

async fn close_session(
    State(state):State<AppState>,headers:HeaderMap,
) -> Result<(HeaderMap,Json<Envelope<()>>),(StatusCode,Json<Envelope<()>>)> {
    if let Some(origin)=headers.get("origin") {
        let origin=origin.to_str().unwrap_or("");
        let configured=std::env::var("YAYA_OPERATION_CENTER_WEB_ORIGIN").unwrap_or_default();
        if !["http://localhost:8778","http://127.0.0.1:8778"].contains(&origin) && (configured.is_empty() || origin!=configured.trim_end_matches('/')) {
            return Err(error(StatusCode::FORBIDDEN,"请求来源不被允许"));
        }
    }
    if let Some(token)=cookie_value(&headers,"license_center_session").map(str::to_owned) {
        ops::db::blocking(state.database_path.clone(),move |connection|staff_sessions::revoke(connection,&token,Utc::now().timestamp_millis())).await
            .map_err(|message|error(StatusCode::SERVICE_UNAVAILABLE,message))?;
    }
    let mut response=HeaderMap::new();
    response.insert(SET_COOKIE,HeaderValue::from_str(&format!("license_center_session=; HttpOnly; Path=/; SameSite=Lax; Max-Age=0{}",staff_cookie_secure_suffix())).unwrap());
    Ok((response,Json(success("已退出",()))))
}

async fn require_authenticated(
    headers: &HeaderMap,
    state: &AppState,
) -> Result<AuthenticatedUser, (StatusCode, Json<Envelope<()>>)> {
    let token=cookie_value(headers,"license_center_session").ok_or_else(||error(StatusCode::UNAUTHORIZED,"请先登录"))?.to_owned();
    // Only the existing, server-generated test-purchase adapter uses the ephemeral map.
    // Browser logins are never added to it, and cannot bypass persistent account checks.
    if let Some(user)=state.sessions.read().await.get(&token).filter(|u|u.user_id=="system-test-purchase").cloned() {
        return Ok(user);
    }
    ops::db::blocking(state.database_path.clone(),move |connection|staff_sessions::authenticate(connection,&token,Utc::now().timestamp_millis())).await
        .map_err(|message|error(StatusCode::SERVICE_UNAVAILABLE,message))?
        .ok_or_else(||error(StatusCode::UNAUTHORIZED,"登录已过期或账号权限已变更，请重新登录"))
}

async fn require_admin(
    headers: &HeaderMap,
    state: &AppState,
) -> Result<AuthenticatedUser, (StatusCode, Json<Envelope<()>>)> {
    let user = require_authenticated(headers, state).await?;
    if ["platform_admin", "platform_operator", "platform_finance"].contains(&user.role.as_str()) {
        Ok(user)
    } else {
        Err(error(StatusCode::FORBIDDEN, "当前账号没有平台运营权限"))
    }
}

async fn require_platform_admin(
    headers: &HeaderMap,
    state: &AppState,
) -> Result<AuthenticatedUser, (StatusCode, Json<Envelope<()>>)> {
    let user = require_authenticated(headers, state).await?;
    if user.role == "platform_admin" {
        Ok(user)
    } else {
        Err(error(StatusCode::FORBIDDEN, "仅平台超级管理员可管理账号"))
    }
}

fn user_view(
    database_path: &PathBuf,
    user_id: &str,
) -> Result<UserView, (StatusCode, Json<Envelope<()>>)> {
    Connection::open(database_path).map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "账号读取失败"))?
        .query_row("SELECT user_id, username, display_name, role, organization_id, status FROM users WHERE user_id = ?", [user_id], |row| Ok(UserView { user_id: row.get(0)?, username: row.get(1)?, display_name: row.get(2)?, role: row.get(3)?, organization_id: row.get(4)?, status: row.get(5)? }))
        .map_err(|_| error(StatusCode::UNAUTHORIZED, "账号不存在或已失效"))
}

async fn list_users(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Envelope<Vec<UserView>>>, (StatusCode, Json<Envelope<()>>)> {
    require_admin(&headers, &state).await?;
    let connection = Connection::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "账号读取失败"))?;
    let mut statement = connection.prepare("SELECT user_id, username, display_name, role, organization_id, status FROM users ORDER BY created_at DESC")
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "账号读取失败"))?;
    let users = statement
        .query_map([], |row| {
            Ok(UserView {
                user_id: row.get(0)?,
                username: row.get(1)?,
                display_name: row.get(2)?,
                role: row.get(3)?,
                organization_id: row.get(4)?,
                status: row.get(5)?,
            })
        })
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "账号读取失败"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "账号读取失败"))?;
    Ok(Json(success("账号已读取", users)))
}

async fn create_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateUserRequest>,
) -> Result<Json<Envelope<UserView>>, (StatusCode, Json<Envelope<()>>)> {
    require_platform_admin(&headers, &state).await?;
    platform_versions::check_origin(&headers)?;
    let username = request.username.trim();
    if username.len() < 3
        || username.len() > 64
        || !username
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "账号仅支持 3-64 位字母、数字及 _-.",
        ));
    }
    if request.password.is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "初始密码不能为空"));
    }
    if !FIXED_ROLES.contains(&request.role.as_str()) {
        return Err(error(StatusCode::BAD_REQUEST, "角色不在固定角色范围内"));
    }
    let organization_id = request.organization_id.as_deref().unwrap_or("platform").trim();
    if organization_id.is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "所属组织不能为空"));
    }
    let salt = SaltString::encode_b64(Uuid::new_v4().as_bytes())
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "密码处理失败"))?;
    let password_hash = Argon2::default()
        .hash_password(request.password.as_bytes(), &salt)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "密码处理失败"))?
        .to_string();
    let user = UserView {
        user_id: generated_resource_id("usr"),
        username: username.to_string(),
        display_name: request.display_name.trim().to_string(),
        role: request.role,
        organization_id: organization_id.to_string(),
        status: "active".to_string(),
    };
    let connection = Connection::open(&state.database_path)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "账号保存失败"))?;
    connection.execute("INSERT INTO users (user_id, username, password_hash, display_name, role, organization_id, status, created_at) VALUES (?, ?, ?, ?, ?, ?, 'active', ?)", params![user.user_id, user.username, password_hash, user.display_name, user.role, user.organization_id, Utc::now().timestamp()])
        .map_err(|_| error(StatusCode::CONFLICT, "账号已存在"))?;
    Ok(Json(success("账号已创建", user)))
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}

fn cookie_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get("cookie")?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|part| {
            let (key, value) = part.trim().split_once('=')?;
            (key == name).then_some(value)
        })
}

fn get_customer(
    connection: &Connection,
    customer_id: &str,
) -> rusqlite::Result<Option<CustomerRecord>> {
    connection
        .query_row(
            "SELECT customer_id, name, contact_name, contact_phone, contact_email,
                    status, notes, created_at
             FROM customers WHERE customer_id = ?",
            [customer_id],
            |row| {
                Ok(CustomerRecord {
                    customer_id: row.get(0)?,
                    name: row.get(1)?,
                    contact_name: row.get(2)?,
                    contact_phone: row.get(3)?,
                    contact_email: row.get(4)?,
                    status: row.get(5)?,
                    notes: row.get(6)?,
                    created_at: row.get(7)?,
                })
            },
        )
        .optional()
}

fn order_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<OrderRecord> {
    let items_json: String = row.get(4)?;
    let status:String=row.get(7)?;let received:i64=row.get(6)?;let total:i64=row.get(5)?;
    Ok(OrderRecord {
        payment_status:if status=="cancelled"{"cancelled"}else if received>=total||["paid","fulfilled"].contains(&status.as_str()){"paid"}else if received>0{"partial"}else{"unpaid"}.into(),
        fulfillment_status:if status=="fulfilled"{"completed"}else if status=="cancelled"{"cancelled"}else if status=="paid"{"pending"}else{"awaiting_payment"}.into(),
        order_id: row.get(0)?,
        order_no: row.get(1)?,
        customer_id: row.get(2)?,
        customer_name: row.get(3)?,
        items: serde_json::from_str(&items_json).unwrap_or_default(),
        total_amount_cents: row.get(5)?,
        received_amount_cents: row.get(6)?,
        status: row.get(7)?,
        created_at: row.get(8)?,
        due_at: row.get(9)?,
        deployment_type: row.get(10)?,
        paid_at: row.get(11)?,
        license_id: row.get(12)?,
        notes: row.get(13)?,
        provider_id: row.get(14)?,
    })
}

const ORDER_SELECT: &str = "SELECT o.order_id, o.order_no, o.customer_id, c.name, o.items_json,
            o.total_amount_cents, COALESCE(SUM(t.amount_cents), 0), o.status,
            o.created_at, o.due_at, o.deployment_type, o.paid_at, o.license_id, o.notes, o.provider_id
     FROM orders o
     JOIN customers c ON c.customer_id = o.customer_id
     LEFT JOIN transactions t ON t.order_id = o.order_id";

fn load_orders(
    database_path: &PathBuf,
    provider_id: Option<&str>,
) -> rusqlite::Result<Vec<OrderRecord>> {
    let connection = Connection::open(database_path)?;
    let sql = format!(
        "{ORDER_SELECT} {} GROUP BY o.order_id ORDER BY o.created_at DESC",
        if provider_id.is_some() {
            "WHERE o.provider_id = ?"
        } else {
            ""
        }
    );
    let mut statement = connection.prepare(&sql)?;
    if let Some(provider_id) = provider_id {
        statement
            .query_map([provider_id], order_from_row)?
            .collect()
    } else {
        statement.query_map([], order_from_row)?.collect()
    }
}

fn get_order(connection: &Connection, order_id: &str) -> rusqlite::Result<Option<OrderRecord>> {
    let sql = format!(
        "{ORDER_SELECT}
         WHERE o.order_id = ?
         GROUP BY o.order_id"
    );
    connection
        .query_row(&sql, [order_id], order_from_row)
        .optional()
}

fn ai_employee_product_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AiEmployeeProduct> {
    let skill_ids_json: String = row.get(7)?;
    let allowed_tools_json: String = row.get(10)?;
    let application_ids_json: String = row.get(11)?;
    let mut allowed_tools: Vec<String> = serde_json::from_str(&allowed_tools_json).unwrap_or_default();
    if allowed_tools.iter().any(|tool| tool == "browser_read_page") {
        allowed_tools.retain(|tool| tool != "browser_read_page");
        if !allowed_tools.iter().any(|tool| tool == "browser_control") {
            allowed_tools.push("browser_control".into());
        }
    }
    Ok(AiEmployeeProduct {
        id: row.get(0)?,
        title: row.get(1)?,
        description: row.get(2)?,
        category: row.get(3)?,
        price_cents: row.get(4)?,
        billing_cycle: row.get(5)?,
        version: row.get(6)?,
        skill_ids: serde_json::from_str(&skill_ids_json).unwrap_or_default(),
        system_prompt: row.get(8)?,
        allow_network: row.get(9)?,
        allowed_tools,
        application_ids: serde_json::from_str(&application_ids_json).unwrap_or_default(),
        avatar_url: row.get(12)?,
        status: row.get(13)?,
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
    })
}

fn load_ai_employee_products(connection: &Connection) -> rusqlite::Result<Vec<AiEmployeeProduct>> {
    let mut statement = connection.prepare(
        "SELECT id, title, description, category, price_cents, billing_cycle,
                version, skill_ids_json, system_prompt, allow_network,
                allowed_tools_json, application_ids_json, avatar_url, status, created_at, updated_at
         FROM ai_employee_products ORDER BY updated_at DESC",
    )?;
    statement
        .query_map([], ai_employee_product_from_row)?
        .collect()
}

fn get_ai_employee_product(
    connection: &Connection,
    employee_id: &str,
) -> rusqlite::Result<Option<AiEmployeeProduct>> {
    connection
        .query_row(
            "SELECT id, title, description, category, price_cents, billing_cycle,
                    version, skill_ids_json, system_prompt, allow_network,
                    allowed_tools_json, application_ids_json, avatar_url, status, created_at, updated_at
             FROM ai_employee_products WHERE id = ?",
            [employee_id],
            ai_employee_product_from_row,
        )
        .optional()
}

fn next_patch_version(version: &str) -> String {
    let (mut major, mut minor, mut patch) = version_parts(version);
    patch += 1;
    if patch >= 10 {
        patch = 0;
        minor += 1;
    }
    if minor >= 10 {
        minor = 0;
        major += 1;
    }
    format!("{major}.{minor}.{patch}")
}

fn version_parts(version: &str) -> (u64, u64, u64) {
    let mut parts = version.trim().trim_start_matches('v').split('.');
    (
        parts
            .next()
            .and_then(|value| value.parse().ok())
            .unwrap_or(1),
        parts
            .next()
            .and_then(|value| value.parse().ok())
            .unwrap_or(0),
        parts
            .next()
            .and_then(|value| value.parse().ok())
            .unwrap_or(0),
    )
}

fn bump_employee_versions_for_skill(
    connection: &Connection,
    skill_id: &str,
    updated_at: i64,
) -> rusqlite::Result<()> {
    for product in load_ai_employee_products(connection)? {
        if product.skill_ids.iter().any(|id| id == skill_id) {
            connection.execute(
                "UPDATE ai_employee_products SET version = ?, updated_at = ? WHERE id = ?",
                params![next_patch_version(&product.version), updated_at, product.id],
            )?;
        }
    }
    Ok(())
}

fn operation_skill_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<OperationSkill> {
    let package_path: String = row.get(5)?;
    let instructions: String = row.get(7)?;
    let environment_variables = skill_environment::discover(&package_path, &instructions);
    let mut environment_values: BTreeMap<String, String> =
        serde_json::from_str(&row.get::<_, String>(14)?).unwrap_or_default();
    // Preserve the old address only when this package actually consumes that variable.
    let legacy: String = row.get(13)?;
    if !legacy.is_empty() && !environment_values.contains_key("COMPONENT_LIBRARY_API_URL") {
        environment_values.insert("COMPONENT_LIBRARY_API_URL".into(), legacy);
    }
    environment_values.retain(|name, _| {
        environment_variables
            .iter()
            .any(|variable| variable.name == *name)
    });
    Ok(OperationSkill {
        id: row.get(0)?,
        title: row.get(1)?,
        package_name: row.get(2)?,
        source: row.get(3)?,
        version: row.get(4)?,
        package_path: row.get(5)?,
        description: row.get(6)?,
        instructions: row.get(7)?,
        environment_variables,
        environment_values,
        status: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
}

fn load_operation_skills(connection: &Connection) -> rusqlite::Result<Vec<OperationSkill>> {
    let mut statement = connection.prepare(
        "SELECT id, title, package_name, source, version, package_path,
                description, instructions, allowed_tools_json, application_ids_json,
                status, created_at, updated_at, component_library_api_url, environment_values_json
         FROM operation_skills ORDER BY updated_at DESC",
    )?;
    statement.query_map([], operation_skill_from_row)?.collect()
}

fn get_operation_skill(
    connection: &Connection,
    skill_id: &str,
) -> rusqlite::Result<Option<OperationSkill>> {
    connection
        .query_row(
            "SELECT id, title, package_name, source, version, package_path,
                description, instructions, allowed_tools_json, application_ids_json,
                status, created_at, updated_at, component_library_api_url, environment_values_json
         FROM operation_skills WHERE id = ?",
            [skill_id],
            operation_skill_from_row,
        )
        .optional()
}

fn validate_active_skills(
    connection: &Connection,
    skill_ids: &[String],
) -> Result<(), (StatusCode, Json<Envelope<()>>)> {
    for skill_id in skill_ids {
        let skill = get_operation_skill(connection, skill_id)
            .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Skill 读取失败"))?
            .ok_or_else(|| error(StatusCode::BAD_REQUEST, format!("Skill 不存在：{skill_id}")))?;
        if skill.status != "active" {
            return Err(error(
                StatusCode::BAD_REQUEST,
                format!("Skill 已停用：{}", skill.title),
            ));
        }
    }
    Ok(())
}

fn platform_tool_catalog() -> Vec<PlatformTool> {
    vec![
        platform_tool("knowledge_search", "检索知识库", "检索当前用户授权和任务快照内的知识资料，返回带版本与位置的来源；实际关键词、混合及重排模式以运行结果为准。", "knowledge", "read"),
        platform_tool("knowledge_read_document", "读取知识来源", "按本轮检索返回的引用读取有界原文，并重新校验用户权限。", "knowledge", "read"),
        platform_tool(
            "browser_control",
            "操作浏览器",
            "在桌面端通过 CDP 操作 Chrome / Edge，支持导航、读取、点击、填写、按键、滚动和截图；写操作遵循会话审批模式。",
            "skill",
            "execution",
        ),
        platform_tool(
            "skill_run_script",
            "运行 Skill 脚本",
            "在当前系统账户下运行已关联 Skill 脚本；没有系统级沙箱。依赖安装需另有终端执行能力。",
            "skill",
            "execution",
        ),
        platform_tool(
            "workspace_execute_code",
            "执行代码与终端命令",
            "允许会话内 Python、Node 和终端命令，包括按需 pip/npm 安装；当前账户执行，无系统级隔离，不额外弹出依赖安装确认。",
            "skill",
            "execution",
        ),
        platform_tool(
            "list_apps",
            "读取应用列表",
            "查询当前可访问的应用。",
            "app",
            "read",
        ),
        platform_tool(
            "create_app",
            "创建应用",
            "创建新的低代码应用。",
            "app",
            "write",
        ),
        platform_tool(
            "update_app",
            "编辑应用",
            "修改低代码应用的名称、描述或状态。",
            "app",
            "write",
        ),
        platform_tool(
            "delete_app",
            "删除应用",
            "永久删除低代码应用及其资源，始终要求人工确认。",
            "app",
            "destructive",
        ),
        platform_tool(
            "list_forms",
            "读取表单列表",
            "查询应用内表单元数据。",
            "form",
            "read",
        ),
        platform_tool(
            "list_navigation_groups",
            "读取导航分组",
            "读取应用导航分组的真实 ID 和层级，用于创建或移动表单。",
            "form",
            "read",
        ),
        platform_tool(
            "create_navigation_group",
            "创建导航分组",
            "在应用导航中创建根级或嵌套分组。",
            "form",
            "write",
        ),
        platform_tool(
            "delete_navigation_group",
            "删除导航分组",
            "删除分组容器并将内部项目上移，需用户确认。",
            "form",
            "destructive",
        ),
        platform_tool(
            "get_form_schema",
            "读取表单 Schema",
            "读取表单当前版本结构和字段。",
            "form",
            "read",
        ),
        platform_tool(
            "get_form_schema_contract",
            "读取表单 Schema 规范",
            "读取当前平台表单结构与规范。V2 配套提供只读组件目录及精确合同查询，无需预先创建参考表单或知识库。",
            "form",
            "read",
        ),
        platform_tool(
            "get_form_relationships",
            "读取表单关系",
            "读取 Schema 中的关联字段、目标表单和填充规则。",
            "form",
            "read",
        ),
        platform_tool(
            "get_related_records",
            "追溯关联记录",
            "沿已配置关联字段批量读取目标表单记录。",
            "form",
            "read",
        ),
        platform_tool(
            "list_form_records",
            "读取表单记录",
            "读取已授权表单最近的有限记录，用于业务分析。",
            "form",
            "read",
        ),
        platform_tool(
            "query_form_records",
            "条件查询表单记录",
            "在指定页的有限记录中按字段条件筛选，用于分析。",
            "form",
            "read",
        ),
        platform_tool(
            "aggregate_form_records",
            "汇总表单记录",
            "对有限页范围内的记录执行计数、求和、平均值或分组计数。",
            "form",
            "read",
        ),
        platform_tool(
            "get_detail_form_definition",
            "读取明细表定义",
            "读取明细表与父表、子表字段的关联。",
            "form",
            "read",
        ),
        platform_tool(
            "list_detail_records",
            "读取明细表记录",
            "读取父表子表字段中的有限明细行。",
            "form",
            "read",
        ),
        platform_tool(
            "create_form",
            "创建表单",
            "创建表单；可选 schema 同时校验并事务保存初始字段/布局，不传则创建空白表单。传 schema 还要求开发视图、编辑及关联资源权限；流程图另行配置。",
            "form",
            "write",
        ),
        platform_tool(
            "move_form_to_group",
            "移动表单到分组",
            "将已有表单移动到指定导航分组或根级。",
            "form",
            "write",
        ),
        platform_tool(
            "create_detail_form",
            "生成明细表配置",
            "为父表的 subform 字段生成明细表。",
            "form",
            "write",
        ),
        platform_tool(
            "save_form_schema",
            "保存表单 Schema",
            "保存表单 Schema 后立即成为当前版本。",
            "form",
            "write",
        ),
        platform_tool(
            "yaya_apply_design_patch",
            "修改表单设计",
            "修改普通表单或工作流表单设计；按当前用户 RBAC 校验，所有修改均须用户确认。",
            "form",
            "write",
        ),
        platform_tool(
            "delete_form",
            "删除表单",
            "永久删除表单及其 Schema、记录、导航和关联资源，始终要求人工确认。",
            "form",
            "destructive",
        ),
        platform_tool(
            "list_automations",
            "读取自动化列表",
            "查询应用内集成自动化。",
            "automation",
            "read",
        ),
        platform_tool(
            "get_automation_graph",
            "读取自动化流程",
            "读取自动化的触发器、节点和连线。",
            "automation",
            "read",
        ),
        platform_tool(
            "create_automation_draft",
            "创建集成自动化",
            "预览并确认后创建处于暂停状态的集成自动化；不启用、不执行。V2 提供配套的只读节点规范及纯设计校验。",
            "automation",
            "write",
        ),
        platform_tool(
            "yaya_update_automation",
            "修改集成自动化设计",
            "基于当前版本预览并确认后修改已暂停的集成自动化节点和连线；不修改审批流程，不启用或试运行。",
            "automation",
            "write",
        ),
        platform_tool(
            "delete_automation",
            "删除集成自动化",
            "永久删除普通事件集成自动化，始终要求人工确认。",
            "automation",
            "destructive",
        ),
        platform_tool(
            "get_workflow_process_definition",
            "读取工作流定义",
            "读取工作流表单的流程节点和连线。",
            "workflow",
            "read",
        ),
        platform_tool(
            "yaya_save_workflow_process",
            "配置审批流程设计",
            "读取规范和真实成员/角色，校验后预览并确认保存流程表单的审批、执行、抄送及条件节点。保留启用状态；启用中的流程对后续提交立即生效，不提交记录、不代审批或试跑。",
            "workflow",
            "write",
        ),
        platform_tool(
            "get_workflow_record_runtime",
            "读取工作流运行态",
            "读取一条工作流记录的实例、待办和动作轨迹。",
            "workflow",
            "read",
        ),
    ]
}

fn platform_tool(
    id: &'static str,
    title: &'static str,
    description: &'static str,
    category: &'static str,
    risk_level: &'static str,
) -> PlatformTool {
    PlatformTool {
        id,
        title,
        description,
        category,
        group: platform_tool_group(id, category),
        risk_level,
    }
}

fn platform_tool_group(id: &str, category: &str) -> &'static str {
    match (category, id) {
        ("app", "create_app" | "update_app" | "delete_app") => "构建",
        ("app", "list_apps") => "分析",
        (
            "form",
            "list_forms"
            | "list_navigation_groups"
            | "get_form_schema"
            | "get_form_schema_contract"
            | "get_form_relationships"
            | "get_related_records"
            | "list_form_records"
            | "query_form_records"
            | "aggregate_form_records"
            | "get_detail_form_definition"
            | "list_detail_records",
        ) => "分析",
        ("form", _) => "构建",
        ("automation", "list_automations" | "get_automation_graph") => "分析",
        ("automation", _) => "构建",
        ("workflow", _) => "分析",
        ("plugin" | "script", _) => "扩展",
        _ => "通用",
    }
}

fn normalize_package_name(value: &str) -> String {
    let normalized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if normalized.is_empty() {
        "skill".to_string()
    } else {
        normalized
    }
}

fn extract_skill_package(package_name: &str, archive: &[u8]) -> Result<(String, String), String> {
    let root = PathBuf::from("runtime/skills");
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let package_dir = root.join(package_name);
    let staging_dir = root.join(format!(".skill-upload-{}", Uuid::new_v4().simple()));
    let instructions = match extract_skill_package_to_directory(archive, &staging_dir) {
        Ok(instructions) => instructions,
        Err(message) => {
            let _ = fs::remove_dir_all(&staging_dir);
            return Err(message);
        }
    };
    let backup_dir = root.join(format!(".skill-backup-{}", Uuid::new_v4().simple()));
    let had_existing_package = package_dir.exists();
    if had_existing_package {
        fs::rename(&package_dir, &backup_dir).map_err(|error| {
            let _ = fs::remove_dir_all(&staging_dir);
            error.to_string()
        })?;
    }
    if let Err(error) = fs::rename(&staging_dir, &package_dir) {
        if had_existing_package {
            let _ = fs::rename(&backup_dir, &package_dir);
        }
        let _ = fs::remove_dir_all(&staging_dir);
        return Err(error.to_string());
    }
    if had_existing_package {
        let _ = fs::remove_dir_all(&backup_dir);
    }
    let package_path = package_dir
        .join("SKILL.md")
        .to_string_lossy()
        .replace('\\', "/");
    Ok((instructions, package_path))
}

fn extract_skill_package_to_directory(
    archive: &[u8],
    package_dir: &FilePath,
) -> Result<String, String> {
    const MAX_ARCHIVE_BYTES: usize = 10 * 1024 * 1024;
    const MAX_FILES: usize = 256;
    const MAX_UNCOMPRESSED_BYTES: u64 = 20 * 1024 * 1024;
    if archive.is_empty() || archive.len() > MAX_ARCHIVE_BYTES {
        return Err("Skill 压缩包不能为空且不得超过 10 MB".to_string());
    }
    let mut zip = ZipArchive::new(Cursor::new(archive))
        .map_err(|error| format!("无法读取 Skill ZIP 文件：{error}"))?;
    if zip.len() > MAX_FILES {
        return Err("Skill 压缩包中的文件数量不能超过 256 个".to_string());
    }
    let mut skill_prefix = None;
    let mut total_size = 0_u64;
    for index in 0..zip.len() {
        let file = zip.by_index(index).map_err(|error| error.to_string())?;
        let path = file
            .enclosed_name()
            .ok_or_else(|| "Skill 压缩包包含不安全的文件路径".to_string())?;
        total_size = total_size.saturating_add(file.size());
        if total_size > MAX_UNCOMPRESSED_BYTES {
            return Err("Skill 压缩包解压后不得超过 20 MB".to_string());
        }
        if !file.is_dir() && path.file_name().is_some_and(|name| name == "SKILL.md") {
            skill_prefix = Some(
                path.parent()
                    .unwrap_or_else(|| FilePath::new(""))
                    .to_path_buf(),
            );
        }
    }
    let prefix = skill_prefix.ok_or_else(|| "Skill 压缩包必须包含 SKILL.md".to_string())?;
    fs::create_dir_all(&package_dir).map_err(|error| error.to_string())?;
    let mut instructions = None;
    for index in 0..zip.len() {
        let mut file = zip.by_index(index).map_err(|error| error.to_string())?;
        let source = file
            .enclosed_name()
            .ok_or_else(|| "Skill 压缩包包含不安全的文件路径".to_string())?;
        if !source.starts_with(&prefix) {
            continue;
        }
        let relative = source
            .strip_prefix(&prefix)
            .map_err(|_| "Skill 压缩包目录结构无效".to_string())?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        let target = package_dir.join(relative);
        if file.is_dir() {
            fs::create_dir_all(&target).map_err(|error| error.to_string())?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let mut bytes = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if relative == FilePath::new("SKILL.md") {
            instructions = Some(
                String::from_utf8(bytes.clone())
                    .map_err(|_| "SKILL.md 必须是 UTF-8 文本".to_string())?,
            );
        }
        fs::write(target, bytes).map_err(|error| error.to_string())?;
    }
    let instructions = instructions.ok_or_else(|| "Skill 压缩包必须包含 SKILL.md".to_string())?;
    Ok(instructions)
}

fn archive_operation_skill_package(skill: &OperationSkill) -> Result<Vec<u8>, String> {
    let package_name = FilePath::new(&skill.package_name);
    if package_name.components().count() != 1
        || !matches!(package_name.components().next(), Some(Component::Normal(_)))
    {
        return Err("Skill 包名无效".to_string());
    }
    let root = PathBuf::from("runtime/skills");
    let package_dir = root.join(package_name);
    if !package_dir.join("SKILL.md").is_file() {
        return Err("Skill 文件包不存在".to_string());
    }
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    archive_skill_directory(&mut writer, &package_dir, &package_dir)?;
    writer
        .finish()
        .map(Cursor::into_inner)
        .map_err(|error| format!("无法打包 Skill 文件：{error}"))
}

fn archive_skill_directory(
    writer: &mut ZipWriter<Cursor<Vec<u8>>>,
    root: &FilePath,
    directory: &FilePath,
) -> Result<(), String> {
    for entry in fs::read_dir(directory).map_err(|error| format!("无法读取 Skill 文件：{error}"))?
    {
        let entry = entry.map_err(|error| format!("无法读取 Skill 文件：{error}"))?;
        let path = entry.path();
        let metadata =
            fs::symlink_metadata(&path).map_err(|error| format!("无法读取 Skill 文件：{error}"))?;
        if metadata.file_type().is_symlink() {
            return Err("Skill 文件包不能包含符号链接".to_string());
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|_| "Skill 文件路径无效".to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        if metadata.is_dir() {
            writer
                .add_directory(format!("{relative}/"), SimpleFileOptions::default())
                .map_err(|error| format!("无法写入 Skill 文件包：{error}"))?;
            archive_skill_directory(writer, root, &path)?;
        } else if metadata.is_file() {
            writer
                .start_file(
                    relative,
                    SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Deflated),
                )
                .map_err(|error| format!("无法写入 Skill 文件包：{error}"))?;
            let bytes = fs::read(&path).map_err(|error| format!("无法读取 Skill 文件：{error}"))?;
            writer
                .write_all(&bytes)
                .map_err(|error| format!("无法写入 Skill 文件包：{error}"))?;
        }
    }
    Ok(())
}

fn insert_audit_log(database_path: &PathBuf, record: &AuditLogRecord) -> rusqlite::Result<()> {
    let connection = Connection::open(database_path)?;
    let customer = resolve_audit_log_customer(&connection, record.path.as_deref())?;
    let customer_id = customer.as_ref().map(|value| value.customer_id.as_str());
    let customer_name = customer
        .as_ref()
        .map(|value| value.customer_name.as_str())
        .unwrap_or("无客户");
    connection.execute(
        "INSERT INTO audit_logs (
            created_at, level, source, endpoint_title, message, ip_address, method, path,
            status_code, duration_ms, user_agent, response_body, customer_id, customer_name
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            record.created_at,
            record.level,
            record.source,
            record.endpoint_title,
            record.message,
            record.ip_address,
            record.method,
            record.path,
            record.status_code,
            record.duration_ms,
            record.user_agent,
            record.response_body,
            customer_id,
            customer_name,
        ],
    )?;
    if connection.last_insert_rowid() % 500 == 0 {
        let cutoff = Utc::now().timestamp() - 90 * 24 * 60 * 60;
        connection.execute(
            "DELETE FROM audit_logs WHERE created_at < ?",
            params![cutoff],
        )?;
        connection.execute(
            "DELETE FROM audit_logs
             WHERE id NOT IN (SELECT id FROM audit_logs ORDER BY id DESC LIMIT 50000)",
            [],
        )?;
    }
    Ok(())
}

fn resolve_audit_log_customer(
    connection: &Connection,
    path: Option<&str>,
) -> rusqlite::Result<Option<AuditLogCustomer>> {
    let Some(path) = path else {
        return Ok(None);
    };
    let segments = path.trim_matches('/').split('/').collect::<Vec<_>>();
    let lookup_customer = |customer_id: &str| {
        connection
            .query_row(
                "SELECT customer_id, name FROM customers WHERE customer_id = ?",
                [customer_id],
                |row| {
                    Ok(AuditLogCustomer {
                        customer_id: row.get(0)?,
                        customer_name: row.get(1)?,
                    })
                },
            )
            .optional()
    };

    match segments.as_slice() {
        ["api", "customers", customer_id, ..] => lookup_customer(customer_id),
        ["api", "orders", order_id, ..] => connection
            .query_row(
                "SELECT c.customer_id, c.name
                 FROM orders o
                 JOIN customers c ON c.customer_id = o.customer_id
                 WHERE o.order_id = ?",
                [order_id],
                |row| {
                    Ok(AuditLogCustomer {
                        customer_id: row.get(0)?,
                        customer_name: row.get(1)?,
                    })
                },
            )
            .optional(),
        ["api", "licenses", license_id, ..] => connection
            .query_row(
                "SELECT subject, customer_name_snapshot FROM licenses WHERE license_id = ?",
                [license_id],
                |row| {
                    let customer_id: String = row.get(0)?;
                    let customer_name: String = row.get(1)?;
                    Ok(AuditLogCustomer {
                        customer_id: customer_id.clone(),
                        customer_name: if customer_name.trim().is_empty() {
                            customer_id
                        } else {
                            customer_name
                        },
                    })
                },
            )
            .optional(),
        _ => Ok(None),
    }
}

fn backfill_audit_log_customers(connection: &Connection) -> rusqlite::Result<()> {
    let records = {
        let mut statement = connection.prepare(
            "SELECT id, path FROM audit_logs
             WHERE customer_id IS NULL AND (customer_name IS NULL OR customer_name = '')
                   AND path IS NOT NULL",
        )?;
        statement
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    for (id, path) in records {
        if let Some(customer) = resolve_audit_log_customer(connection, Some(&path))? {
            connection.execute(
                "UPDATE audit_logs SET customer_id = ?, customer_name = ? WHERE id = ?",
                params![customer.customer_id, customer.customer_name, id],
            )?;
        }
    }
    Ok(())
}

fn list_audit_log_records(
    database_path: &PathBuf,
    query: &AuditLogQuery,
) -> rusqlite::Result<Vec<AuditLogRecord>> {
    let connection = Connection::open(database_path)?;
    let mut statement = connection.prepare(
        "SELECT id, created_at, level, source, endpoint_title, message, ip_address, method, path,
                status_code, duration_ms, user_agent, response_body, customer_id,
                COALESCE(NULLIF(customer_name, ''), '无客户')
         FROM audit_logs
         WHERE (path IS NULL OR path <> '/api/logs')
           AND (:before IS NULL OR id < :before)
           AND (:start IS NULL OR created_at >= :start)
           AND (:end IS NULL OR created_at < :end)
           AND (:level IS NULL OR level = :level)
           AND (:customer IS NULL OR customer_id = :customer OR (:customer = '__none__' AND (customer_id IS NULL OR customer_id = '')))
           AND (:search = '' OR instr(lower(COALESCE(message,'') || ' ' || COALESCE(path,'') || ' ' || COALESCE(customer_name,'') || ' ' || COALESCE(endpoint_title,'') || ' ' || COALESCE(source,'') || ' ' || COALESCE(ip_address,'') || ' ' || COALESCE(status_code,'')), lower(:search)) > 0)
         ORDER BY id DESC
         LIMIT :limit",
    )?;
    statement
        .query_map(rusqlite::named_params! {
            ":limit": query.limit.unwrap_or(300).clamp(1, 1000),
            ":before": query.before,
            ":start": query.start,
            ":end": query.end,
            ":level": query.level,
            ":customer": query.customer_id,
            ":search": query.search.as_deref().unwrap_or("").trim(),
        }, |row| {
            Ok(AuditLogRecord {
                id: row.get(0)?,
                created_at: row.get(1)?,
                level: row.get(2)?,
                source: row.get(3)?,
                endpoint_title: row.get(4)?,
                message: row.get(5)?,
                ip_address: row.get(6)?,
                method: row.get(7)?,
                path: row.get(8)?,
                status_code: row.get(9)?,
                duration_ms: row.get(10)?,
                user_agent: row.get(11)?,
                response_body: row.get(12)?,
                customer_id: row.get(13)?,
                customer_name: row.get(14)?,
            })
        })?
        .collect()
}

fn scalar_i64<P: rusqlite::Params>(
    connection: &Connection,
    sql: &str,
    params: P,
) -> Result<i64, (StatusCode, Json<Envelope<()>>)> {
    connection
        .query_row(sql, params, |row| row.get(0))
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "财务汇总读取失败"))
}

fn initialize_database(
    database_path: &PathBuf,
    legacy_licenses_path: &PathBuf,
    legacy_revoked_path: &PathBuf,
    initial_admin_username: &str,
    initial_admin_password: Option<&str>,
) -> Result<(), String> {
    if let Some(parent) = database_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut connection = Connection::open(database_path).map_err(|error| error.to_string())?;
    connection
        .execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS users (
                user_id TEXT PRIMARY KEY NOT NULL,
                username TEXT UNIQUE NOT NULL,
                password_hash TEXT NOT NULL,
                display_name TEXT NOT NULL DEFAULT '',
                role TEXT NOT NULL CHECK (role IN ('platform_admin', 'platform_operator', 'platform_finance', 'provider_admin', 'provider_sales')),
                organization_id TEXT NOT NULL DEFAULT 'platform',
                status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'inactive')),
                created_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS providers (provider_id TEXT PRIMARY KEY, name TEXT NOT NULL, contact_name TEXT NOT NULL DEFAULT '', contact_phone TEXT NOT NULL DEFAULT '', status TEXT NOT NULL DEFAULT 'active', created_at INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS commission_rules (rule_id TEXT PRIMARY KEY, product_type TEXT NOT NULL, rate_basis_points INTEGER NOT NULL, status TEXT NOT NULL DEFAULT 'active', effective_at INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS commission_entries (entry_id TEXT PRIMARY KEY, order_id TEXT NOT NULL, provider_id TEXT NOT NULL, product_type TEXT NOT NULL, received_amount_cents INTEGER NOT NULL, rate_basis_points INTEGER NOT NULL, commission_amount_cents INTEGER NOT NULL, status TEXT NOT NULL DEFAULT 'pending_settlement', created_at INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS settlement_batches (batch_id TEXT PRIMARY KEY, provider_id TEXT NOT NULL, total_amount_cents INTEGER NOT NULL, entry_count INTEGER NOT NULL, status TEXT NOT NULL DEFAULT 'settled', settled_at INTEGER NOT NULL, notes TEXT NOT NULL DEFAULT '');
            CREATE TABLE IF NOT EXISTS licenses (
                license_id TEXT PRIMARY KEY NOT NULL,
                license TEXT NOT NULL,
                subject TEXT NOT NULL,
                customer_name_snapshot TEXT NOT NULL DEFAULT '',
                modules_json TEXT NOT NULL,
                issued_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                module_expires_at_json TEXT NOT NULL,
                ai_employees_json TEXT NOT NULL DEFAULT '[]',
                platform_status TEXT NOT NULL,
                module_statuses_json TEXT NOT NULL,
                order_id TEXT NOT NULL UNIQUE,
                linkage_status TEXT NOT NULL DEFAULT 'linked',
                FOREIGN KEY(order_id) REFERENCES orders(order_id)
            );
            CREATE TABLE IF NOT EXISTS legacy_licenses (
                license_id TEXT PRIMARY KEY NOT NULL,
                license TEXT NOT NULL,
                subject TEXT NOT NULL,
                customer_name_snapshot TEXT NOT NULL DEFAULT '',
                modules_json TEXT NOT NULL,
                issued_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                module_expires_at_json TEXT NOT NULL,
                ai_employees_json TEXT NOT NULL DEFAULT '[]',
                platform_status TEXT NOT NULL,
                module_statuses_json TEXT NOT NULL,
                order_id TEXT,
                linkage_status TEXT NOT NULL DEFAULT 'legacy_unlinked'
            );
            CREATE TABLE IF NOT EXISTS customers (
                customer_id TEXT PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                contact_name TEXT NOT NULL DEFAULT '',
                contact_phone TEXT NOT NULL DEFAULT '',
                contact_email TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'active',
                notes TEXT NOT NULL DEFAULT '',
                created_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS orders (
                order_id TEXT PRIMARY KEY NOT NULL,
                order_no TEXT UNIQUE NOT NULL,
                customer_id TEXT NOT NULL,
                items_json TEXT NOT NULL,
                total_amount_cents INTEGER NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending_payment',
                created_at INTEGER NOT NULL,
                due_at INTEGER,
                deployment_type TEXT NOT NULL DEFAULT 'saas',
                paid_at INTEGER,
                license_id TEXT,
                notes TEXT NOT NULL DEFAULT '',
                FOREIGN KEY(customer_id) REFERENCES customers(customer_id)
            );
            CREATE TABLE IF NOT EXISTS transactions (
                transaction_id TEXT PRIMARY KEY NOT NULL,
                order_id TEXT NOT NULL,
                customer_id TEXT NOT NULL,
                amount_cents INTEGER NOT NULL,
                method TEXT NOT NULL,
                reference TEXT NOT NULL DEFAULT '',
                notes TEXT NOT NULL DEFAULT '',
                occurred_at INTEGER NOT NULL,
                FOREIGN KEY(order_id) REFERENCES orders(order_id),
                FOREIGN KEY(customer_id) REFERENCES customers(customer_id)
            );
            CREATE TABLE IF NOT EXISTS ai_employee_products (
                id TEXT PRIMARY KEY NOT NULL,
                title TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                category TEXT NOT NULL DEFAULT '通用',
                price_cents INTEGER NOT NULL,
                billing_cycle TEXT NOT NULL DEFAULT 'year',
                version TEXT NOT NULL DEFAULT '1.0.0',
                skill_ids_json TEXT NOT NULL DEFAULT '[]',
                system_prompt TEXT NOT NULL DEFAULT '',
                allow_network INTEGER NOT NULL DEFAULT 0,
                allowed_tools_json TEXT NOT NULL DEFAULT '[]',
                application_ids_json TEXT NOT NULL DEFAULT '[]',
                avatar_url TEXT,
                status TEXT NOT NULL DEFAULT 'active',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS application_releases (
                submission_id TEXT PRIMARY KEY NOT NULL,
                app_id TEXT NOT NULL,
                app_name TEXT NOT NULL,
                version TEXT NOT NULL DEFAULT '1.0.0',
                status TEXT NOT NULL DEFAULT 'pending_review',
                submitted_by TEXT NOT NULL,
                applicant_subject TEXT NOT NULL DEFAULT '',
                submitted_at INTEGER NOT NULL,
                snapshot_json TEXT NOT NULL,
                reviewed_at INTEGER,
                review_reason TEXT NOT NULL DEFAULT ''
            );
            CREATE INDEX IF NOT EXISTS idx_application_releases_status ON application_releases(status, submitted_at DESC);
            CREATE TABLE IF NOT EXISTS application_products (
                product_id TEXT PRIMARY KEY NOT NULL,
                publisher_subject TEXT NOT NULL,
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'active'
            );
            CREATE TABLE IF NOT EXISTS application_listing_versions (
                product_id TEXT PRIMARY KEY NOT NULL,
                release_id TEXT NOT NULL,
                listing_status TEXT NOT NULL DEFAULT 'withdrawn',
                published_at INTEGER,
                withdrawn_at INTEGER
            );
            CREATE TABLE IF NOT EXISTS application_download_tickets (
                ticket TEXT PRIMARY KEY NOT NULL,
                release_id TEXT NOT NULL,
                subject TEXT NOT NULL,
                expires_at INTEGER NOT NULL,
                used_at INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_application_download_tickets_expiry ON application_download_tickets(expires_at);
            CREATE TABLE IF NOT EXISTS application_entitlements (
                subject TEXT NOT NULL,
                product_id TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'active',
                expires_at INTEGER,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY(subject, product_id)
            );
            CREATE INDEX IF NOT EXISTS idx_application_entitlements_product ON application_entitlements(product_id, status);
            CREATE TABLE IF NOT EXISTS operation_skills (
                id TEXT PRIMARY KEY NOT NULL,
                title TEXT NOT NULL,
                package_name TEXT NOT NULL DEFAULT '',
                source TEXT NOT NULL DEFAULT 'operation-center',
                version TEXT NOT NULL DEFAULT '1.0.0',
                package_path TEXT NOT NULL DEFAULT '',
                description TEXT NOT NULL DEFAULT '',
                instructions TEXT NOT NULL,
                allowed_tools_json TEXT NOT NULL DEFAULT '[]',
                application_ids_json TEXT NOT NULL DEFAULT '[]',
                status TEXT NOT NULL DEFAULT 'active',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS audit_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at INTEGER NOT NULL,
                level TEXT NOT NULL,
                source TEXT NOT NULL,
                endpoint_title TEXT NOT NULL DEFAULT '运营接口',
                message TEXT NOT NULL,
                ip_address TEXT,
                method TEXT,
                path TEXT,
                status_code INTEGER,
                duration_ms INTEGER,
                user_agent TEXT,
                response_body TEXT,
                customer_id TEXT,
                customer_name TEXT NOT NULL DEFAULT ''
            );
            CREATE INDEX IF NOT EXISTS idx_orders_customer ON orders(customer_id);
            CREATE INDEX IF NOT EXISTS idx_orders_status ON orders(status);
            CREATE INDEX IF NOT EXISTS idx_transactions_order ON transactions(order_id);
            CREATE INDEX IF NOT EXISTS idx_transactions_occurred ON transactions(occurred_at);
            CREATE INDEX IF NOT EXISTS idx_audit_logs_created ON audit_logs(created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_audit_logs_level ON audit_logs(level, created_at DESC);
            ",
        )
        .map_err(|error| error.to_string())?;
    let has_primary_member = connection.prepare("PRAGMA table_info(application_products)")
        .map_err(|e| e.to_string())?.query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| e.to_string())?.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?
        .iter().any(|name| name == "publisher_app_id");
    if has_primary_member {
        connection.execute_batch("ALTER TABLE application_products DROP COLUMN publisher_app_id;").map_err(|e| e.to_string())?;
    }
    application_commerce::initialize(&connection)?;
    let user_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    if let Ok(reset_password) = std::env::var("YAYA_OPERATION_CENTER_RESET_ADMIN_PASSWORD") {
        if reset_password.is_empty() {
            return Err("管理员重置密码不能为空".to_string());
        }
        let salt =
            SaltString::encode_b64(Uuid::new_v4().as_bytes()).map_err(|error| error.to_string())?;
        let password_hash = Argon2::default()
            .hash_password(reset_password.as_bytes(), &salt)
            .map_err(|error| error.to_string())?
            .to_string();
        let updated = connection
            .execute(
                "UPDATE users SET password_hash = ?, status = 'active' WHERE username = 'admin'",
                params![password_hash],
            )
            .map_err(|error| error.to_string())?;
        if updated == 0 {
            return Err("未找到 admin 账号，无法重置管理员密码".to_string());
        }
    }
    if user_count == 0 {
        let initial_admin_password = initial_admin_password.ok_or_else(|| {
            "首次启动需要设置 YAYA_OPERATION_CENTER_INITIAL_ADMIN_PASSWORD".to_string()
        })?;
        if initial_admin_password.is_empty() {
            return Err("初始管理员密码不能为空".to_string());
        }
        let salt =
            SaltString::encode_b64(Uuid::new_v4().as_bytes()).map_err(|error| error.to_string())?;
        let password_hash = Argon2::default()
            .hash_password(initial_admin_password.as_bytes(), &salt)
            .map_err(|error| error.to_string())?
            .to_string();
        connection.execute("INSERT INTO users (user_id, username, password_hash, display_name, role, organization_id, status, created_at) VALUES (?, ?, ?, '平台超级管理员', 'platform_admin', 'platform', 'active', ?)", params![generated_resource_id("usr"), initial_admin_username, password_hash, Utc::now().timestamp()]).map_err(|error| error.to_string())?;
    }
    employee_knowledge::migrate(&connection).map_err(|e| e.to_string())?;
    remove_persona_schema(&connection)?;
    remove_skill_legacy_schema(&connection)?;
    ensure_column(
        &connection,
        "commission_entries",
        "settlement_batch_id",
        "ALTER TABLE commission_entries ADD COLUMN settlement_batch_id TEXT",
    )?;
    ensure_column(
        &connection,
        "orders",
        "provider_id",
        "ALTER TABLE orders ADD COLUMN provider_id TEXT",
    )?;
    ensure_column(
        &connection,
        "audit_logs",
        "endpoint_title",
        "ALTER TABLE audit_logs ADD COLUMN endpoint_title TEXT NOT NULL DEFAULT '运营接口'",
    )?;
    ensure_column(
        &connection,
        "audit_logs",
        "response_body",
        "ALTER TABLE audit_logs ADD COLUMN response_body TEXT",
    )?;
    ensure_column(
        &connection,
        "audit_logs",
        "customer_id",
        "ALTER TABLE audit_logs ADD COLUMN customer_id TEXT",
    )?;
    ensure_column(
        &connection,
        "audit_logs",
        "customer_name",
        "ALTER TABLE audit_logs ADD COLUMN customer_name TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        &connection,
        "orders",
        "deployment_type",
        "ALTER TABLE orders ADD COLUMN deployment_type TEXT NOT NULL DEFAULT 'saas'",
    )?;
    ensure_column(
        &connection,
        "ai_employee_products",
        "version",
        "ALTER TABLE ai_employee_products ADD COLUMN version TEXT NOT NULL DEFAULT '1.0.0'",
    )?;
    ensure_column(
        &connection,
        "ai_employee_products",
        "skill_ids_json",
        "ALTER TABLE ai_employee_products ADD COLUMN skill_ids_json TEXT NOT NULL DEFAULT '[]'",
    )?;
    ensure_column(
        &connection,
        "ai_employee_products",
        "system_prompt",
        "ALTER TABLE ai_employee_products ADD COLUMN system_prompt TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        &connection,
        "ai_employee_products",
        "allow_network",
        "ALTER TABLE ai_employee_products ADD COLUMN allow_network INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        &connection,
        "ai_employee_products",
        "allowed_tools_json",
        "ALTER TABLE ai_employee_products ADD COLUMN allowed_tools_json TEXT NOT NULL DEFAULT '[]'",
    )?;
    ensure_column(
        &connection,
        "ai_employee_products",
        "application_ids_json",
        "ALTER TABLE ai_employee_products ADD COLUMN application_ids_json TEXT NOT NULL DEFAULT '[]'",
    )?;
    ensure_column(
        &connection,
        "ai_employee_products",
        "avatar_url",
        "ALTER TABLE ai_employee_products ADD COLUMN avatar_url TEXT",
    )?;
    ensure_column(
        &connection,
        "operation_skills",
        "environment_values_json",
        "ALTER TABLE operation_skills ADD COLUMN environment_values_json TEXT NOT NULL DEFAULT '{}'",
    )?;
    ensure_column(
        &connection,
        "operation_skills",
        "component_library_api_url",
        "ALTER TABLE operation_skills ADD COLUMN component_library_api_url TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        &connection,
        "operation_skills",
        "package_name",
        "ALTER TABLE operation_skills ADD COLUMN package_name TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        &connection,
        "operation_skills",
        "source",
        "ALTER TABLE operation_skills ADD COLUMN source TEXT NOT NULL DEFAULT 'operation-center'",
    )?;
    ensure_column(
        &connection,
        "operation_skills",
        "version",
        "ALTER TABLE operation_skills ADD COLUMN version TEXT NOT NULL DEFAULT '1.0.0'",
    )?;
    ensure_column(
        &connection,
        "operation_skills",
        "package_path",
        "ALTER TABLE operation_skills ADD COLUMN package_path TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        &connection,
        "operation_skills",
        "application_ids_json",
        "ALTER TABLE operation_skills ADD COLUMN application_ids_json TEXT NOT NULL DEFAULT '[]'",
    )?;
    // Skill-level platform permissions belonged to the removed architecture.
    // Keep the physical columns temporarily for SQLite compatibility, but
    // clear their data and never expose or consume it.
    connection
        .execute(
            "UPDATE operation_skills SET allowed_tools_json = '[]', application_ids_json = '[]'",
            [],
        )
        .map_err(|error| error.to_string())?;
    let has_ai_employees_column = {
        let mut statement = connection
            .prepare("PRAGMA table_info(licenses)")
            .map_err(|error| error.to_string())?;
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|error| error.to_string())?;
        columns
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?
            .iter()
            .any(|column| column == "ai_employees_json")
    };
    if !has_ai_employees_column {
        connection
            .execute(
                "ALTER TABLE licenses ADD COLUMN ai_employees_json TEXT NOT NULL DEFAULT '[]'",
                [],
            )
            .map_err(|error| error.to_string())?;
    }
    let has_order_id_column = {
        let mut statement = connection
            .prepare("PRAGMA table_info(licenses)")
            .map_err(|error| error.to_string())?;
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|error| error.to_string())?;
        columns
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?
            .iter()
            .any(|column| column == "order_id")
    };
    if !has_order_id_column {
        connection
            .execute("ALTER TABLE licenses ADD COLUMN order_id TEXT", [])
            .map_err(|error| error.to_string())?;
    }
    migrate_license_relation_schema(&mut connection)?;
    // Keep application release schema up to date even when license data is already initialized.
    ensure_column(&connection,"application_releases","product_id","ALTER TABLE application_releases ADD COLUMN product_id TEXT")?;
    ensure_column(
        &connection,
        "application_releases",
        "applicant_subject",
        "ALTER TABLE application_releases ADD COLUMN applicant_subject TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        &connection,
        "application_releases",
        "version",
        "ALTER TABLE application_releases ADD COLUMN version TEXT NOT NULL DEFAULT '1.0.0'",
    )?;

    let record_count: i64 = connection
        .query_row(
            "SELECT (SELECT COUNT(*) FROM licenses) + (SELECT COUNT(*) FROM legacy_licenses)",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if record_count != 0 {
        licensing::initialize(&mut connection)?;
        backfill_audit_log_customers(&connection).map_err(|error| error.to_string())?;
        return Ok(());
    }

    let revoked = load_revoked(legacy_revoked_path);
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    for mut license in load_licenses(legacy_licenses_path) {
        if revoked.contains(&license.license_id) {
            license.platform_status = "destroyed".to_string();
            license.module_statuses = license
                .modules
                .iter()
                .map(|module| (module.clone(), "destroyed".to_string()))
                .collect();
        }
        if license.order_id.is_some() {
            insert_license_into_connection(&transaction, &license)
                .map_err(|error| error.to_string())?;
        } else {
            insert_legacy_license_into_connection(&transaction, &license)
                .map_err(|error| error.to_string())?;
        }
    }
    transaction.commit().map_err(|error| error.to_string())?;
    licensing::initialize(&mut connection)?;
    backfill_audit_log_customers(&connection).map_err(|error| error.to_string())?;
    Ok(())
}

fn ensure_column(
    connection: &Connection,
    table: &str,
    column: &str,
    alter_sql: &str,
) -> Result<(), String> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| error.to_string())?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    drop(statement);
    if !columns.iter().any(|existing| existing == column) {
        connection
            .execute(alter_sql, [])
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn remove_persona_schema(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch("DROP TABLE IF EXISTS operation_personas;")
        .map_err(|error| error.to_string())?;
    let mut statement = connection
        .prepare("PRAGMA table_info(ai_employee_products)")
        .map_err(|error| error.to_string())?;
    let has_persona_id = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?
        .iter()
        .any(|column| column == "persona_id");
    drop(statement);
    if has_persona_id {
        connection
            .execute_batch("ALTER TABLE ai_employee_products DROP COLUMN persona_id;")
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn remove_skill_legacy_schema(connection: &Connection) -> Result<(), String> {
    let mut statement = connection
        .prepare("PRAGMA table_info(operation_skills)")
        .map_err(|error| error.to_string())?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    drop(statement);
    for column in ["is_system", "requires_confirmation"] {
        if columns.iter().any(|existing| existing == column) {
            connection
                .execute_batch(&format!(
                    "ALTER TABLE operation_skills DROP COLUMN {column};"
                ))
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn migrate_license_relation_schema(connection: &mut Connection) -> Result<(), String> {
    let mut statement = connection
        .prepare("PRAGMA table_info(licenses)")
        .map_err(|error| error.to_string())?;
    let columns = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(1)?, row.get::<_, i64>(3)?))
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    drop(statement);
    let order_id_not_null = columns
        .iter()
        .find(|(name, _)| name == "order_id")
        .is_some_and(|(_, not_null)| *not_null != 0);
    let has_snapshot = columns
        .iter()
        .any(|(name, _)| name == "customer_name_snapshot");
    let has_linked_foreign_key = connection
        .prepare("PRAGMA foreign_key_list(licenses)")
        .and_then(|mut statement| {
            statement
                .query_map([], |row| row.get::<_, String>(2))?
                .collect::<Result<Vec<_>, _>>()
        })
        .map(|tables| tables.iter().any(|table| table == "orders"))
        .unwrap_or(false);
    if order_id_not_null && has_snapshot && has_linked_foreign_key {
        return Ok(());
    }

    connection
        .execute_batch(
            "PRAGMA foreign_keys = OFF;
             ALTER TABLE licenses RENAME TO licenses_relation_migration;
             CREATE TABLE licenses (
                license_id TEXT PRIMARY KEY NOT NULL,
                license TEXT NOT NULL,
                subject TEXT NOT NULL,
                customer_name_snapshot TEXT NOT NULL DEFAULT '',
                modules_json TEXT NOT NULL,
                issued_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                module_expires_at_json TEXT NOT NULL,
                ai_employees_json TEXT NOT NULL DEFAULT '[]',
                platform_status TEXT NOT NULL,
                module_statuses_json TEXT NOT NULL,
                order_id TEXT NOT NULL UNIQUE,
                linkage_status TEXT NOT NULL DEFAULT 'linked',
                FOREIGN KEY(order_id) REFERENCES orders(order_id)
             );",
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute(
            "INSERT INTO licenses (
                license_id, license, subject, customer_name_snapshot, modules_json,
                issued_at, expires_at, module_expires_at_json, ai_employees_json,
                platform_status, module_statuses_json, order_id, linkage_status
             )
             SELECT source.license_id, source.license, source.subject,
                    COALESCE(customers.name, source.subject), source.modules_json,
                    source.issued_at, source.expires_at, source.module_expires_at_json,
                    source.ai_employees_json, source.platform_status, source.module_statuses_json,
                    source.order_id, 'linked'
             FROM licenses_relation_migration source
             JOIN orders ON orders.order_id = source.order_id
             LEFT JOIN customers ON customers.customer_id = orders.customer_id
             WHERE source.order_id IS NOT NULL
               AND NOT EXISTS (
                 SELECT 1 FROM licenses_relation_migration newer
                 WHERE newer.order_id = source.order_id
                   AND (newer.issued_at > source.issued_at
                        OR (newer.issued_at = source.issued_at AND newer.license_id > source.license_id))
               )",
            [],
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute(
            "INSERT OR IGNORE INTO legacy_licenses (
                license_id, license, subject, customer_name_snapshot, modules_json,
                issued_at, expires_at, module_expires_at_json, ai_employees_json,
                platform_status, module_statuses_json, order_id, linkage_status
             )
             SELECT source.license_id, source.license, source.subject, source.subject,
                    source.modules_json, source.issued_at, source.expires_at,
                    source.module_expires_at_json, source.ai_employees_json,
                    source.platform_status, source.module_statuses_json,
                    source.order_id, 'legacy_unlinked'
             FROM licenses_relation_migration source
             WHERE NOT EXISTS (SELECT 1 FROM licenses linked WHERE linked.license_id = source.license_id)",
            [],
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute_batch("DROP TABLE licenses_relation_migration; PRAGMA foreign_keys = ON;")
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn insert_license_into_connection(
    connection: &Connection,
    license: &LicenseRecord,
) -> rusqlite::Result<()> {
    connection.execute(
        "INSERT INTO licenses (
            license_id, license, subject, customer_name_snapshot, modules_json, issued_at, expires_at,
            module_expires_at_json, ai_employees_json, platform_status, module_statuses_json, order_id, linkage_status
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            license.license_id,
            license.license,
            license.subject,
            license.customer_name_snapshot,
            serde_json::to_string(&license.modules).expect("modules are serializable"),
            license.issued_at,
            license.expires_at,
            serde_json::to_string(&license.module_expires_at)
                .expect("module expiries are serializable"),
            serde_json::to_string(&license.ai_employees)
                .expect("AI employee entitlements are serializable"),
            license.platform_status,
            serde_json::to_string(&license.module_statuses)
                .expect("module statuses are serializable"),
            license.order_id,
            "linked",
        ],
    )?;
    Ok(())
}

fn insert_legacy_license_into_connection(
    connection: &Connection,
    license: &LicenseRecord,
) -> rusqlite::Result<()> {
    connection.execute(
        "INSERT OR IGNORE INTO legacy_licenses (
            license_id, license, subject, customer_name_snapshot, modules_json, issued_at, expires_at,
            module_expires_at_json, ai_employees_json, platform_status, module_statuses_json, order_id, linkage_status
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'legacy_unlinked')",
        params![
            license.license_id,
            license.license,
            license.subject,
            if license.customer_name_snapshot.is_empty() { &license.subject } else { &license.customer_name_snapshot },
            serde_json::to_string(&license.modules).expect("modules are serializable"),
            license.issued_at,
            license.expires_at,
            serde_json::to_string(&license.module_expires_at).expect("module expiries are serializable"),
            serde_json::to_string(&license.ai_employees).expect("AI employee entitlements are serializable"),
            license.platform_status,
            serde_json::to_string(&license.module_statuses).expect("module statuses are serializable"),
            license.order_id,
        ],
    )?;
    Ok(())
}

fn list_license_records(database_path: &PathBuf) -> rusqlite::Result<Vec<LicenseRecord>> {
    let connection = Connection::open(database_path)?;
    let mut statement = connection.prepare(
        "SELECT license_id, license, subject, customer_name_snapshot, modules_json, issued_at, expires_at,
                module_expires_at_json, ai_employees_json, platform_status, module_statuses_json, order_id, linkage_status
         FROM licenses
         UNION ALL
         SELECT license_id, license, subject, customer_name_snapshot, modules_json, issued_at, expires_at,
                module_expires_at_json, ai_employees_json, platform_status, module_statuses_json, order_id, linkage_status
         FROM legacy_licenses
         ORDER BY issued_at DESC",
    )?;
    statement.query_map([], license_record_from_row)?.collect()
}

fn get_license_record(
    database_path: &PathBuf,
    license_id: &str,
) -> rusqlite::Result<Option<LicenseRecord>> {
    let connection = Connection::open(database_path)?;
    connection
        .query_row(
            "SELECT license_id, license, subject, customer_name_snapshot, modules_json, issued_at, expires_at,
                    module_expires_at_json, ai_employees_json, platform_status, module_statuses_json, order_id, linkage_status
             FROM licenses WHERE license_id = ?
             UNION ALL
             SELECT license_id, license, subject, customer_name_snapshot, modules_json, issued_at, expires_at,
                    module_expires_at_json, ai_employees_json, platform_status, module_statuses_json, order_id, linkage_status
             FROM legacy_licenses WHERE license_id = ?",
            params![license_id, license_id],
            license_record_from_row,
        )
        .optional()
}

// Activating the latest signature replaces every older signature for this subject.
// The transaction prevents two platform instances from leaving multiple active licenses.
fn activate_latest_license_record(
    database_path: &PathBuf,
    license: &LicenseRecord,
) -> rusqlite::Result<bool> {
    let mut connection = Connection::open(database_path)?;
    let transaction = connection.transaction()?;
    let latest_license_id = licensing::latest(&transaction,&license.subject)?.map(|v|v.license_id)
        .or(transaction.query_row("SELECT license_id FROM licenses WHERE subject=? AND linkage_status='linked' AND platform_status<>'destroyed' ORDER BY issued_at DESC,license_id DESC LIMIT 1",[&license.subject],|r|r.get::<_,String>(0)).optional()?);
    if latest_license_id.as_deref() != Some(license.license_id.as_str()) {
        return Ok(false);
    }

    let updated = transaction.execute(
        "UPDATE licenses SET platform_status = ?, module_statuses_json = ?
         WHERE license_id = ? AND platform_status <> 'destroyed'",
        params![
            license.platform_status,
            serde_json::to_string(&license.module_statuses)
                .expect("module statuses are serializable"),
            license.license_id,
        ],
    )?;
    if updated != 1 {
        return Ok(false);
    }

    // Historical signatures remain immutable. Supersession belongs to authorization versions.
    let mapped:bool=transaction.query_row("SELECT EXISTS(SELECT 1 FROM authorization_versions WHERE license_id=?)",[&license.license_id],|r|r.get(0))?;
    if mapped && !licensing::activate(&transaction,license)? {return Ok(false);}
    transaction.commit()?;
    Ok(true)
}

fn license_record_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LicenseRecord> {
    let modules_json: String = row.get(4)?;
    let module_expires_at_json: String = row.get(7)?;
    let ai_employees_json: String = row.get(8)?;
    let module_statuses_json: String = row.get(10)?;
    Ok(LicenseRecord {
        license_id: row.get(0)?,
        license: row.get(1)?,
        subject: row.get(2)?,
        customer_name_snapshot: row.get(3)?,
        order_id: row.get(11)?,
        linkage_status: row.get(12).unwrap_or_else(|_| default_linked_status()),
        modules: serde_json::from_str(&modules_json).unwrap_or_default(),
        issued_at: row.get(5)?,
        expires_at: row.get(6)?,
        module_expires_at: serde_json::from_str(&module_expires_at_json).unwrap_or_default(),
        ai_employees: serde_json::from_str(&ai_employees_json).unwrap_or_default(),
        platform_status: row.get(9).unwrap_or_else(|_| default_unactivated_status()),
        module_statuses: serde_json::from_str(&module_statuses_json).unwrap_or_default(),
    })
}

fn load_revoked(path: &PathBuf) -> HashSet<String> {
    std::fs::read(path)
        .ok()
        .and_then(|content| serde_json::from_slice(&content).ok())
        .unwrap_or_default()
}

fn load_licenses(path: &PathBuf) -> Vec<LicenseRecord> {
    std::fs::read(path)
        .ok()
        .and_then(|content| serde_json::from_slice(&content).ok())
        .unwrap_or_default()
}

fn required_env(name: &str) -> String {
    if let Ok(value) = std::env::var(name) {
        return value;
    }

    let local_secret = match name {
        "LICENSE_SIGNING_PRIVATE_KEY_PEM" => Some("../secrets/private.pem"),
        "LICENSE_SIGNING_PUBLIC_KEY_PEM" => Some("../secrets/public.pem"),
        _ => None,
    };

    local_secret
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| {
            panic!("{name} must be configured, or its local development secret must exist")
        })
}

fn success<T: Serialize>(message: impl Into<String>, data: T) -> Envelope<T> {
    Envelope {
        code: 0,
        message: message.into(),
        data: Some(data),
    }
}
fn error(status: StatusCode, message: impl Into<String>) -> (StatusCode, Json<Envelope<()>>) {
    (
        status,
        Json(Envelope {
            code: status.as_u16() as i32,
            message: message.into(),
            data: None,
        }),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn new_customer_requires_unique_nonempty_name_and_phone() {
        use super::*;
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch(
            "CREATE TABLE customers(name TEXT, contact_phone TEXT, status TEXT);
             INSERT INTO customers VALUES (' 已有客户 ', ' 13800000000 ', 'inactive');"
        ).unwrap();
        for (name, phone, status, message) in [
            (" ", "13900000000", StatusCode::BAD_REQUEST, "客户名称不能为空"),
            ("新客户", " ", StatusCode::BAD_REQUEST, "联系电话（手机号）不能为空"),
            (" 已有客户 ", "13900000000", StatusCode::CONFLICT, "客户名称已存在，请勿重复创建"),
            ("新客户", " 13800000000 ", StatusCode::CONFLICT, "联系电话（手机号）已存在，请勿重复创建"),
        ] {
            let failure = validate_new_customer(&connection, name, phone).unwrap_err();
            assert_eq!(failure.0, status);
            assert_eq!(failure.1.0.message, message);
        }
        assert!(validate_new_customer(&connection, "新客户", "13900000000").is_ok());
    }

    use super::*;

    #[test]
    fn migrates_environment_configuration_and_preserves_package_values() {
        let root = std::env::temp_dir().join(format!("skill-env-db-{}", Uuid::new_v4()));
        let database = root.join("catalog.sqlite");
        let missing = root.join("missing.json");
        let initialize = || {
            initialize_database(
                &database,
                &missing,
                &missing,
                "admin",
                Some("test-only-password"),
            )
        };
        initialize().unwrap();
        {
            let connection = Connection::open(&database).unwrap();
            connection.execute_batch("ALTER TABLE operation_skills DROP COLUMN environment_values_json;
                INSERT INTO operation_skills (id,title,instructions,created_at,updated_at,component_library_api_url)
                VALUES ('generic','Generic','os.getenv(''CUSTOM_URL'')',1,1,'http://wrong.example');").unwrap();
        }
        initialize().unwrap();
        initialize().unwrap();
        {
            let connection = Connection::open(&database).unwrap();
            let skill = get_operation_skill(&connection, "generic")
                .unwrap()
                .unwrap();
            assert_eq!(skill.environment_variables[0].name, "CUSTOM_URL");
            assert!(skill.environment_values.is_empty()); // No legacy address leaking into unrelated skills.
            let request: SaveOperationSkillRequest = serde_json::from_value(serde_json::json!({
                "id":"generic", "title":"Generic", "instructions":"os.getenv('CUSTOM_URL')",
                "environmentValues":{"CUSTOM_URL":"https://example.com"}
            }))
            .unwrap();
            let values = request.environment_values.unwrap();
            skill_environment::validate_values(&skill.environment_variables, &values).unwrap();
            connection
                .execute(
                    "UPDATE operation_skills SET environment_values_json=? WHERE id=?",
                    params![serde_json::to_string(&values).unwrap(), request.id],
                )
                .unwrap();
            let skill = get_operation_skill(&connection, "generic")
                .unwrap()
                .unwrap();
            let snapshot = serde_json::to_value(OperationSkillSnapshot::from(skill)).unwrap();
            assert_eq!(
                snapshot["environmentValues"]["CUSTOM_URL"],
                "https://example.com"
            );
            assert!(snapshot.get("componentLibraryApiUrl").is_none());
            connection.execute("UPDATE operation_skills SET instructions='no environment dependencies' WHERE id='generic'", []).unwrap();
            let updated = get_operation_skill(&connection, "generic")
                .unwrap()
                .unwrap();
            assert!(updated.environment_variables.is_empty());
            assert!(updated.environment_values.is_empty());
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn employee_catalog_contains_explicit_design_patch_capability() {
        let tools = platform_tool_catalog();
        let patch = tools
            .iter()
            .find(|tool| tool.id == "yaya_apply_design_patch")
            .unwrap();
        assert_eq!(patch.risk_level, "write");
        assert!(patch.description.contains("确认"));
        let ids: std::collections::HashSet<_> = tools.iter().map(|tool| &tool.id).collect();
        assert_eq!(ids.len(), tools.len());
    }

    #[test]
    fn employee_catalog_exposes_explicit_local_execution_capabilities() {
        let tools=platform_tool_catalog();
        for id in ["skill_run_script","workspace_execute_code"] {
            let tool=tools.iter().find(|tool|tool.id==id).unwrap();
            assert_eq!(tool.risk_level,"execution");
            assert!(tool.description.contains("账户"));
        }
    }

    #[test]
    fn removes_legacy_skill_configuration_columns() {
        let connection = Connection::open_in_memory().expect("create database");
        connection
            .execute_batch(
                "CREATE TABLE operation_skills (
                    id TEXT PRIMARY KEY,
                    is_system INTEGER NOT NULL DEFAULT 0,
                    requires_confirmation INTEGER NOT NULL DEFAULT 0
                 );",
            )
            .expect("create legacy skill table");

        remove_skill_legacy_schema(&connection).expect("remove legacy skill columns");
        let columns = connection
            .prepare("PRAGMA table_info(operation_skills)")
            .expect("inspect skill schema")
            .query_map([], |row| row.get::<_, String>(1))
            .expect("query skill columns")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect skill columns");
        assert_eq!(columns, vec!["id"]);
    }

    #[test]
    fn prevents_deleting_skill_referenced_by_ai_employee() {
        let connection = Connection::open_in_memory().expect("create database");
        connection
            .execute_batch(
                "CREATE TABLE ai_employee_products (
                    id TEXT PRIMARY KEY,
                    title TEXT NOT NULL,
                    description TEXT NOT NULL,
                    category TEXT NOT NULL,
                    price_cents INTEGER NOT NULL,
                    billing_cycle TEXT NOT NULL,
                    version TEXT NOT NULL,
                    package_version TEXT NOT NULL,
                    skill_ids_json TEXT NOT NULL,
                    system_prompt TEXT NOT NULL,
                    allow_network INTEGER NOT NULL,
                    allowed_tools_json TEXT NOT NULL,
                    application_ids_json TEXT NOT NULL,
                    avatar_url TEXT,
                    status TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                 );
                 INSERT INTO ai_employee_products VALUES (
                    'emp_1', '报价助手', '', '通用', 0, 'year', '1.0.0', '1.0.0',
                    '[\"skill_1\"]', '', 0, '[]', '[]', NULL, 'active', 0, 0
                 );",
            )
            .expect("seed employee skill reference");

        let eligibility = operation_skill_deletion_eligibility(&connection, "skill_1")
            .expect("check skill deletion eligibility");
        assert!(!eligibility.can_delete);
        assert_eq!(eligibility.blockers.len(), 1);
        assert_eq!(eligibility.blockers[0].employee_id, "emp_1");
        assert!(
            operation_skill_deletion_eligibility(&connection, "skill_other")
                .expect("check unreferenced skill")
                .can_delete
        );
    }

    #[test]
    fn resource_versions_roll_over_each_decimal_digit() {
        assert_eq!(next_patch_version("1.0.0"), "1.0.1");
        assert_eq!(next_patch_version("1.0.9"), "1.1.0");
        assert_eq!(next_patch_version("1.9.9"), "2.0.0");
    }

    #[test]
    fn skill_changes_increment_only_bound_employee_versions() {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE ai_employee_products (
            id TEXT PRIMARY KEY,title TEXT,description TEXT,category TEXT,price_cents INTEGER,
            billing_cycle TEXT,version TEXT,skill_ids_json TEXT,system_prompt TEXT,
            allow_network INTEGER,allowed_tools_json TEXT,application_ids_json TEXT,
            avatar_url TEXT,status TEXT,created_at INTEGER,updated_at INTEGER);
            INSERT INTO ai_employee_products VALUES
            ('bound','BOM','','',0,'year','1.1.7','[\"bom\"]','',0,'[]','[]',NULL,'active',0,0),
            ('other','Other','','',0,'year','1.0.0','[]','',0,'[]','[]',NULL,'active',0,0);").unwrap();
        bump_employee_versions_for_skill(&connection, "bom", 42).unwrap();
        let bound = get_ai_employee_product(&connection, "bound").unwrap().unwrap();
        assert_eq!(bound.version, "1.1.8");
        assert_eq!(bound.updated_at, 42);
        assert_eq!(get_ai_employee_product(&connection, "other").unwrap().unwrap().version, "1.0.0");
        assert!(serde_json::to_value(bound).unwrap().get("packageVersion").is_none());
    }

    #[test]
    fn audit_response_redacts_sensitive_fields() {
        let body = br#"{"message":"ok","data":{"license":"signed-token","licenseId":"lic_1","apiKey":"secret","environmentValues":{"CUSTOM_CREDENTIAL":"do-not-log"}}}"#;
        let sanitized = sanitize_response_body(body).expect("JSON response");
        assert!(!sanitized.contains("signed-token"));
        assert!(!sanitized.contains("\"secret\""));
        assert!(!sanitized.contains("do-not-log"));
        assert!(sanitized.contains("lic_1"));
        assert!(sanitized.contains("[REDACTED]"));
    }

    #[test]
    fn audit_ignores_operation_frontend_requests() {
        let mut headers = HeaderMap::new();
        headers.insert("origin", HeaderValue::from_static("http://localhost:8778"));
        assert!(is_operation_frontend_request(&headers, "/api/orders"));
        assert!(is_operation_frontend_request(
            &HeaderMap::new(),
            "/api/logs"
        ));
        assert!(!is_operation_frontend_request(
            &HeaderMap::new(),
            "/healthz"
        ));
    }

    #[test]
    fn audit_customer_resolution_uses_license_snapshots_and_order_relations() {
        let connection = Connection::open_in_memory().expect("create database");
        connection
            .execute_batch(
                "CREATE TABLE customers (customer_id TEXT PRIMARY KEY, name TEXT NOT NULL);
                 CREATE TABLE orders (order_id TEXT PRIMARY KEY, customer_id TEXT NOT NULL);
                 CREATE TABLE licenses (
                    license_id TEXT PRIMARY KEY,
                    subject TEXT NOT NULL,
                    customer_name_snapshot TEXT NOT NULL
                 );
                 INSERT INTO customers VALUES ('cus_1', '当前客户名');
                 INSERT INTO orders VALUES ('ord_1', 'cus_1');
                 INSERT INTO licenses VALUES ('lic_1', 'cus_1', '签发时客户名');",
            )
            .expect("seed database");

        let license_customer =
            resolve_audit_log_customer(&connection, Some("/api/licenses/lic_1/status"))
                .expect("resolve license")
                .expect("license customer");
        assert_eq!(license_customer.customer_id, "cus_1");
        assert_eq!(license_customer.customer_name, "签发时客户名");

        let order_customer =
            resolve_audit_log_customer(&connection, Some("/api/orders/ord_1/payment"))
                .expect("resolve order")
                .expect("order customer");
        assert_eq!(order_customer.customer_name, "当前客户名");
        assert!(
            resolve_audit_log_customer(&connection, Some("/healthz"))
                .expect("resolve unrelated path")
                .is_none()
        );
    }

    #[test]
    fn migrates_unlinked_licenses_and_enforces_order_relation() {
        let database_path = std::env::temp_dir().join(format!(
            "yaya-operation-center-license-relation-{}.sqlite3",
            Uuid::new_v4().simple()
        ));
        let missing_legacy_path = database_path.with_extension("missing.json");
        {
            let connection = Connection::open(&database_path).expect("create legacy database");
            connection
                .execute_batch(
                    "CREATE TABLE licenses (
                        license_id TEXT PRIMARY KEY NOT NULL,
                        license TEXT NOT NULL,
                        subject TEXT NOT NULL,
                        modules_json TEXT NOT NULL,
                        issued_at INTEGER NOT NULL,
                        expires_at INTEGER NOT NULL,
                        module_expires_at_json TEXT NOT NULL,
                        ai_employees_json TEXT NOT NULL DEFAULT '[]',
                        platform_status TEXT NOT NULL,
                        module_statuses_json TEXT NOT NULL,
                        order_id TEXT
                     );
                     INSERT INTO licenses VALUES (
                        'lic_legacy', 'token', '历史客户', '[\"platform\"]', 1, 2,
                        '{}', '[]', 'unactivated', '{}', NULL
                     );",
                )
                .expect("seed legacy license");
        }

        initialize_database(
            &database_path,
            &missing_legacy_path,
            &missing_legacy_path,
            "admin",
            Some("development-only-password"),
        )
        .expect("migrate database");
        let connection = Connection::open(&database_path).expect("open migrated database");
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .expect("enable foreign keys");
        let linkage_status: String = connection
            .query_row(
                "SELECT linkage_status FROM legacy_licenses WHERE license_id = 'lic_legacy'",
                [],
                |row| row.get(0),
            )
            .expect("legacy license remains visible");
        assert_eq!(linkage_status, "legacy_unlinked");

        let order_id_not_null: i64 = connection
            .query_row(
                "SELECT \"notnull\" FROM pragma_table_info('licenses') WHERE name = 'order_id'",
                [],
                |row| row.get(0),
            )
            .expect("order_id schema");
        assert_eq!(order_id_not_null, 1);
        let foreign_key_table: String = connection
            .query_row(
                "SELECT \"table\" FROM pragma_foreign_key_list('licenses') WHERE \"from\" = 'order_id'",
                [],
                |row| row.get(0),
            )
            .expect("order foreign key");
        assert_eq!(foreign_key_table, "orders");
        let table_sql: String = connection
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'licenses'",
                [],
                |row| row.get(0),
            )
            .expect("license schema sql");
        assert!(table_sql.contains("order_id TEXT NOT NULL UNIQUE"));
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM licenses", [], |row| row
                    .get::<_, i64>(0))
                .expect("strict license count"),
            0
        );

        drop(connection);
        let _ = std::fs::remove_file(&database_path);
        let _ = std::fs::remove_file(database_path.with_extension("sqlite3-wal"));
        let _ = std::fs::remove_file(database_path.with_extension("sqlite3-shm"));
    }

    #[test]
    fn activating_latest_license_preserves_older_signature_history() {
        let database_path = std::env::temp_dir().join(format!(
            "yaya-operation-center-license-activation-{}.sqlite3",
            Uuid::new_v4().simple()
        ));
        let connection = Connection::open(&database_path).expect("create database");
        connection
            .execute_batch(
                "CREATE TABLE licenses (
                    license_id TEXT PRIMARY KEY NOT NULL,
                    license TEXT NOT NULL,
                    subject TEXT NOT NULL,
                    customer_name_snapshot TEXT NOT NULL DEFAULT '',
                    modules_json TEXT NOT NULL,
                    issued_at INTEGER NOT NULL,
                    expires_at INTEGER NOT NULL,
                    module_expires_at_json TEXT NOT NULL,
                    ai_employees_json TEXT NOT NULL DEFAULT '[]',
                    platform_status TEXT NOT NULL,
                    module_statuses_json TEXT NOT NULL,
                    order_id TEXT NOT NULL UNIQUE,
                    linkage_status TEXT NOT NULL DEFAULT 'linked'
                );",
            )
            .expect("create licenses table");
        let make_license =
            |license_id: &str, order_id: &str, issued_at, status: &str| LicenseRecord {
                license: "signed-token".to_string(),
                license_id: license_id.to_string(),
                subject: "cus_1".to_string(),
                customer_name_snapshot: "客户一".to_string(),
                order_id: Some(order_id.to_string()),
                linkage_status: "linked".to_string(),
                modules: vec!["platform".to_string()],
                issued_at,
                expires_at: 9_999_999_999,
                module_expires_at: HashMap::new(),
                ai_employees: Vec::new(),
                platform_status: status.to_string(),
                module_statuses: HashMap::from([("platform".to_string(), status.to_string())]),
            };
        let old = make_license("lic_old", "ord_old", 1, "running");
        let mut latest = make_license("lic_latest", "ord_latest", 2, "unactivated");
        insert_license_into_connection(&connection, &old).expect("insert old license");
        insert_license_into_connection(&connection, &latest).expect("insert latest license");
        connection.execute_batch("CREATE TABLE customers(customer_id TEXT PRIMARY KEY);INSERT INTO customers VALUES('cus_1');CREATE TABLE orders(order_id TEXT PRIMARY KEY);INSERT INTO orders VALUES('ord_old'),('ord_latest');").unwrap();
        let mut connection=connection;
        licensing::initialize(&mut connection).unwrap();
        drop(connection);

        latest.platform_status = "running".to_string();
        latest
            .module_statuses
            .insert("platform".to_string(), "running".to_string());
        assert!(activate_latest_license_record(&database_path, &latest).expect("activate latest"));
        assert!(!activate_latest_license_record(&database_path, &old).expect("reject old"));

        let connection = Connection::open(&database_path).expect("open database");
        let old_status: String = connection
            .query_row(
                "SELECT platform_status FROM licenses WHERE license_id = 'lic_old'",
                [],
                |row| row.get(0),
            )
            .expect("read old status");
        let latest_status: String = connection
            .query_row(
                "SELECT platform_status FROM licenses WHERE license_id = 'lic_latest'",
                [],
                |row| row.get(0),
            )
            .expect("read latest status");
        assert_eq!(old_status, "running");
        assert_eq!(latest_status, "running");

        drop(connection);
        let _ = std::fs::remove_file(&database_path);
        let _ = std::fs::remove_file(database_path.with_extension("sqlite3-wal"));
        let _ = std::fs::remove_file(database_path.with_extension("sqlite3-shm"));
    }
}

#[cfg(test)]
mod application_market_tests;

#[cfg(test)]
mod application_commerce_tests;
