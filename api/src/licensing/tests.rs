use super::*;
use http_body_util::BodyExt;
use tower::ServiceExt;

struct Fixture {root:PathBuf,state:AppState,headers:HeaderMap}
impl Fixture {
 fn new()->Self {
  let root=std::env::temp_dir().join(format!("licensing-business-{}",Uuid::new_v4()));fs::create_dir(&root).unwrap();
  let path=root.join("test.sqlite");
  initialize_database(&path,&root.join("none"),&root.join("none"),"admin",Some("test-password")).unwrap();
  ops::initialize(&path).unwrap();
  let db=ops::db::open(&path).unwrap();
  db.execute("INSERT INTO customers(customer_id,name,contact_name,contact_phone,contact_email,status,notes,created_at) VALUES('c','客户','张三','13800000000','','active','',1)",[]).unwrap();
  let (token,_)=staff_sessions::login(&db,LoginRequest{username:"admin".into(),password:"test-password".into()},Utc::now().timestamp_millis()).unwrap();
  let mut headers=HeaderMap::new();headers.insert("cookie",HeaderValue::from_str(&format!("license_center_session={token}")).unwrap());headers.insert("origin",HeaderValue::from_static("http://localhost:8778"));
  let state=AppState{signing_key:Arc::new(EncodingKey::from_rsa_pem(include_bytes!("../../tests/fixtures/licensing-test-private.pem")).unwrap()),verification_key:Arc::new(DecodingKey::from_rsa_pem(include_bytes!("../../tests/fixtures/licensing-test-public.pem")).unwrap()),database_path:path,sessions:Arc::new(RwLock::new(HashMap::new())),ops:Arc::new(ops::OpsState::new(false,root.join("none")))};
  Self{root,state,headers}
 }
 fn order(&self,id:&str,items:serde_json::Value) {
  let db=ops::db::open(&self.state.database_path).unwrap();
  db.execute("INSERT INTO orders(order_id,order_no,customer_id,items_json,total_amount_cents,status,created_at,deployment_type,notes) VALUES(?,?,'c',?,100,'paid',?,'saas','')",params![id,id,items.to_string(),Utc::now().timestamp()]).unwrap();
 }
 async fn issue(&self,id:&str)->Result<IssueLicenseResponse,Failure> {
  issue_license(State(self.state.clone()),self.headers.clone(),Json(IssueLicenseRequest{order_id:id.into()})).await.map(|v|v.0.data.unwrap())
 }
}
impl Drop for Fixture{fn drop(&mut self){let _=fs::remove_dir_all(&self.root);}}
fn line(kind:&str,reference:&str,days:u32)->serde_json::Value {serde_json::json!({"kind":kind,"referenceId":reference,"title":reference,"quantity":1,"unitPriceCents":100,"entitlementDays":days})}

#[tokio::test]
async fn paid_orders_accumulate_renew_and_replay_without_losing_entitlements() {
 let f=Fixture::new();
 f.order("first",serde_json::json!([line("platform","platform",365),line("module","communication",60)]));
 let first=f.issue("first").await.unwrap();
 let auth=view(&ops::db::open(&f.state.database_path).unwrap(),"c").unwrap()["authorizationId"].clone();
 let base=verify_license(&first.license,&f.state).unwrap();
 let mut first_record=get_license_record(&f.state.database_path,&first.license_id).unwrap().unwrap();first_record.platform_status="running".into();
 assert!(activate_latest_license_record(&f.state.database_path,&first_record).unwrap());
 f.order("addon",serde_json::json!([line("module","new-module",30)]));
 let addon=f.issue("addon").await.unwrap();let claims=verify_license(&addon.license,&f.state).unwrap();
 assert_eq!(claims.exp,base.exp);assert_eq!(claims.module_expires_at["communication"],base.module_expires_at["communication"]);assert!(claims.modules.contains(&"new-module".to_string()));
 assert_eq!(f.issue("addon").await.unwrap().license_id,addon.license_id);
 f.order("renew",serde_json::json!([line("platform","platform",365)]));let renewed=f.issue("renew").await.unwrap();let claims=verify_license(&renewed.license,&f.state).unwrap();assert_eq!(claims.exp,base.exp+365*86400);assert!(claims.modules.contains(&"new-module".to_string()));
 let db=ops::db::open(&f.state.database_path).unwrap();let summary=view(&db,"c").unwrap();assert_eq!(summary["authorizationId"],auth);assert_eq!(summary["revision"],3);assert_eq!(summary["activeLicenseId"],first.license_id);assert_eq!(summary["latestLicenseId"],renewed.license_id);
 assert_eq!(get_license_record(&f.state.database_path,&first.license_id).unwrap().unwrap().platform_status,"running");
 let mut newest=get_license_record(&f.state.database_path,&renewed.license_id).unwrap().unwrap();newest.platform_status="running".into();assert!(activate_latest_license_record(&f.state.database_path,&newest).unwrap());assert!(!activate_latest_license_record(&f.state.database_path,&first_record).unwrap());
 let summary=view(&db,"c").unwrap();assert_eq!(summary["activeLicenseId"],renewed.license_id);assert_eq!(summary["versions"][2]["state"],"superseded");
 let (a,b)=tokio::join!(f.issue("renew"),f.issue("renew"));assert_eq!(a.unwrap().license_id,b.unwrap().license_id);
 initialize(&mut ops::db::open(&f.state.database_path).unwrap()).unwrap();assert_eq!(view(&db,"c").unwrap()["revision"],3);
 destroy_license_by_id(&f.state,&renewed.license_id).await.unwrap();assert!(latest(&db,"c").unwrap().is_none());assert_eq!(view(&db,"c").unwrap()["versions"][2]["state"],"revoked");
}
#[tokio::test]
async fn first_order_needs_platform_service_fulfillment_and_api_permissions() {
 let f=Fixture::new();f.order("addon",serde_json::json!([line("module","communication",30)]));assert_eq!(f.issue("addon").await.err().unwrap().0,StatusCode::BAD_REQUEST);
 let db=ops::db::open(&f.state.database_path).unwrap();assert_eq!(db.query_row("SELECT count(*) FROM licenses",[],|r|r.get::<_,i64>(0)).unwrap(),0);
 f.order("service",serde_json::json!([line("service","installation",1)]));assert!(service(State(f.state.clone()),f.headers.clone(),Path("service".into())).await.is_ok());assert!(service(State(f.state.clone()),f.headers.clone(),Path("service".into())).await.is_ok());
 assert_eq!(db.query_row("SELECT count(*) FROM customer_authorizations",[],|r|r.get::<_,i64>(0)).unwrap(),0);
 assert_eq!(extend(1000,2000,30).unwrap(),2000+30*86400);assert_eq!(extend(3000,2000,30).unwrap(),3000+30*86400);assert!(extend(0,0,0).is_err());
 let request=Request::builder().uri("/api/authorizations").body(Body::empty()).unwrap();let response=router().with_state(f.state.clone()).oneshot(request).await.unwrap();assert_eq!(response.status(),StatusCode::UNAUTHORIZED);let _=response.into_body().collect().await;
}
